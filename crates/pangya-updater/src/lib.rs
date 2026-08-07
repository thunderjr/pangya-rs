//! The HTTP contract a retail PangYa client needs before it will start.
//!
//! # Why this exists
//!
//! [`SPEC.md`] §13.4 deferred PAK/XTEA/`updatelist` support to Tier D "unless a real client
//! startup path proves it is required earlier". Running the U.S. 852 client against this server
//! proved exactly that. Before the client opens a single socket to LoginService it performs
//! three HTTP requests, and failing any one of them ends the run:
//!
//! 1. `GET /Translation/Read.aspx` — a base64 string catalog. Missing it aborts startup with
//!    the client's own "string load failed." dialog.
//! 2. `GET …/S4_Patch/updatelist` — the XTEA-encrypted patch manifest. Missing it aborts with
//!    "Please re-install the game or run the update program first."
//! 3. `GET …/S4_Patch/extracontents/extracontents.xml`, then the theme document it names, then
//!    every image the theme names.
//!
//! Only after all three does the client mount its PAK series. None of this is gameplay
//! protocol, which is why it lives in its own crate: it is a static content contract, it holds
//! no domain state, and it never touches the database.
//!
//! Separately from anything served here, the client requires the registry value
//! `HKLM\SOFTWARE\WOW6432Node\Ntreev USA\Pangya\IntegratedPak`. Retail's updater writes it; a
//! copied install has no such value and the client refuses to start. See
//! `docs/RUNNING_THE_CLIENT.md`.
//!
//! # Scope
//!
//! Nothing here is a claim that the client reaches its login screen. See `docs/PROGRESS.md` for
//! the open blocker past this point.
//!
//! [`SPEC.md`]: https://github.com/thunderjr/pangya-rs/blob/main/docs/SPEC.md

pub mod crc;
pub mod theme;
pub mod updatelist;
pub mod xtea;

pub use crc::{FileChecksum, checksum};
pub use theme::{Theme, ThemeError, encode_translation_catalog, extra_contents_xml};
pub use updatelist::{
    EntrySelection, FileEntry, MAX_ENTRIES, MAX_FILE_BYTES, UPDATE_LIST_VERSION, UpdateList,
    UpdateListError, build_from_directory,
};
pub use xtea::{UpdateListRegion, XteaKey, XteaLengthError, decipher_trim_nul, encipher_pad_nul};
