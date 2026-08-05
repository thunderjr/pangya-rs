# Independent LZO1X developer evidence — 2026-08-05

## Result

A one-time developer probe compressed a deterministic 131,072-byte plaintext
stream with `lzokay` 2.0.1. The raw LZO1X stream was 1,700 bytes. The independent
Go implementation `github.com/rasky/go-lzo` at
`v0.0.0-20200203143853-96a758eda86e` decoded it byte-for-byte exactly.

| Artifact | Size | SHA-256 |
|---|---:|---|
| plaintext | 131,072 | `af45fd2b18770102fd622556c556da0820dc61b36106a6432fb2106922890851` |
| lzokay-compressed LZO1X | 1,700 | `7b836b87e49e8396a8ff5f558aa4a7264d83d0c98e9df9389c73b035c8a84b89` |

This proves one generated non-empty lzokay stream interoperated with a second
implementation. It is not the separate real U.S. 852 client acceptance gate.

## Command description

The probe was intentionally outside the repository. A small Rust helper pinned
`lzokay = "=2.0.1"`, generated the deterministic 131,072-byte input, called
`lzokay::compress::compress`, and wrote `plain.bin` and `compressed.lzo`. A Go
module pinned the decoder revision and compared its output:

```bash
cargo run --release --manifest-path /tmp/lzokay-proof/Cargo.toml
shasum -a 256 /tmp/lzokay-proof/plain.bin /tmp/lzokay-proof/compressed.lzo
cd /tmp/go-lzo-proof
go mod init independent-lzo-proof
go get github.com/rasky/go-lzo@v0.0.0-20200203143853-96a758eda86e
go run . /tmp/lzokay-proof/compressed.lzo /tmp/lzokay-proof/plain.bin
```

The Go helper used `lzo.Decompress1X(bytes.NewReader(compressed), 0, 0)` and
`bytes.Equal(decoded, plaintext)`, reporting `got=131072 match=true`. Repeating
the procedure with any deterministic non-empty byte stream reproduces the
cross-implementation check; the hashes above identify the exact executed probe.

## License boundary

`go-lzo` is GPL-licensed. It was used only as an external developer command for
this one-time compatibility probe. It is not linked, vendored, copied, or
distributed by PangYa-RS and is absent from both Cargo dependency graphs.
