# Third-party notices

## pangbox/pangcrypt

`crates/pangya-crypto/src/oracle.rs` and crypto golden-vector byte sequences are
adapted from pangbox/pangcrypt revision `2bf7a1d36591`, ISC licensed:

Copyright © 2018-2019, John Chadwick <johnwchadwick@gmail.com>

Permission to use, copy, modify, and/or distribute this software for any purpose
with or without fee is hereby granted, provided that the above copyright notice
and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.

## pangbox/packetdoc

LoginService layout material and captured layout fixtures are directly adapted
from pangbox/packetdoc revision `d61f583a3e67`, ISC licensed:

Copyright © 2019, John Chadwick <johnwchadwick@gmail.com>

Permission to use, copy, modify, and/or distribute this software for any purpose
with or without fee is hereby granted, provided that the above copyright notice
and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.

## pangbox/server

The U.S. LoginService hello material is directly adapted from pangbox/server
revision `91d8a5a4f3be`, ISC licensed:

Copyright © 2018-2023, John Chadwick <john@jchw.io>

Permission to use, copy, modify, and/or distribute this software for any purpose
with or without fee is hereby granted, provided that the above copyright notice
and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.

## lzokay

lzokay 2.x is MIT licensed. It is used for LZO1X compression/decompression. See
Cargo.lock and the dependency's packaged license for its exact version/notice.

## M2 security and PostgreSQL dependencies

Argon2, password-hash, base64, rand, SHA-2, subtle, UUID, chrono, SQLx 0.8.6,
Tokio Rustls, and their transitive dependencies are used for the M2 credential,
handover, and PostgreSQL foundation. They are distributed under the permissive
licenses accepted in `deny.toml`, including MIT, Apache-2.0, ISC, BSD-3-Clause,
Zlib, and CDLA-Permissive-2.0. `ring` includes Apache-2.0 and ISC-licensed
components; web PKI root-certificate data uses CDLA-Permissive-2.0. Exact crate
versions and packaged license texts are recorded by `Cargo.lock` and the Cargo
registry packages. No PostgreSQL client/server source is vendored.

## M2 runtime, configuration, and observability dependencies

Axum 0.8, config-rs 0.15, Clap 4, futures-util, humantime, Serde, tracing, and
tracing-subscriber are used for the M2 runtime, CLI, layered configuration, and
read-only admin endpoints. Their Cargo-packaged license texts and exact resolved
versions are recorded in `Cargo.lock`; all are covered by the permissive license
allowlist in `deny.toml`. `zeroize` clears canonical credential, CLI secret, and
bearer/session-key buffers on drop. No source from the ignored reference clones
is used at build or runtime.

## M3 capability filesystem dependency

cap-std 4 and its transitive capability/filesystem dependencies provide
capability-relative catalog access. Their exact versions and packaged license
texts are recorded in `Cargo.lock`; the graph is covered by `deny.toml`'s
permissive MIT, Apache-2.0, and Apache-2.0 WITH LLVM-exception allowlist.
