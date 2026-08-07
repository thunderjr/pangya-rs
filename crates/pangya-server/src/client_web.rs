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
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use pangya_updater::{
    EntrySelection, Theme, UpdateListRegion, build_from_directory, encode_translation_catalog,
    extra_contents_xml,
};
use std::io::Read;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Maximum bytes served for one theme image.
///
/// The largest wallpaper in the U.S. client is a little over 600 KiB; four mebibytes leaves
/// headroom without letting a mistaken theme directory stream something huge to a client.
pub const MAX_THEME_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

/// Maximum bytes read for the translation catalog.
pub const MAX_TRANSLATION_BYTES: u64 = 4 * 1024 * 1024;

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
}

/// Everything the routes serve, prepared once.
struct Prepared {
    update_list: Vec<u8>,
    translation: String,
    extra_contents: String,
    theme_document: String,
    theme_directory: Option<Dir>,
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
        let update_list = build_from_directory(
            &client_directory,
            settings.entries,
            &settings.patch_version,
            settings.patch_number,
        )?
        .to_encrypted(settings.region.key());

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
            translation,
            extra_contents,
            theme_document,
            theme_directory,
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
    let mut router = Router::new().route("/Translation/Read.aspx", get(translation));
    for prefix in PREFIXES {
        router = router
            .route(&format!("{prefix}/updatelist"), get(updatelist))
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
    router.with_state(state)
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
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        state.0.update_list.clone(),
    )
        .into_response()
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
            update_list: Vec::new(),
            translation: String::new(),
            extra_contents: String::new(),
            theme_document: String::new(),
            theme_directory: None,
        }));
        let _router = client_web_router(state);
    }
}
