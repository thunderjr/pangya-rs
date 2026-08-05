# lzokay 2.x compatibility report

Status: **accepted for the bounded M1 protocol foundation; real-client gate open**.

Evidence from automated tests:

- lzokay 2.0.1 decompresses every representative ISC PangCrypt server vector.
- lzokay-generated non-empty server packets round-trip boundary and property inputs.
- Production server plaintext shorter than the required `u16` opcode is rejected.
- `server_decrypt` enforces absolute output and compressed-input expansion-ratio
  caps; a frame with an internally consistent outer length and corrupt LZO body
  returns the typed `CryptoError::Lzo` error.

Independent developer evidence:

- `github.com/rasky/go-lzo` decoded a deterministic 131,072-byte lzokay-generated
  stream exactly; sizes, hashes, commands, and the external GPL-tool boundary are
  recorded in [`../evidence/LZO_INDEPENDENT_2026-08-05.md`](../evidence/LZO_INDEPENDENT_2026-08-05.md).

Not yet proven:

- a legally held real U.S. 852 client accepts generated frames through
  login/channel entry.

Therefore ADR-0005 does not claim client acceptance. Exact compressed bytes need
not match the Go oracle when the LZO1X stream is interoperable.
