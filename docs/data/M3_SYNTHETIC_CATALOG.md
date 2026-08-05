# M3 synthetic catalog manifest and mount

No proprietary IFF files are included. The committed `.bin` files under
`crates/pangya-data/tests/fixtures/synthetic-catalog/` are generated eight-byte
records and are not client-derived.

## Enablement

```toml
[game]
enabled = true
channel_id = 1

[data]
catalog_required_m3 = true
iff_directory = "/srv/pangya/us852-read-only"
manifest = "manifest.toml"
load_timeout = "5s"
```

Mount the directory read-only. Startup opens `iff_directory` once as a directory
capability, then opens the manifest and every member relative to that capability;
it never reopens joined ambient paths. Absolute paths, `.`/`..`, non-regular
files, and symlink escapes are rejected. Startup reads at most 64 KiB of manifest
and 64 MiB per declared file before binding listeners. The filesystem read itself
is not cancellable, so timeout detaches its dedicated standard thread; it cannot
retain the Tokio runtime or delay runtime/process teardown.

## Version 1 manifest

```toml
manifest_version = 1

[[files]]
filename = "Character.bin"
sha256 = "<64 lowercase hex characters>"
kind = "character" # character | club_set | ball
count = 123
binding = 1
version = 1
record_size = 64
```

Exactly one declaration for each minimum kind is required. The file header is
synthetic LE `count:u16`, `binding:u16`, `version:u32`. `count` must be nonzero
and match the manifest. Exact file length is `8 + count * record_size`, computed
with checked arithmetic; trailing bytes are rejected. `record_size` is 4..65536.
The first four bytes of each record are a LE `u32 type_id`; that identifier must
be globally unique across Character, ClubSet, and Ball. The remaining bytes are
immutable opaque data.

The manifest digest covers the complete file. Starter character, items, and
explicit club/ball equipment bindings are cross-checked before any listener is
bound. Player snapshots are checked again before bootstrap packets are emitted.
U.S. 852 production headers, bindings, versions, and record sizes remain
unattested and must be supplied through future legal evidence rather than inferred
from this synthetic format.
