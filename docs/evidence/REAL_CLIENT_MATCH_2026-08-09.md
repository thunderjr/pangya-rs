# Real U.S. 852 versus-hole evidence — 2026-08-09

## Scope

This is manual compatibility evidence for `SPEC.md` §19.6 step 8. The legally held client and screenshots remain outside the repository. No packet bodies, credentials, client binaries, or client assets are committed.

- Client protocol version: `2016110200` / U.S. 852
- Initial load-fix revision: worktree after `bd434af`, with the `MascotInfo` width correction described below
- Completed-stroke revision: `7253485` plus the shot-meter exclusion/default update subsequently committed with this evidence
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

The match is `committed`, with one row for each participant. This proved that the real client reached the hole and that the shipped settlement path remained reachable. A later run closed the completed-stroke and retention gaps below.

## Completed real-client stroke and durable settlement

A second two-seat run used the same accounts and Blue Lagoon hole. The host had 8,612 Pang on the visible client header before play.

1. `Invoke-PangyaObservedShotMeter` sent the three DirectInput Space transitions while comparing the live meter against its reset pixels. The trace observed the power cursor advancing through `196, 289, 320, 418, ...` and the returning impact cursor through `207, 179, 153`.
2. The retail client completed the animation, changed its own `Shot` count from 0 to 1, moved the lie marker from 230y to 218y, entered `Wait Mode`, and handed the turn to account 149. This is a real rendered retail stroke, not a synthetic finish frame.
3. The second seat disconnected. The server atomically settled match `e1adcbaa-acda-4f07-85f9-406878cf00c2` with result commit key `069848e8-d2d8-4539-a1c9-a13b66954f6f`.

| account | strokes | completion | place | Pang | EXP | Pang after | EXP after |
|---:|---:|---|---:|---:|---:|---:|---:|
| 257 | 1 | `winner_by_forfeit` | 1 | 10 | 5 | 8,622 | 5 |
| 149 | 0 | `disconnect` | 2 | 0 | 0 | 216 | 105 |

PostgreSQL contains exactly one `currency_ledger` row (`+10`) and one `progression_ledger` row (`+5`) for the winning player result key `e711293f-ab09-4595-b5cb-e855ea44df5e`. After stopping and restarting the actual server, a fresh retail login visibly displayed 8,622 Pang. The value is non-negative and exactly the pre-match 8,612 plus the committed 10-Pang result. Equipment was retained too.

The disconnect-forfeit route returns directly to the room rather than displaying the normal holed-out standings overlay. Therefore the visible client-side evidence is the completed stroke, room return, and post-restart 8,622 balance; the exact result tuple is the immutable server settlement shown above.

## Replay resistance and automated evidence

- `cargo test -p pangya-protocol --all-features --locked` passes with `player_data_width_matches_the_reference_roster_stride`, pinning the `0x2f82` player-data width.
- `game_retail_two_players_play_and_settle_full_card` passes against PostgreSQL. It completes both retail-wire seats, verifies `0x0065`/`0x0066`, then resends both exact `0x0031` finish frames with a new transport salt. The database remains one match, two immutable player rows, and one Pang/EXP ledger row per player (`crates/pangya-server/tests/game_e2e.rs:6698-6750`). This directly exercises duplicate/replayed finish rejection at the retail opcode boundary.
- A restart does not create a second row or mutate the real-client result. The real settlement remains one match-player row and one Pang/EXP ledger mutation for account 257.
