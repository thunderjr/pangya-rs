//! Startup cross-check between the served client archive and the loaded server catalog.
//!
//! `scripts/author-client-iff.py` writes both halves of a custom shop out of one authored ZIP,
//! so within a successful authoring run they cannot disagree. What nothing checked until now is
//! the *deployment* of that run: an operator can restart with a fresh `iff-gb` and a stale PAK,
//! or the reverse, and the two failure modes are among the most expensive in this project
//! because neither names its cause.
//!
//! - stale PAK, fresh catalog → the client refuses to start with
//!   `"projectg850gb.pak file has been corrupted."`, because the updatelist is generated from
//!   the served directory and no longer matches what the client holds;
//! - fresh PAK, stale catalog → the client renders the shop perfectly and every purchase is
//!   refused with `not_in_catalog`, which reads as a server bug.
//!
//! The authoring run already records the SHA-256 of both halves in its report. Comparing them at
//! startup turns a silent, hours-long debugging session into a refusal that names the stale side.
//! Configured with `client_web.publish_report`; absent, behaviour is exactly as before.

use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::path::Path;

/// Largest report this will read. The real file is well under a megabyte even with one offer
/// per authorable row (4,031 of them is ~560 KB), so this is generous by an order of magnitude.
const MAX_REPORT_BYTES: u64 = 8 * 1024 * 1024;

/// The subset of `shop-sync-report.json` this check needs.
///
/// Deliberately not the whole document: the report also carries every authored offer, and
/// deserializing thousands of rows to compare two digests would be waste. Unknown fields are
/// ignored, so the script can grow its report without breaking startup here.
#[derive(Debug, Deserialize)]
struct PublishReport {
    /// SHA-256 of the PAK the authoring run staged for clients.
    client_pak_sha256: String,
    server_iff: ServerIff,
}

#[derive(Debug, Deserialize)]
struct ServerIff {
    /// SHA-256 of the `manifest.toml` written alongside the server tables.
    manifest_sha256: String,
}

/// Why a publish cross-check refused.
#[derive(Debug, thiserror::Error)]
pub enum PublishReportError {
    /// The report could not be read or parsed.
    #[error("the configured publish report could not be read")]
    Unreadable,
    /// A file the report attests could not be read.
    #[error("a file named by the publish report could not be read: {0}")]
    MissingArtifact(String),
    /// The served client archive is not the one the report attests.
    #[error(
        "the served {name} does not match the publish report \
         (report {expected}, served {actual}); the client PAK and the server catalog are out of \
         step — re-run scripts/sync-client-shop.sh, or deploy the authored PAK that matches this \
         catalog"
    )]
    ClientPakMismatch {
        /// File name of the archive that disagreed.
        name: String,
        /// Digest the authoring run recorded.
        expected: String,
        /// Digest of the archive actually being served.
        actual: String,
    },
    /// The loaded catalog manifest is not the one the report attests.
    #[error(
        "the loaded catalog manifest does not match the publish report \
         (report {expected}, loaded {actual}); the server catalog and the client PAK are out of \
         step — point data.iff_directory at the catalog this report authored"
    )]
    ManifestMismatch {
        /// Digest the authoring run recorded.
        expected: String,
        /// Digest of the manifest actually loaded.
        actual: String,
    },
}

/// Verifies that the served client archive and the loaded catalog came from one authoring run.
///
/// `client_pak` is the archive the report names — `projectg850gb.pak` in every run so far — and
/// `manifest` is the `manifest.toml` inside the configured `data.iff_directory`.
///
/// # Errors
///
/// Returns an error when the report is unreadable, when either artifact is missing, or when
/// either digest disagrees. Every variant names which side is stale, because "these two files
/// disagree" without saying which one to fix is only half an error message.
pub fn verify(report: &Path, client_pak: &Path, manifest: &Path) -> Result<(), PublishReportError> {
    let bytes = read_bounded(report, MAX_REPORT_BYTES).ok_or(PublishReportError::Unreadable)?;
    let report: PublishReport =
        serde_json::from_slice(&bytes).map_err(|_| PublishReportError::Unreadable)?;

    let pak_name = client_pak
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("client archive")
        .to_owned();
    let served = digest_of(client_pak)
        .ok_or_else(|| PublishReportError::MissingArtifact(pak_name.clone()))?;
    if !served.eq_ignore_ascii_case(&report.client_pak_sha256) {
        return Err(PublishReportError::ClientPakMismatch {
            name: pak_name,
            expected: report.client_pak_sha256,
            actual: served,
        });
    }

    let loaded = digest_of(manifest)
        .ok_or_else(|| PublishReportError::MissingArtifact("manifest.toml".to_owned()))?;
    if !loaded.eq_ignore_ascii_case(&report.server_iff.manifest_sha256) {
        return Err(PublishReportError::ManifestMismatch {
            expected: report.server_iff.manifest_sha256,
            actual: loaded,
        });
    }

    Ok(())
}

/// Streams a file through SHA-256 without holding it in memory.
///
/// The PAK is megabytes today and the base archive is over a gigabyte, so this must not read the
/// whole file to compare a digest.
fn digest_of(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(format!("{:x}", hasher.finalize()))
}

/// Reads a whole small file, refusing anything oversized before allocating for it.
fn read_bounded(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > limit {
        return None;
    }
    std::io::read_to_string(file).ok().map(String::into_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write(dir: &Path, name: &str, body: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(body).expect("write");
        path
    }

    fn sha256_of(body: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(body);
        format!("{:x}", hasher.finalize())
    }

    fn fixture(
        pak: &[u8],
        manifest: &[u8],
        report_pak: &str,
        report_manifest: &str,
    ) -> tempdir::Fixture {
        let dir = std::env::temp_dir().join(format!("pangya-report-{}", uuid_like()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let pak_path = write(&dir, "projectg850gb.pak", pak);
        let manifest_path = write(&dir, "manifest.toml", manifest);
        let report_path = write(
            &dir,
            "shop-sync-report.json",
            format!(
                r#"{{"version":1,"client_pak_sha256":"{report_pak}",
                     "server_iff":{{"manifest_sha256":"{report_manifest}"}}}}"#
            )
            .as_bytes(),
        );
        tempdir::Fixture {
            dir,
            pak: pak_path,
            manifest: manifest_path,
            report: report_path,
        }
    }

    /// Enough uniqueness for parallel test runs without pulling `uuid` into this crate's
    /// non-dev dependency set.
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        format!("{nanos}-{:?}", std::thread::current().id()).replace(['(', ')', ' '], "")
    }

    mod tempdir {
        pub struct Fixture {
            pub dir: std::path::PathBuf,
            pub pak: std::path::PathBuf,
            pub manifest: std::path::PathBuf,
            pub report: std::path::PathBuf,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
    }

    #[test]
    fn a_matching_pair_passes() {
        let pak = b"authored pak bytes";
        let manifest = b"manifest_version = 3\n";
        let f = fixture(pak, manifest, &sha256_of(pak), &sha256_of(manifest));
        assert!(verify(&f.report, &f.pak, &f.manifest).is_ok());
    }

    #[test]
    fn a_stale_client_pak_is_refused_and_named() {
        // The operator restarted with a fresh catalog and forgot to deploy the PAK. Without this
        // the symptom is the client's own corruption dialog, which names no server-side cause.
        let manifest = b"manifest_version = 3\n";
        let f = fixture(
            b"the PAK actually on disk",
            manifest,
            &sha256_of(b"the PAK the run authored"),
            &sha256_of(manifest),
        );
        let error = verify(&f.report, &f.pak, &f.manifest).expect_err("must refuse");
        assert!(matches!(
            error,
            PublishReportError::ClientPakMismatch { .. }
        ));
        assert!(error.to_string().contains("projectg850gb.pak"));
    }

    #[test]
    fn a_stale_catalog_is_refused_and_named() {
        // The reverse: the PAK moved, the catalog did not. The client shows the new shop and
        // every purchase is refused with `not_in_catalog`.
        let pak = b"authored pak bytes";
        let f = fixture(
            pak,
            b"the manifest actually loaded",
            &sha256_of(pak),
            &sha256_of(b"the manifest the run authored"),
        );
        let error = verify(&f.report, &f.pak, &f.manifest).expect_err("must refuse");
        assert!(matches!(error, PublishReportError::ManifestMismatch { .. }));
    }

    #[test]
    fn a_missing_artifact_is_refused_rather_than_treated_as_matching() {
        let pak = b"authored pak bytes";
        let manifest = b"manifest_version = 3\n";
        let f = fixture(pak, manifest, &sha256_of(pak), &sha256_of(manifest));
        std::fs::remove_file(&f.pak).expect("remove");
        assert!(matches!(
            verify(&f.report, &f.pak, &f.manifest),
            Err(PublishReportError::MissingArtifact(_))
        ));
    }

    #[test]
    fn an_unreadable_report_is_refused() {
        let pak = b"x";
        let manifest = b"y";
        let f = fixture(pak, manifest, &sha256_of(pak), &sha256_of(manifest));
        std::fs::write(&f.report, b"{ not json").expect("write");
        assert!(matches!(
            verify(&f.report, &f.pak, &f.manifest),
            Err(PublishReportError::Unreadable)
        ));
    }

    #[test]
    fn digest_comparison_is_case_insensitive() {
        // The report is written lowercase today, but a hand-edited or differently-generated
        // report must not be rejected for casing alone.
        let pak = b"authored pak bytes";
        let manifest = b"manifest_version = 3\n";
        let f = fixture(
            pak,
            manifest,
            &sha256_of(pak).to_uppercase(),
            &sha256_of(manifest).to_uppercase(),
        );
        assert!(verify(&f.report, &f.pak, &f.manifest).is_ok());
    }
}
