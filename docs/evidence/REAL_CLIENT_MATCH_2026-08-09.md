# Real U.S. 852 versus-hole evidence — 2026-08-09

## Scope

This is manual compatibility evidence for `SPEC.md` §19.6 step 8. The legally held client and screenshots remain outside the repository. No packet bodies, credentials, client binaries, or client assets are committed.

- Client protocol version: `2016110200` / U.S. 852
- Server revision under test: worktree after `bd434af`, with the `MascotInfo` width correction described below
- Server config: ignored `local-data/run/retail.toml`
- Real client account: account 257
- Headless retail-wire second seat: account 149
- Course shown by the client: Blue Lagoon, hole 1, par 4

## Failure and reference-first diagnosis

Before this change the client reached the named course-loading screen and faulted at `0x00b65c25` while constructing player index 1's model. Player index 0 had a model. The roster writer emitted a 46-byte mascot block.

Three independent references require 62 bytes:

- `opensource-references/pangbox--server/pangya/player.go:181-197` defines the mascot as an eight-byte item, five unknown bytes, a 16-byte string, and 33 trailing bytes, then embeds it at the end of `PlayerData`.
- `opensource-references/pangbox--packetdoc/src/packets/gameservice/server/0076.ksy:79-85` gives the field immediately before start time a fixed width of 62 bytes.
- `opensource-references/Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/TYPE/pangya_game_st.h:1170-1185` defines packed `MascotInfo` with a 16-byte `SYSTEMTIME` between its type and flag.

The omitted `SYSTEMTIME` made the first roster entry look plausible but placed its start time, card count, and every later entry sixteen bytes early. Restoring those sixteen bytes makes the player-data block `0x2f82` bytes; with the leading two-byte seat, the client-visible record reaches the measured `0x2f84` boundary before start time.

## Manual run

1. Built release `pangya-server` and `pangya-test-client`.
2. Started the server against the real extracted U.S. catalog and PostgreSQL.
3. Used the Windows VM harness to launch and authenticate the client, open Multiplay, create room 1, and enter it.
4. Joined account 149 through `scripts/second-seat.sh --username rsp5 --room 1 --strokes 0`.
5. The real client started the match, completed its loading ramp, and rendered the playable Blue Lagoon tee with both player rows. It did not crash.
6. The real client sent first-shot-ready `0x0034` and received `0x0090`.
7. The host did not complete a stroke during this run. Its three-minute turn deadline expired; the server settled the headless seat as winner by forfeit and returned the real client to the room.

The durable result is match `fa42b311-5bab-433b-aa24-a19bbfa498ae`:

| account | completion | place | Pang | EXP |
|---:|---|---:|---:|---:|
| 257 | `turn_timeout` | 2 | 0 | 0 |
| 149 | `winner_by_forfeit` | 1 | 10 | 5 |

The match is `committed`, with one row for each participant. This proves that the real client now reaches the hole and that the shipped settlement path remains reachable. It does **not** yet prove a completed real-client stroke, a visible coherent results/balance screen, restart retention for this result, or replay resistance from a captured real finish frame. Those §19.6 items remain open.

## Automated evidence

- `cargo test -p pangya-protocol --all-features --locked` passes with `player_data_width_matches_the_reference_roster_stride`, pinning the `0x2f82` player-data width.
- `game_retail_two_players_play_and_settle_one_versus_hole` remains the full two-retail-wire server-side lifecycle proof.
