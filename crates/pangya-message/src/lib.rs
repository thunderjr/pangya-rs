#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Compiling M1 skeleton for the future message boundary.

/// Marker proving the M1 crate boundary is available.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "message"
}
