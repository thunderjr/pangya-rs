//! The client's patch manifest: which files it should have, and their checksums.
//!
//! The U.S. 852 client fetches this before it will load its PAK series at all. It is XML,
//! XTEA-encrypted with a per-region key, and its exact byte layout is a compatibility surface:
//! the writer here reproduces the indentation, attribute order, and self-closing-tag spacing
//! the reference implementation emits, because that output is known to be accepted.
//!
//! # Provenance
//!
//! The document schema and emitter conventions are adapted from `pangbox/pangfiles`
//! (`updatelist`, `encoding/litexml`), ISC licensed, © 2018-2020 John Chadwick. See
//! `docs/PROVENANCE.md`.

use crate::crc::FileChecksum;
use crate::xtea::{XteaKey, encipher_pad_nul};
use cap_std::fs::Dir;
use std::io::Read;

/// Maximum entries in one generated document.
///
/// A client directory is operator-supplied, so the count is untrusted input to the allocation
/// below it. The real U.S. series is 84 files; four thousand leaves room for a heavily patched
/// install without letting a mistaken directory produce an unbounded document.
pub const MAX_ENTRIES: usize = 4096;

/// Maximum bytes read from one listed file while checksumming it.
///
/// The base U.S. PAK is a little over 1 GiB, so this has to be generous. It exists to bound
/// the work a single mistaken entry can cause, not to express a protocol limit.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

const CHECKSUM_CHUNK_BYTES: usize = 1 << 20;

/// The fixed `updatelistVer` the U.S. 852 client expects.
///
/// The client compares this against a hardcoded value; it is a format version, not a date to
/// keep current.
pub const UPDATE_LIST_VERSION: &str = "20090331";

/// Errors from building an update list.
#[derive(Debug, thiserror::Error)]
pub enum UpdateListError {
    /// The directory could not be opened or listed.
    #[error("the client directory could not be read")]
    Directory,
    /// A listed file could not be opened or read.
    #[error("a listed client file could not be read")]
    File,
    /// A file name was not representable in the document.
    #[error("a client file name is not usable in an update list")]
    FileName,
    /// The directory held more entries than [`MAX_ENTRIES`].
    #[error("the client directory holds more than {MAX_ENTRIES} listable files")]
    TooManyEntries,
    /// A listed file was larger than [`MAX_FILE_BYTES`].
    #[error("a listed client file is larger than the configured maximum")]
    FileTooLarge,
}

/// One file's entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    /// File name, without a directory component.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// PangYa file checksum in the signed form the attribute carries.
    ///
    /// This is what decides whether the client starts, so it is the authority for "is this
    /// archive the one the server expects". It is also only 32 bits and not a cryptographic
    /// digest, which is why [`FileEntry::sha256`] exists alongside it.
    pub checksum: i32,
    /// SHA-256 of the same bytes, lowercase hex.
    ///
    /// Never written to the retail document — the client has no field for it. It exists so the
    /// launcher can verify a *download* before letting it near a client directory: a 32-bit
    /// checksum is a compatibility signal, not an integrity boundary for a network transfer.
    /// Computed in the same read pass as the checksum so publishing it costs no extra I/O.
    pub sha256: String,
    /// Modification date as `YYYY-MM-DD`.
    pub date: String,
    /// Modification time as `HH:MM:SS`.
    pub time: String,
}

/// A complete update list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateList {
    /// Human-readable patch version.
    pub patch_version: String,
    /// Numeric patch number.
    pub patch_number: u32,
    /// Entries, in the order they will be written.
    pub entries: Vec<FileEntry>,
}

impl UpdateList {
    /// Renders the plaintext XML document.
    ///
    /// Kept separate from encryption so tests and operator tooling can read what the client
    /// will see without holding a key.
    #[must_use]
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        out.push_str("<?xml version=\"1.0\" encoding=\"euc-kr\" standalone=\"yes\" ?>\n");
        out.push_str("<patchVer value=\"");
        push_escaped(&mut out, &self.patch_version);
        out.push_str("\" />\n");
        out.push_str(&format!("<patchNum value=\"{}\" />\n", self.patch_number));
        out.push_str(&format!(
            "<updatelistVer value=\"{UPDATE_LIST_VERSION}\" />\n"
        ));
        out.push_str(&format!("<updatefiles count=\"{}\">\n", self.entries.len()));
        for entry in &self.entries {
            out.push_str("        <fileinfo fname=\"");
            push_escaped(&mut out, &entry.name);
            out.push_str("\" fdir=\"\" fsize=\"");
            out.push_str(&entry.size.to_string());
            out.push_str("\" fcrc=\"");
            out.push_str(&entry.checksum.to_string());
            out.push_str("\" fdate=\"");
            push_escaped(&mut out, &entry.date);
            out.push_str("\" ftime=\"");
            push_escaped(&mut out, &entry.time);
            out.push_str("\" pname=\"");
            push_escaped(&mut out, &entry.name);
            out.push_str(".zip\" psize=\"");
            out.push_str(&entry.size.to_string());
            out.push_str("\" />\n");
        }
        out.push_str("</updatefiles>\n");
        out
    }

    /// Renders and encrypts the document for the client.
    #[must_use]
    pub fn to_encrypted(&self, key: XteaKey) -> Vec<u8> {
        encipher_pad_nul(key, self.to_xml().as_bytes())
    }
}

/// Escapes a value the way the reference emitter's `xml.EscapeText` does.
///
/// Go escapes quotes numerically and also escapes the ASCII control characters that a file
/// name could contain, so a name with a quote in it produces the same bytes here as there.
fn push_escaped(out: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            other => out.push(other),
        }
    }
}

/// Which files in the directory to list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntrySelection {
    /// Only the PAK series, which is what the client needs in order to mount its data.
    PakSeriesOnly,
    /// Every regular file in the directory.
    ///
    /// This mirrors a retail patch server, which also distributes executables and DLLs. It
    /// means a locally modified client file — a replaced `ijl15.dll`, for one — appears in the
    /// list with the checksum it has on the server rather than the one on disk.
    AllFiles,
}

impl EntrySelection {
    fn accepts(self, name: &str) -> bool {
        match self {
            Self::PakSeriesOnly => name
                .rsplit_once('.')
                .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("pak")),
            Self::AllFiles => true,
        }
    }
}

/// Builds an update list by scanning an opened client directory.
///
/// Entries are sorted by name so the document is reproducible: the same directory always
/// produces the same bytes, which is what lets a golden test pin the format.
///
/// # Errors
/// Returns [`UpdateListError`] when the directory cannot be listed, an entry cannot be read,
/// a name is unusable, or the directory exceeds the entry or size bounds.
pub fn build_from_directory(
    directory: &Dir,
    selection: EntrySelection,
    patch_version: &str,
    patch_number: u32,
) -> Result<UpdateList, UpdateListError> {
    let mut names = Vec::new();
    for entry in directory
        .entries()
        .map_err(|_| UpdateListError::Directory)?
    {
        let entry = entry.map_err(|_| UpdateListError::Directory)?;
        let metadata = entry.metadata().map_err(|_| UpdateListError::Directory)?;
        if !metadata.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| UpdateListError::FileName)?;
        if !selection.accepts(&name) {
            continue;
        }
        if names.len() >= MAX_ENTRIES {
            return Err(UpdateListError::TooManyEntries);
        }
        names.push(name);
    }
    names.sort();

    let mut entries = Vec::with_capacity(names.len());
    for name in names {
        entries.push(entry_for(directory, &name)?);
    }
    Ok(UpdateList {
        patch_version: patch_version.to_owned(),
        patch_number,
        entries,
    })
}

fn entry_for(directory: &Dir, name: &str) -> Result<FileEntry, UpdateListError> {
    let mut file = directory.open(name).map_err(|_| UpdateListError::File)?;
    let metadata = file.metadata().map_err(|_| UpdateListError::File)?;
    let size = metadata.len();
    if size > MAX_FILE_BYTES {
        return Err(UpdateListError::FileTooLarge);
    }
    let modified = metadata.modified().map_err(|_| UpdateListError::File)?;
    let (date, time) = format_timestamp(modified.into_std());

    let mut hasher = FileChecksum::new();
    let mut digest = <sha2::Sha256 as sha2::Digest>::new();
    let mut buffer = vec![0_u8; CHECKSUM_CHUNK_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(|_| UpdateListError::File)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        sha2::Digest::update(&mut digest, &buffer[..read]);
    }

    Ok(FileEntry {
        name: name.to_owned(),
        size,
        checksum: hasher.finish_signed(),
        sha256: format!("{:x}", sha2::Digest::finalize(digest)),
        date,
        time,
    })
}

/// Formats a modification time the way the document's two attributes carry it.
///
/// Local time is deliberate: the retail documents carry the patch machine's wall clock, and
/// the client only compares these fields against its own recorded copy.
fn format_timestamp(time: std::time::SystemTime) -> (String, String) {
    let local: chrono::DateTime<chrono::Local> = time.into();
    (
        local.format("%Y-%m-%d").to_string(),
        local.format("%H:%M:%S").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtea::{UpdateListRegion, decipher_trim_nul};

    fn sample() -> UpdateList {
        UpdateList {
            patch_version: "FakeVer".to_owned(),
            patch_number: 9999,
            entries: vec![
                FileEntry {
                    name: "projectg700gb+.pak".to_owned(),
                    size: 1_131_201_576,
                    checksum: -1_234_567,
                    sha256: String::new(),
                    date: "2016-04-15".to_owned(),
                    time: "09:41:58".to_owned(),
                },
                FileEntry {
                    name: "projectg851gb.pak".to_owned(),
                    size: 690_331,
                    checksum: 42,
                    sha256: String::new(),
                    date: "2016-11-02".to_owned(),
                    time: "00:30:00".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn document_layout_is_byte_exact() {
        let expected = concat!(
            "<?xml version=\"1.0\" encoding=\"euc-kr\" standalone=\"yes\" ?>\n",
            "<patchVer value=\"FakeVer\" />\n",
            "<patchNum value=\"9999\" />\n",
            "<updatelistVer value=\"20090331\" />\n",
            "<updatefiles count=\"2\">\n",
            "        <fileinfo fname=\"projectg700gb+.pak\" fdir=\"\" fsize=\"1131201576\" ",
            "fcrc=\"-1234567\" fdate=\"2016-04-15\" ftime=\"09:41:58\" ",
            "pname=\"projectg700gb+.pak.zip\" psize=\"1131201576\" />\n",
            "        <fileinfo fname=\"projectg851gb.pak\" fdir=\"\" fsize=\"690331\" ",
            "fcrc=\"42\" fdate=\"2016-11-02\" ftime=\"00:30:00\" ",
            "pname=\"projectg851gb.pak.zip\" psize=\"690331\" />\n",
            "</updatefiles>\n",
        );
        assert_eq!(sample().to_xml(), expected);
    }

    #[test]
    fn encrypted_document_round_trips_through_the_region_key() {
        let key = UpdateListRegion::Us.key();
        let encrypted = sample().to_encrypted(key);
        let decrypted = decipher_trim_nul(key, &encrypted).expect("aligned");
        assert_eq!(
            String::from_utf8(decrypted).expect("utf8"),
            sample().to_xml()
        );
    }

    #[test]
    fn selection_matches_the_pak_series_case_insensitively() {
        let paks = EntrySelection::PakSeriesOnly;
        assert!(paks.accepts("projectg700gb+.pak"));
        assert!(paks.accepts("ProjectG984.PAK"));
        assert!(!paks.accepts("ijl15.dll"));
        assert!(!paks.accepts("pak"));
        assert!(EntrySelection::AllFiles.accepts("ijl15.dll"));
    }

    #[test]
    fn names_needing_escapes_do_not_break_the_document() {
        let list = UpdateList {
            patch_version: "a&b".to_owned(),
            patch_number: 1,
            entries: vec![FileEntry {
                name: "we\"ird<&>'.pak".to_owned(),
                size: 1,
                checksum: 0,
                sha256: String::new(),
                date: "2016-01-01".to_owned(),
                time: "00:00:00".to_owned(),
            }],
        };
        let xml = list.to_xml();
        assert!(xml.contains("<patchVer value=\"a&amp;b\" />"));
        assert!(xml.contains("fname=\"we&#34;ird&lt;&amp;&gt;&#39;.pak\""));
        assert!(xml.contains("pname=\"we&#34;ird&lt;&amp;&gt;&#39;.pak.zip\""));
    }
}
