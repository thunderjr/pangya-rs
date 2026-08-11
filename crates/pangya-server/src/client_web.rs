//! The HTTP listener a retail client needs before it will speak the game protocol.
//!
//! This is deliberately a listener of its own rather than a few extra routes on the admin
//! server. The admin surface carries health and metrics and stays on loopback; this surface has
//! to be reachable by whatever machine runs the client, so binding them together would publish
//! readiness and metrics to reach a patch manifest. Both still default to loopback and a
//! non-loopback bind still needs `--acknowledge-public-bind`.
//!
//! Everything served here is static for the lifetime of the process. The update list is built
//! once at startup, because building it means checksumming the whole client directory — a
//! little over 2.5 GiB for the U.S. series — and the directory does not change underneath a
//! running server. Building it before the listener binds also means a misconfigured client
//! directory fails at startup with an actionable error instead of at the client's first
//! request.

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use pangya_patch::ReleaseManifest;
use pangya_updater::{
    EntrySelection, Theme, UpdateListRegion, build_from_directory, encode_translation_catalog,
    extra_contents_xml,
};
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;

/// Maximum bytes served for one theme image.
///
/// The largest wallpaper in the U.S. client is a little over 600 KiB; four mebibytes leaves
/// headroom without letting a mistaken theme directory stream something huge to a client.
pub const MAX_THEME_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

/// Maximum bytes read for the translation catalog.
pub const MAX_TRANSLATION_BYTES: u64 = 4 * 1024 * 1024;
/// Incremental payloads are changed IFF tables, not archives; cap both one member and release.
const MAX_INCREMENTAL_MEMBER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_INCREMENTAL_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_INCREMENTAL_MANIFEST_BYTES: u64 = 1024 * 1024;
const INCREMENTAL_SIGNATURE_BYTES: u64 = 64;

/// Validated client web-service policy.
#[derive(Clone, Debug)]
pub struct ClientWebSettings {
    /// Address the client will be told to fetch theme content from, as `host:port`.
    pub advertise: std::net::SocketAddr,
    /// Region key for the update list.
    pub region: UpdateListRegion,
    /// Directory holding the client's PAK series.
    pub client_directory: std::path::PathBuf,
    /// Which files to list.
    pub entries: EntrySelection,
    /// Human-readable patch version reported to the client.
    pub patch_version: String,
    /// Numeric patch number reported to the client.
    pub patch_number: u32,
    /// Optional path to the operator's plaintext translation catalog XML.
    pub translation_catalog: Option<std::path::PathBuf>,
    /// Optional directory holding the theme images.
    pub theme_directory: Option<std::path::PathBuf>,
    /// Directory produced by pangya-patch-bundle; only metadata/signature/member payloads.
    pub incremental_release: Option<std::path::PathBuf>,
}

/// Failures while preparing the client web content.
#[derive(Debug, thiserror::Error)]
pub enum ClientWebError {
    /// The client directory could not be opened or listed.
    #[error("the configured client directory could not be read")]
    ClientDirectory,
    /// The update list could not be built.
    #[error("the client update list could not be built: {0}")]
    UpdateList(#[from] pangya_updater::UpdateListError),
    /// The theme directory could not be opened or listed.
    #[error("the configured theme directory could not be read")]
    ThemeDirectory,
    /// A theme image name was unusable.
    #[error("the theme directory holds a file name a client could not request: {0}")]
    Theme(#[from] pangya_updater::ThemeError),
    /// The translation catalog could not be read.
    #[error("the configured translation catalog could not be read")]
    TranslationCatalog,
    /// Incremental release directory is malformed or contains unavailable payloads.
    #[error("the configured incremental release could not be read")]
    IncrementalRelease,
}

/// Everything the routes serve, prepared once.
struct Prepared {
    update_list: Vec<u8>,
    /// Pre-serialized launcher manifest, minus the staleness flag which is computed per request.
    launcher_paks: Vec<LauncherPak>,
    /// What `prepare` observed for each listed archive, so a swap under a running server can be
    /// detected. This matters more than it looks: replacing a served file with `mv -f` leaves
    /// the held `File` pointing at the unlinked old inode, so the server goes on serving the old
    /// bytes with the old update list and nothing anywhere reports a problem.
    stat_snapshot: Vec<(String, u64, Option<std::time::SystemTime>)>,
    patch_version: String,
    patch_number: u32,
    client_directory_path: std::path::PathBuf,
    patch_files: HashMap<String, PatchFile>,
    translation: String,
    extra_contents: String,
    theme_document: String,
    theme_directory: Option<Dir>,
    incremental: Option<IncrementalRelease>,
}

struct IncrementalRelease {
    metadata: ReleaseManifest,
    manifest: Vec<u8>,
    signature: Vec<u8>,
    payloads: HashMap<String, Vec<u8>>,
}

/// One archive as the launcher sees it.
///
/// `pangya_crc` is the authority for *will the client start* — it is literally the `fcrc` the
/// client compares. `sha256` is the authority for *did I receive the right bytes*; a 32-bit
/// checksum is a compatibility signal, not an integrity boundary for a network transfer. A
/// launcher must satisfy size, CRC and digest before a download may touch a client directory.
#[derive(Clone, Debug, serde::Serialize)]
pub struct LauncherPak {
    /// File name, no directory component.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// PangYa checksum, in the signed form the retail document carries.
    pub pangya_crc: i32,
    /// SHA-256 of the same bytes, lowercase hex.
    pub sha256: String,
}

/// What `GET /launcher/v1/manifest` answers.
#[derive(Debug, serde::Serialize)]
struct LauncherManifest<'a> {
    manifest_version: u32,
    patch_version: &'a str,
    patch_number: u32,
    /// True when a listed archive no longer matches what startup observed. The launcher must
    /// refuse to patch against a stale manifest: the update list it would be validated against
    /// was built at startup and no longer describes what is on disk.
    stale: bool,
    paks: &'a [LauncherPak],
}

/// An allowlisted patch payload held open from startup so request text never becomes a path.
struct PatchFile {
    file: Arc<std::fs::File>,
    size: u64,
}

/// Shared handle for the router.
#[derive(Clone)]
pub struct ClientWebState(Arc<Prepared>);

/// Reads a file through a directory capability, refusing anything over `limit`.
///
/// The capacity hint is taken from the metadata only after the length has been checked against
/// the cap, so a large file cannot cause a large reservation before being rejected.
fn read_bounded(directory: &Dir, name: &str, limit: u64) -> Option<Vec<u8>> {
    let file = directory.open(name).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Classifies the client's theme images by their retail naming convention.
///
/// The U.S. client ships `extrares/` with three families: date-stamped notice images, `main_bg*`
/// lobby wallpapers, and `background_*`/`loading_background` loading wallpapers. Deriving the
/// theme from the directory rather than a config list means an operator points at the folder the
/// client already has and gets the same content the retail theme document named.
fn load_incremental(
    path: Option<&std::path::Path>,
) -> Result<Option<IncrementalRelease>, ClientWebError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let manifest_path = path.join("release-manifest.json");
    if std::fs::metadata(&manifest_path)
        .map_err(|_| ClientWebError::IncrementalRelease)?
        .len()
        > MAX_INCREMENTAL_MANIFEST_BYTES
    {
        return Err(ClientWebError::IncrementalRelease);
    }
    let manifest = std::fs::read(&manifest_path).map_err(|_| ClientWebError::IncrementalRelease)?;
    let parsed: ReleaseManifest =
        serde_json::from_slice(&manifest).map_err(|_| ClientWebError::IncrementalRelease)?;
    // Re-serialize through the canonical validator before serving anything.
    if parsed
        .canonical_json()
        .map_err(|_| ClientWebError::IncrementalRelease)?
        != manifest
    {
        return Err(ClientWebError::IncrementalRelease);
    }
    let signature_path = path.join("release-manifest.json.sig");
    if std::fs::metadata(&signature_path)
        .map_err(|_| ClientWebError::IncrementalRelease)?
        .len()
        != INCREMENTAL_SIGNATURE_BYTES
    {
        return Err(ClientWebError::IncrementalRelease);
    }
    let signature =
        std::fs::read(signature_path).map_err(|_| ClientWebError::IncrementalRelease)?;
    if signature.len() != INCREMENTAL_SIGNATURE_BYTES as usize {
        return Err(ClientWebError::IncrementalRelease);
    }
    let mut payloads = HashMap::new();
    let mut total = 0_u64;
    for member in parsed.members.clone() {
        if member.new_length > MAX_INCREMENTAL_MEMBER_BYTES {
            return Err(ClientWebError::IncrementalRelease);
        }
        total = total
            .checked_add(member.new_length)
            .filter(|n| *n <= MAX_INCREMENTAL_TOTAL_BYTES)
            .ok_or(ClientWebError::IncrementalRelease)?;
        let payload_path = path.join("payload").join(&member.name);
        let metadata =
            std::fs::metadata(&payload_path).map_err(|_| ClientWebError::IncrementalRelease)?;
        if !metadata.is_file() || metadata.len() != member.new_length {
            return Err(ClientWebError::IncrementalRelease);
        }
        let bytes = std::fs::read(payload_path).map_err(|_| ClientWebError::IncrementalRelease)?;
        payloads.insert(member.name, bytes);
    }
    Ok(Some(IncrementalRelease {
        metadata: parsed,
        manifest,
        signature,
        payloads,
    }))
}

fn classify_theme(names: &[String]) -> Theme {
    let mut theme = Theme::default();
    for name in names {
        let stem = name.strip_suffix(".jpg").unwrap_or(name);
        if stem.starts_with("main_bg") {
            theme.lobby_wallpapers.push(name.clone());
        } else if stem.starts_with("background_") || stem == "loading_background" {
            theme.loading_wallpapers.push(name.clone());
        } else if stem.starts_with(|character: char| character.is_ascii_digit()) {
            theme.notices.push(name.clone());
        }
    }
    theme.lobby_wallpapers.sort();
    theme.loading_wallpapers.sort();
    // Newest notice first, which is the order the client displays them in.
    theme.notices.sort_by(|left, right| right.cmp(left));
    theme
}

impl ClientWebState {
    /// Prepares every document the client will request.
    ///
    /// # Errors
    /// Returns [`ClientWebError`] when the client directory, theme directory, or translation
    /// catalog cannot be read, or when a theme file name could not be requested by a client.
    pub fn prepare(settings: &ClientWebSettings) -> Result<Self, ClientWebError> {
        let client_directory =
            Dir::open_ambient_dir(&settings.client_directory, ambient_authority())
                .map_err(|_| ClientWebError::ClientDirectory)?;
        let incremental = load_incremental(settings.incremental_release.as_deref())?;
        let mut update_list = build_from_directory(
            &client_directory,
            settings.entries,
            &settings.patch_version,
            settings.patch_number,
        )?;
        if let Some(release) = &incremental {
            let target = &release.metadata.target_pak;
            let entry = update_list
                .entries
                .iter_mut()
                .find(|entry| entry.name.eq_ignore_ascii_case(target))
                .ok_or(ClientWebError::IncrementalRelease)?;
            if entry.size != release.metadata.base_pak.size
                || entry.checksum != release.metadata.base_pak.pangya_crc
                || entry.sha256 != release.metadata.base_pak.sha256
            {
                return Err(ClientWebError::IncrementalRelease);
            }
            entry.size = release.metadata.result_pak.size;
            entry.checksum = release.metadata.result_pak.pangya_crc;
            entry.sha256 = release.metadata.result_pak.sha256.clone();
        }
        let mut patch_files = HashMap::with_capacity(update_list.entries.len());
        for entry in &update_list.entries {
            if incremental.as_ref().is_some_and(|release| {
                entry
                    .name
                    .eq_ignore_ascii_case(&release.metadata.target_pak)
            }) {
                continue;
            }
            let file = client_directory
                .open(&entry.name)
                .map_err(|_| ClientWebError::ClientDirectory)?;
            patch_files.insert(
                entry.name.clone(),
                PatchFile {
                    file: Arc::new(file.into_std()),
                    size: entry.size,
                },
            );
        }
        // Built before the document is encrypted, from the same `FileEntry` values the client
        // will be validated against. Computing these independently could drift; taking them from
        // one structure means launcher and client cannot disagree.
        let launcher_paks: Vec<LauncherPak> = update_list
            .entries
            .iter()
            .filter(|entry| {
                std::path::Path::new(&entry.name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pak"))
            })
            .map(|entry| LauncherPak {
                name: entry.name.clone(),
                size: entry.size,
                pangya_crc: entry.checksum,
                sha256: entry.sha256.clone(),
            })
            .collect();
        let stat_snapshot = launcher_paks
            .iter()
            .filter(|pak| {
                !incremental.as_ref().is_some_and(|release| {
                    pak.name.eq_ignore_ascii_case(&release.metadata.target_pak)
                })
            })
            .map(|pak| {
                let observed = client_directory.metadata(&pak.name).ok().map(|meta| {
                    (
                        meta.len(),
                        meta.modified()
                            .ok()
                            .map(cap_std::time::SystemTime::into_std),
                    )
                });
                let (size, modified) = observed.unwrap_or((0, None));
                (pak.name.clone(), size, modified)
            })
            .collect();

        let update_list = update_list.to_encrypted(settings.region.key());

        let base_url = format!(
            "http://{}/new/Service/S4_Patch/extracontents/default/",
            settings.advertise
        );
        let extra_contents = extra_contents_xml(&base_url)?;

        let (theme_document, theme_directory) = match &settings.theme_directory {
            Some(path) => {
                let directory = Dir::open_ambient_dir(path, ambient_authority())
                    .map_err(|_| ClientWebError::ThemeDirectory)?;
                let mut names = Vec::new();
                for entry in directory
                    .entries()
                    .map_err(|_| ClientWebError::ThemeDirectory)?
                {
                    let entry = entry.map_err(|_| ClientWebError::ThemeDirectory)?;
                    let metadata = entry
                        .metadata()
                        .map_err(|_| ClientWebError::ThemeDirectory)?;
                    if !metadata.is_file() {
                        continue;
                    }
                    if let Ok(name) = entry.file_name().into_string() {
                        names.push(name);
                    }
                }
                names.sort();
                let theme = classify_theme(&names);
                (theme.to_xml()?, Some(directory))
            }
            // An empty theme is a complete, valid document; the client accepts it and then
            // downloads nothing.
            None => (Theme::default().to_xml()?, None),
        };

        let translation = match &settings.translation_catalog {
            Some(path) => {
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new("."));
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(ClientWebError::TranslationCatalog)?;
                let directory = Dir::open_ambient_dir(parent, ambient_authority())
                    .map_err(|_| ClientWebError::TranslationCatalog)?;
                let bytes = read_bounded(&directory, name, MAX_TRANSLATION_BYTES)
                    .ok_or(ClientWebError::TranslationCatalog)?;
                encode_translation_catalog(&bytes)
            }
            // An empty body is what the client gets from a retail server with no overrides. It
            // is not a substitute for the catalog: without one the client falls back to its own
            // `.dat` strings, and any string it expects only from the service is missing.
            None => String::new(),
        };

        Ok(Self(Arc::new(Prepared {
            update_list,
            launcher_paks,
            stat_snapshot,
            patch_version: settings.patch_version.clone(),
            patch_number: settings.patch_number,
            client_directory_path: settings.client_directory.clone(),
            patch_files,
            translation,
            extra_contents,
            theme_document,
            theme_directory,
            incremental,
        })))
    }

    /// Returns the prepared update list, for tests and operator tooling.
    #[must_use]
    pub fn update_list_bytes(&self) -> &[u8] {
        &self.0.update_list
    }
}

/// Builds the client-facing router.
///
/// Every path is registered under all three prefixes the retail clients use, because which one
/// a build requests is a property of the build rather than of configuration.
pub fn client_web_router(state: ClientWebState) -> Router {
    const PREFIXES: [&str; 3] = [
        "/new/Service/S4_Patch",
        "/S4_Patch",
        "/pangya/season4/patch",
    ];
    // Registered outside the three retail prefixes so it can never shadow a path the client
    // asks for. Nothing retail requests this; it exists for the launcher.
    let mut router = Router::new()
        .route("/Translation/Read.aspx", get(translation))
        .route("/launcher/v1/manifest", get(launcher_manifest))
        .route(
            "/launcher/v2/release-manifest.json",
            get(incremental_manifest),
        )
        .route(
            "/launcher/v2/release-manifest.json.sig",
            get(incremental_signature),
        )
        .route("/launcher/v2/payload/{name}", get(incremental_payload));
    for prefix in PREFIXES {
        router = router
            .route(&format!("{prefix}/updatelist"), get(updatelist))
            .route(&format!("{prefix}/{{name}}"), get(patch_file))
            .route(
                &format!("{prefix}/extracontents/extracontents.xml"),
                get(extracontents),
            )
            .route(
                &format!("{prefix}/extracontents/default/pangya_default.xml"),
                get(theme_document),
            )
            .route(
                &format!("{prefix}/extracontents/default/{{name}}"),
                get(theme_image),
            );
    }
    router.fallback(client_web_not_found).with_state(state)
}

/// Answers the launcher's manifest.
///
/// Unauthenticated by necessity — the same audience and the same listener as the update list
/// this is derived from, and a player's launcher holds no credential. It publishes only what the
/// client is already told in the update list, plus a digest of bytes the same listener already
/// serves in full.
///
/// Staleness is re-stat'd per request rather than cached: it is two `metadata` calls per
/// archive, and the whole point is to notice a change made *after* startup.
async fn launcher_manifest(State(state): State<ClientWebState>) -> Response {
    let stale = state.0.stat_snapshot.iter().any(|(name, size, modified)| {
        let path = state.0.client_directory_path.join(name);
        match std::fs::metadata(&path) {
            Ok(current) => current.len() != *size || current.modified().ok() != *modified,
            // A listed archive that has vanished is at least as stale as one that changed.
            Err(_) => true,
        }
    });
    if stale {
        tracing::warn!(
            "a served client archive changed after startup; the update list no longer describes \
             the directory and the server needs restarting"
        );
    }
    let manifest = LauncherManifest {
        manifest_version: 1,
        patch_version: &state.0.patch_version,
        patch_number: state.0.patch_number,
        stale,
        paks: &state.0.launcher_paks,
    };
    match serde_json::to_vec(&manifest) {
        Ok(body) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn incremental_manifest(State(state): State<ClientWebState>) -> Response {
    state.0.incremental.as_ref().map_or_else(
        || StatusCode::NOT_FOUND.into_response(),
        |release| {
            (
                [(header::CONTENT_TYPE, "application/json")],
                release.manifest.clone(),
            )
                .into_response()
        },
    )
}
async fn incremental_signature(State(state): State<ClientWebState>) -> Response {
    state.0.incremental.as_ref().map_or_else(
        || StatusCode::NOT_FOUND.into_response(),
        |release| {
            (
                [(header::CONTENT_TYPE, "application/octet-stream")],
                release.signature.clone(),
            )
                .into_response()
        },
    )
}
async fn incremental_payload(
    State(state): State<ClientWebState>,
    Path(name): Path<String>,
) -> Response {
    state
        .0
        .incremental
        .as_ref()
        .and_then(|release| release.payloads.get(&name))
        .map_or_else(
            || StatusCode::NOT_FOUND.into_response(),
            |payload| {
                (
                    [(header::CONTENT_TYPE, "application/octet-stream")],
                    payload.clone(),
                )
                    .into_response()
            },
        )
}

async fn client_web_not_found(uri: Uri) -> StatusCode {
    tracing::info!(path = %uri.path(), "client web request did not match a route");
    StatusCode::NOT_FOUND
}

async fn translation(State(state): State<ClientWebState>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        state.0.translation.clone(),
    )
        .into_response()
}

async fn updatelist(State(state): State<ClientWebState>) -> Response {
    tracing::info!("serving client update list");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        state.0.update_list.clone(),
    )
        .into_response()
}

async fn patch_file(State(state): State<ClientWebState>, Path(name): Path<String>) -> Response {
    // `pname` in the update list appends `.zip`, but the retail format reports the raw file size
    // as `psize`: the payload itself is the listed file, not a second ZIP envelope.
    let Some(listed_name) = name.strip_suffix(".zip") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(patch) = state.0.patch_files.get(listed_name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    tracing::info!(
        file = listed_name,
        bytes = patch.size,
        "serving client patch payload"
    );
    let Ok(file) = patch.file.try_clone() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(content_length) = HeaderValue::from_str(&patch.size.to_string()) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let stream = ReaderStream::new(tokio::fs::File::from_std(file));
    let mut response = Body::from_stream(stream).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, content_length);
    response
}

async fn extracontents(State(state): State<ClientWebState>) -> Response {
    xml(state.0.extra_contents.clone())
}

async fn theme_document(State(state): State<ClientWebState>) -> Response {
    xml(state.0.theme_document.clone())
}

fn xml(body: String) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/xml; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn theme_image(State(state): State<ClientWebState>, Path(name): Path<String>) -> Response {
    // The theme document only ever names files this same directory listing produced, so a
    // request for anything else is not a client following the document. Reject it on the name
    // rather than on the open, so no path built from request text reaches the filesystem.
    let named_in_document = state.0.theme_document.contains(&format!("name=\"{name}\""));
    let Some(directory) = state.0.theme_directory.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !named_in_document {
        return StatusCode::NOT_FOUND.into_response();
    }
    match read_bounded(directory, &name, MAX_THEME_IMAGE_BYTES) {
        Some(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/jpeg")],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Serves the client web router until cancellation.
///
/// # Errors
/// Returns an HTTP serving I/O failure.
pub async fn serve_client_web(
    listener: TcpListener,
    state: ClientWebState,
    shutdown: CancellationToken,
) -> Result<(), std::io::Error> {
    axum::serve(listener, client_web_router(state))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_classification_follows_the_retail_naming_convention() {
        let names = vec![
            "2016020301.jpg".to_owned(),
            "2016041502.jpg".to_owned(),
            "background_01.jpg".to_owned(),
            "background_14.jpg".to_owned(),
            "loading_background.jpg".to_owned(),
            "main_bg.jpg".to_owned(),
            "main_bg_8.jpg".to_owned(),
            "readme.txt".to_owned(),
        ];
        let theme = classify_theme(&names);
        assert_eq!(theme.notices, ["2016041502.jpg", "2016020301.jpg"]);
        assert_eq!(theme.lobby_wallpapers, ["main_bg.jpg", "main_bg_8.jpg"]);
        assert_eq!(
            theme.loading_wallpapers,
            [
                "background_01.jpg",
                "background_14.jpg",
                "loading_background.jpg"
            ]
        );
        // An unclassifiable file is left out rather than guessed at.
        assert!(theme.to_xml().expect("valid").matches("readme").count() == 0);
    }

    #[test]
    fn an_empty_theme_directory_still_yields_a_valid_document() {
        let theme = classify_theme(&[]);
        let xml = theme.to_xml().expect("valid");
        assert!(xml.contains("<notice>"));
        assert!(!xml.contains("<file "));
    }
}

#[cfg(test)]
mod router_tests {
    use super::*;

    /// A route table that panics at build time would take the listener task down with it.
    #[test]
    fn router_builds_for_every_prefix() {
        let state = ClientWebState(Arc::new(Prepared {
            launcher_paks: Vec::new(),
            stat_snapshot: Vec::new(),
            patch_version: "PangYa-RS".to_owned(),
            patch_number: 851,
            client_directory_path: std::path::PathBuf::new(),
            update_list: Vec::new(),
            patch_files: HashMap::new(),
            translation: String::new(),
            extra_contents: String::new(),
            theme_document: String::new(),
            theme_directory: None,
            incremental: None,
        }));
        let _router = client_web_router(state);
    }

    #[tokio::test]
    async fn incremental_routes_serve_only_declared_metadata_and_payloads() {
        let manifest = br#"{\"schema_version\":1,\"tool_version\":\"test\",\"release_id\":1,\"key_id\":\"test\",\"target_pak\":\"projectg851gb.pak\",\"base_pak\":{\"size\":1,\"pangya_crc\":0,\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\"},\"current_iff_sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"current_iff_size\":1,\"members\":[],\"result_iff_sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"result_iff_size\":1,\"result_pak\":{\"size\":1,\"pangya_crc\":0,\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\"}}"#.to_vec();
        let state = ClientWebState(Arc::new(Prepared {
            launcher_paks: Vec::new(),
            stat_snapshot: Vec::new(),
            patch_version: "test".into(),
            patch_number: 851,
            client_directory_path: std::path::PathBuf::new(),
            update_list: Vec::new(),
            patch_files: HashMap::new(),
            translation: String::new(),
            extra_contents: String::new(),
            theme_document: String::new(),
            theme_directory: None,
            incremental: Some(IncrementalRelease {
                metadata: serde_json::from_slice(
                    &manifest
                        .iter()
                        .copied()
                        .filter(|byte| *byte != b'\\')
                        .collect::<Vec<_>>(),
                )
                .expect("metadata"),
                manifest: manifest.clone(),
                signature: vec![7; 64],
                payloads: HashMap::from([("one.iff".into(), b"changed".to_vec())]),
            }),
        }));
        assert_eq!(
            incremental_manifest(State(state.clone())).await.status(),
            StatusCode::OK
        );
        let signature = incremental_signature(State(state.clone())).await;
        assert_eq!(signature.status(), StatusCode::OK);
        assert_eq!(
            axum::body::to_bytes(signature.into_body(), 128)
                .await
                .expect("body")
                .len(),
            64
        );
        assert_eq!(
            incremental_payload(State(state.clone()), Path("one.iff".into()))
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            incremental_payload(State(state.clone()), Path("../projectg851gb.pak".into()))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            incremental_payload(State(state), Path("projectg851gb.pak".into()))
                .await
                .status(),
            StatusCode::NOT_FOUND,
            "final PAK is never a v2 payload"
        );
    }

    #[tokio::test]
    async fn patch_payload_is_streamed_only_for_an_allowlisted_packed_name() {
        let root = std::env::temp_dir().join(format!("pangya-patch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).expect("temp directory");
        let path = root.join("projectg852gb.pak");
        std::fs::write(&path, b"authored-pak").expect("write patch");
        let file = std::fs::File::open(&path).expect("open patch");
        let state = ClientWebState(Arc::new(Prepared {
            launcher_paks: Vec::new(),
            stat_snapshot: Vec::new(),
            patch_version: "PangYa-RS".to_owned(),
            patch_number: 851,
            client_directory_path: root.clone(),
            update_list: Vec::new(),
            patch_files: HashMap::from([(
                "projectg852gb.pak".to_owned(),
                PatchFile {
                    file: Arc::new(file),
                    size: 12,
                },
            )]),
            translation: String::new(),
            extra_contents: String::new(),
            theme_document: String::new(),
            theme_directory: None,
            incremental: None,
        }));

        let response = patch_file(
            State(state.clone()),
            Path("projectg852gb.pak.zip".to_owned()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "12");
        let body = axum::body::to_bytes(response.into_body(), 64)
            .await
            .expect("stream body");
        assert_eq!(&body[..], b"authored-pak");

        let response = patch_file(State(state), Path("../secret.zip".to_owned())).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(root).expect("remove temp directory");
    }
}
