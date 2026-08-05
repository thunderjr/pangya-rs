# M2 synthetic LoginService flow (provisional)

This document records **synthetic local evidence**, not U.S. 852 client
acceptance. The remaining real-client gates are response order, exact unknown
values, nickname limits, server-list acceptance, and handover field/length.

## Implemented state/order

1. Server writes the attributed 14-byte U.S. LoginService hello immediately;
   byte 6 contains a CSPRNG key in `0..=15`.
2. Client login `0x0001` is accepted only in `AwaitLogin`. Candidate legacy
   friendly result codes are 5100143 for invalid credentials and 5100107 for an
   authenticated duplicate LoginService connection. Exact fixtures/runtime
   tests cover their encoding; client acceptance remains external.
3. Server login result `0x0001` selects nickname (`0xd9`), provisional character
   (`0xda`), or success (`0x00`) state.
4. Nickname check `0x0007` receives provisional server opcode `0x000e`:
   opaque `u32` (`0` available, `1` unavailable in the synthetic policy), then
   the validated nickname echoed as a `u16`-length PangYa string. Meaning and
   ordering are explicitly capture-gated.
5. Nickname set `0x0006` uses the same provisional `0x000e` acknowledgement.
   No separate first-character acknowledgement was added because no current
   evidence requires one.
6. Once setup is complete, the server persists one digest-only 60-second
   GameService handover, sends empty chat macros `0x0006`, empty MessageService
   list `0x0009`, then configured GameService list `0x0002`.
7. Client select-server `0x0003` must match the configured `u16` server ID.
8. Server sends the synthetic bearer in the existing `0x0003` session-key
   packet: four zero unknown bytes, then the bearer as a `u16`-length string.
   This exact token field, length, and response position are provisional and
   must not be described as external-client compatible.

Synthetic fixture `crates/pangya-protocol/tests/fixtures/login-out-000e/` records
metadata and exact bytes for the nickname response. The token layout is exercised
by the real-PostgreSQL TCP E2E; token bytes are generated per run and never
stored as fixture/log data.
