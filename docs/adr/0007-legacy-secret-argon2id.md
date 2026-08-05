# ADR-0007: legacy MD5 transport secret and Argon2id at rest

- Status: **Accepted**
- Date: 2026-08-05

## Context

The U.S. 852 client supplies a legacy MD5-shaped transport secret. Storing that
value, plaintext passwords, or bare MD5 digests would make a database disclosure
immediately useful. The client-facing representation cannot be strengthened
without breaking compatibility.

## Decision

Accept exactly 32 ASCII hexadecimal characters, canonicalize them to lowercase,
and hash those canonical bytes with the versioned scheme
`argon2id-client-md5-v1`. The PHC policy is Argon2id v19, 19,456 KiB memory, two
iterations, one lane, a canonical 16-byte random salt, and a 32-byte output.
Verification requires exactly the `m`, `t`, and `p` parameters and rejects
missing/wrong fields, extra `keyid`/`data`/unknown parameters, noncanonical salt
shape, wrong output length, and malformed PHC forms. Credential and token types
redact `Debug`; errors never contain presented or stored secret material.

Hash generation stays outside database transactions. Login runtime will execute
hashing/verification in a bounded `spawn_blocking` worker rather than blocking a
Tokio reactor thread.

Handover bearers consist of a nonsecret UUID selector and 256 OS-random bits in
unpadded URL-safe Base64. PostgreSQL stores only SHA-256 digest bytes. Consumers
lock by selector, compare digest bytes in constant time, validate target,
expiry, account status and revocation, mark consumed, and commit. Default expiry
is 60 seconds. The raw peer address is immediately reduced to a canonical IPv4
`/24` or IPv6 `/56` `SourceAddressPrefix`; only that privacy-minimized prefix is
persisted and indexed for future bounded abuse controls.

## Consequences

This protects the stored legacy verifier with an intentionally expensive modern
hash while retaining client compatibility. It does not make MD5 a suitable
password hash or authenticate the legacy TCP transport. Parameter changes need
a new scheme version and migration/rehash policy.
