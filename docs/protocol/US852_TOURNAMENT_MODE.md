# U.S. 852 Tournament mode — reference-derived specification

## Claim boundary

Nothing in this document has been observed on the wire. Every layout, opcode, and ordering
claim below is **derived from vendored open-source references** under
`opensource-references/`, and each is cited to the upstream file and line that supports it.
Where the references disagree, or where no reference covers a question, this document says
so explicitly rather than picking a plausible answer.

The one class of claim here that *is* first-hand is the real-client observation already
recorded in this repository ([`../RUNNING_THE_CLIENT.md`](../RUNNING_THE_CLIENT.md):429-431) about how the acquired U.S. 852 client gates the Start button in a **versus** room.
That observation is the single most load-bearing input to §2, and it does not yet cover
tournament rooms.

### Sources and licences

| Short name | Path under `opensource-references/` | Licence | Use here |
|---|---|---|---|
| pangbox/server | `pangbox--server` | ISC | Typed packet models, room actor, hole/turn/settlement flow |
| PacketDoc | `pangbox--packetdoc` | ISC | Opcode and binary-layout corpus (Kaitai); examples are **TH** captures |
| SuperSS | `Acrisio-Filho--SuperSS-Dev` | MIT | Only full tournament implementation available; a *much later* client generation |
| alter-pangya | `hex-agon--alter-pangya` | **no licence found** | Behavioural facts only (opcode values, field order). No code copied |
| Py_Source_US | `K4T--Py_Source_US` | GPL-3.0 | US-targeting enums/notes only. **No code may be copied into this workspace** |
| hsreina | `hsreina--pangya-server` | Apache-2.0 | Cross-check of the room-type enumeration |

Version caution that applies to the whole document: PacketDoc's examples are Thai captures,
SuperSS targets the FreshUp/SuperSS-era client, and Py_Source_US targets a US "S8" build.
Only pangbox/server and alter-pangya target the GB/US 852 generation this project supports.
Wherever a claim rests solely on SuperSS or Py_Source_US it is marked as such.

---

## 1. What selects Tournament, and what it changes

### 1.1 The selecting field is `room_type`

A room's mode is one byte, present in three places on the wire, with the same enumeration in
each:

| Packet | Direction | Field | Evidence |
|---|---|---|---|
| `0x0008` room create | C→S | `room_type` | `pangbox--packetdoc/src/packets/gameservice/client/0008.ksy:37-39`, enum `:57-61` |
| `0x000a` room settings change, sub-type `0x02` | C→S | `room_mode` | `.../client/000a.ksy:46`, `:63-68`, `:116-117` |
| `0x0047` room list | S→C | `room_type` | `.../server/0047.ksy:55-57`, enum `:119-123` |
| `0x004a` room settings announce | S→C | `room_type` | `.../server/004a.ksy:24-26`, enum `:61-65` |

The enumeration PacketDoc records, and which `pangbox/server` and `alter-pangya` both use:

| Value | Mode | Corroboration |
|---:|---|---|
| `0x00` | Versus | `.../server/004a.ksy:62`; `hex-agon--alter-pangya/.../room/RoomType.kt:13`; `hsreina--pangya-server/src/defs.pas:27` |
| `0x02` | Chat / lounge | `.../server/004a.ksy:63`; `RoomType.kt:14` |
| `0x04` | **Tournament** | `.../server/004a.ksy:64`; `RoomType.kt:15`; `hsreina--pangya-server/src/defs.pas:31`; `K4T--Py_Source_US/Src/Py_Game/Py_Game/Defines/PangyaEnums.cs:456`; `Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/TYPE/pangya_game_st.h:2364` |
| `0x0a` | Pang Battle | `.../server/004a.ksy:65`; `RoomType.kt:16` |

This project already carries exactly this enumeration:
`crates/pangya-protocol/src/us852_room.rs:41-63` (`RetailRoomType::Tournament = 0x04`).
**No protocol change is needed to name the mode** — only to act on it.

Divergence to record, not to resolve here: the *older* references disagree on the values
above `0x04`. `hsreina--pangya-server/src/defs.pas:31-34` and
`K4T--Py_Source_US/.../PangyaEnums.cs:456-459` list `0x05` team tournament, `0x06` guild
battle, `0x07` Pang Battle, whereas PacketDoc/pangbox/alter-pangya put Pang Battle at
`0x0a`. SuperSS (`pangya_game_st.h:2360-2375`) matches the former family. Only `0x00`,
`0x02` and `0x04` are agreed across every reference, and only those three are in scope here.

### 1.2 `room_type` is written twice, and the two slots are not the same thing

`alter-pangya` distinguishes a room type's **id** from its **UI type**:

```
work/fking/pangya/game/room/RoomType.kt:9-17
    val uiType: Int = id,
    val extendedInfo: Boolean = false, // ... this + id in the RoomInfo packet affects which UI is displayed
    VERSUS(0, matchDirector = noopMatchDirector(), extendedInfo = true),
    TOURNAMENT(4, matchDirector = noopMatchDirector()),
    PRACTICE(19, uiType = 4, matchDirector = PracticeMatchDirector());
```

The room record writes `uiType` at one offset and `id` at another —
`.../room/Room.kt:177` and `.../room/Room.kt:189` — and the room settings announce
(`.../room/RoomSettings.kt:75`), the match start `0x0076` and the match plan `0x0052`
(`.../packet/outbound/MatchReplies.kt:42`, `:53`) all carry `uiType`.

This matters twice over:

1. This project currently writes the *same* value into both slots of the room record —
   `crates/pangya-protocol/src/us852_room.rs:212` and `:224`. For Tournament (`4`/`4`) that
   is coincidentally correct, so tournament work does not force the split. It is wrong for
   any type where the two differ, and `PRACTICE` is exactly such a type upstream.
2. **`alter-pangya`'s single-player practice room advertises `uiType = 4`, i.e. it presents
   to the client as a tournament, and is driven by a match director that speaks the
   tournament packet family** (`PracticeMatchDirector.kt`, using `tourneyShotAck`,
   `tourneyShotGhost`, `tourneyUpdatePlayerProgress`, `tourneyEndingScore`,
   `tourneyWinnings`, `tourneyTimeout`). This is the strongest available evidence that a
   GB/US 852 client will run a tournament-shaped match with a single occupant.

### 1.3 Which other room-record fields Tournament changes

| Field | Versus / Chat | Tournament / Battle | Evidence |
|---|---|---|---|
| `shot_timer_ms` | the live per-shot timer | present but unused | `.../client/0008.ksy:28-33`; `.../server/004a.ksy:44-49` |
| `game_timer_ms` | present but unused | the live whole-game timer | same lines; `.../client/000a.ksy:95-100` |
| `max_players` valid values | 2, 3, 4 | **10, 20, 30** | `.../client/000a.ksy:90-94` |
| `game_timer` valid values | n/a | 9 holes: 15/20/25/30 min; 18 holes: 30/35/40/45/50; short: 15/20/25/30/35 | `.../client/000a.ksy:95-100` |
| settings sub-type `0x02` (mode) | works | **"Does not work in Tournaments or Battles"** | `.../client/000a.ksy:64` |
| `unknown_special_room_value` (4 bytes) | `00 00 00 00` | `00 00 xx 2C` | `.../server/0047.ksy:72-74`; `.../server/004a.ksy:50-52` |
| `hole_count` | 3, 6, 9, 18 | same set | `.../client/000a.ksy:74-78` |
| `hole_progression` | 0 front / 1 back / 2 random / 3 shuffle | same set | `.../server/004a.ksy:87-91` |

The `00 00 xx 2C` byte pattern is worth flagging: `alter-pangya` writes a *trophy* item id
at the equivalent offset in both the room record (`Room.kt:183`, `trophyIffId()`) and the
match plan (`MatchReplies.kt:56`), and hard-codes `0x2C000000` as a trophy id in the
tournament results packet (`MatchReplies.kt:190`). `0x2C` is the high byte of an IFF item
family. That makes "the four bytes are a trophy catalog id, zero outside tournaments" the
best-supported reading, and it is consistent with this project already naming that slot
`// trophy catalog id` (`crates/pangya-protocol/src/us852_room.rs:218`,
`crates/pangya-protocol/src/us852_match.rs:120`). It remains **unconfirmed**: PacketDoc
labels it `unknown_special_room_value` and gives no field name.

`hole_progression` is *not* a mode discriminator — it is orthogonal to `room_type` and takes
the same four values in both. `pangbox/server` applies it identically regardless of mode
(`game/room/room.go:458-468`).

**Unknown:** nothing in any reference identifies a per-room flag that distinguishes
"tournament" from "team tournament" beyond `room_type` itself, nor any field that carries
tournament *class* (Amateur/Pro tiers) into the room. Trophy classes exist on the account
side (`.../server/0159.ksy:28-32`, thirteen classes) with no visible link to a room field.

---

## 2. The start condition

### 2.1 What is certain

The start request is client opcode `0x000E`, and it carries no interesting body:
`pangbox--server/game/packet/client.go:38` (`ClientPlayerStartGame`) and
`game/server/conn.go:278-281`. This project already decodes it as
`RETAIL_C2S_START_MATCH` (`crates/pangya-game/src/lib.rs:5954`).

Neither GB/US-852-targeting reference imposes *any* participant-count gate on it:

- `pangbox--server/game/room/room.go:448-549` — `handleRoomStartGame` reads no player count
  before building the hole plan and broadcasting `0x0230`/`0x0231`/`0x0077`/`0x0076`/`0x0052`.
- `hex-agon--alter-pangya/.../packet/handler/match/StartGamePacketHandler.kt:8-13` — the
  whole handler is `room.startGame()`.

So on the U.S. 852 generation, the count rule — if any — is enforced **by the client, or by
server policy, not by the protocol**.

### 2.2 What the client is known to do — for versus

This repository's own real-client run
([`../RUNNING_THE_CLIENT.md`](../RUNNING_THE_CLIENT.md):429-431) records:

> A real client will not start a versus room that holds fewer players than its capacity, and
> the Make Room dialog's smallest versus capacity is two: the room header reads `3 hole (1/2)`
> and the master's button stays on Ready rather than becoming Start.

That is a *client-side* gate: the button never becomes Start, so `0x000E` is never sent. It
is also the reason blocker 23 in [`../PROGRESS.md`](../PROGRESS.md) exists.

### 2.3 Why Tournament is expected to differ

A tournament room's smallest capacity is **10** (`.../client/000a.ksy:90-94`). If the client
applied the same "must be full" rule to tournaments, a tournament would need ten
simultaneous clients, which is not how the retail game behaved. Something must relax the
rule for `room_type == 0x04`. Two readings fit the evidence:

- **(A) The client does not gate tournament rooms at all.** Supported by `alter-pangya`
  running a single-occupant room that advertises `uiType = 4` through the tournament packet
  family, apparently against a real client (`RoomType.kt:17`, `PracticeMatchDirector.kt`).
- **(B) The client gates on "more than one occupant", not on "full".** Supported by
  SuperSS's workaround, below.

### 2.4 The SuperSS counter-evidence, read carefully

SuperSS refuses a one-player start for every mode except practice and Grand Prix/Zodiac —
**tournament is not exempt**:

```
Acrisio-Filho--SuperSS-Dev/Server Lib/Game Server/GAME/room.cpp:3560-3563
    if (!m_bot_tourney && v_sessions.size() == 1 && m_ri.tipo != RoomInfo::PRACTICE && m_ri.tipo != RoomInfo::GRAND_PRIX
        && m_ri.tipo != RoomInfo::GRAND_ZODIAC_INT && m_ri.tipo != RoomInfo::GRAND_ZODIAC_ADV && m_ri.tipo != RoomInfo::GRAND_ZODIAC_PRACTICE)
        throw exception(... "nao tem quantidade de jogadores suficiente para da comecar" ...);
```

It then offers a paid escape hatch. `room.h:332` names the flag
`bool m_bot_tourney; // Bot para começa o Modo tourney só com 1 jogador` — "bot to start
tournament mode with only one player". `room.cpp:6091-6146` (`addBotVisual`) fabricates a
room member — nickname `Bot`, `ready = 1`, slot `0` — broadcasts it into the room census,
and sets the flag; `room.cpp:1331-1345` gates that behind premium status or a consumable
"bot ticket". The readiness check is bypassed for the same case
(`room.cpp:5942-5944`), and the fake member is removed when the game ends
(`room.cpp:6028-6046`).

Two competing conclusions follow, and the evidence does not choose between them:

- The bot exists purely to satisfy SuperSS's *own* server-side check (which the same commit
  edits with `!m_bot_tourney &&`), and the *visual* census entry is cosmetic — an opponent on
  the scoreboard. Under this reading a stock client is happy with one occupant.
- Or the census entry is load-bearing because the client itself counts occupants. Under this
  reading the server must present a second room member for the button to become Start.

The word SuperSS uses for the census entry is "só no visual" / "só visual" — *visual only*
(`room.cpp:6129`, `:6036`) — which leans towards the first reading, but that is the author's
comment about intent, not a statement about the client.

### 2.5 How a start refusal is communicated

`K4T--Py_Source_US/Src/Py_Game/Py_Game/Defines/PangyaEnums.cs:436-449` documents a
server-sent code set with the client-visible strings:

```
//01  || not enough players to start the game
//02  || there are not enough players to start the game
//07  || failed game already started
//08  || you need to update pangya to the latest version
TGAME_PLAY : NOT_ENOUGH_PLAYERS = 1, ENOUGH_PLAYERS = 2, START_GAME_FALIED = 07, UPDATE_GAME = 08
```

SuperSS sends its start failures on opcode `0x0253` with a `u32` code
(`room.cpp:3772-3781`). **This conflicts with PacketDoc**, which documents `0x0253` as
"Event Room Join Response" (`.../server/0253.ksy:5-19`). The carrier opcode for a start
refusal on U.S. 852 is therefore **unknown**; only the code *semantics* are corroborated.

### 2.6 Recommended server behaviour, stated as policy

Given the above, this project should treat the start gate as an explicit, configurable
server policy rather than an inferred protocol rule:

- Accept `0x000E` from the room owner only (`room.cpp:3544-3546` and
  `pangbox--server/game/room/room.go:410-412` both enforce owner-only for settings/start).
- For `room_type == 0x04`, require `members >= 1` — i.e. impose no additional gate.
- Do **not** attempt SuperSS's fake-member workaround unless a real client proves it
  necessary. Fabricating a roster entry that maps to no account would put a non-participant
  into the census that the match aggregate has no identity for, and this project's roster is
  captured at start and validated (`crates/pangya-game/src/stroke_state.rs:41-74`).
- If the button turns out never to become Start with one occupant, the correct fix is a
  *configured* minimum, not a fake player; see §7 open item T-1.

---

## 3. Opcode sequence for a tournament round

The two GB/US-852 references disagree in detail with SuperSS, which is a later client. The
table records the SuperSS sequence, because it is the only complete tournament
implementation available, and marks each row with its GB/US-852 corroboration.

### 3.1 Start (room → loading)

| # | Dir | Opcode | Cast | Meaning | Evidence |
|---:|---|---|---|---|---|
| 1 | C→S | `0x000E` | — | Start match | `pangbox--server/game/packet/client.go:38`; `.../game/server/conn.go:278` |
| 2 | S→C | `0x0230` | broadcast | unclassified pre-start marker | `pangbox--server/game/room/room.go:483`; `alter-pangya MatchReplies.kt:20-24`; SuperSS `room.cpp:3756-3758` |
| 3 | S→C | `0x0231` | broadcast | unclassified pre-start marker | `room.go:484`; `MatchReplies.kt:26-30`; `room.cpp:3760-3762` |
| 4 | S→C | `0x0077` | broadcast | Pang rate, `u32`; pangbox sends `0x64`, SuperSS sends the server's configured rate | `room.go:485`; `MatchReplies.kt:32-37`; `room.cpp:3764-3770` |
| 5 | S→C | `0x0076` | broadcast | Game init: `u8 room_ui_type`, `u32 = 1`, 16-byte packed start time | SuperSS `tourney_base.cpp:84-92`; `alter-pangya MatchReplies.kt:39-48`; pangbox uses a richer per-player variant, `game/packet/server.go:301-324` |
| 6 | S→C | `0x0052` | per player | Match plan: course, `room_ui_type`, hole mode, hole count, trophy id, shot timer, game timer, 18 hole records, seed, then per-hole collectible counts | `alter-pangya MatchReplies.kt:49-77`; `pangbox--server/game/packet/server.go:509-528`; SuperSS sends the equivalent through `0x113` sub-type 4 (`tourney_base.cpp:124-140`) |

Rows 1-6 are already implemented here for the solo case:
`crates/pangya-game/src/lib.rs:3948-3973` (`RetailMatchStart` + `RetailMatchInfo`) and
`crates/pangya-protocol/src/us852_match.rs:51-134`. **Tournament needs no new packet type
for the start** — only `room_ui_type`, `hole_mode`, `hole_count` and the hole vector filled
from real room settings instead of the hard-coded `0`/`0`/`1`/one-hole plan at
`lib.rs:3956-3973`.

Note pangbox's `0x0076` differs materially from SuperSS's and alter-pangya's: it carries a
full per-player roster (`GameInitFull`, `game/packet/server.go:301-324`). This project
implements the short form (`us852_match.rs:60-74`), which matches the two later references.
**Unknown** which form U.S. 852 expects; the short form has not been rejected by a client
because it has not yet been sent to one.

### 3.2 Per-hole load

| # | Dir | Opcode | Cast | Meaning | Evidence |
|---:|---|---|---|---|---|
| 7 | C→S | `0x0048` | — | Load progress, `u8` percent | `pangbox--server/game/packet/client.go:66`; `alter-pangya ClientPacketType.kt:73` |
| 8 | S→C | `0x00A3` | broadcast | Load progress relay | `pangbox--server/game/packet/server.go:71`, `game/room/room.go:551-556` |
| 9 | C→S | `0x001A` | — | Hole info: hole number, 2× `u32`, par, tee `(x,z)`, pin `(x,z)` | `alter-pangya MatchHoleStartPacketHandler.kt:14-20`; SuperSS `tourney_base.cpp:148-176`; pangbox `game/server/conn.go:441-451` |
| 10 | S→C | `0x009E` | **unicast** | Weather: `u16` ordinal, `u8` option | SuperSS `tourney_base.cpp:211-216` (`session_send`); `alter-pangya MatchReplies.kt:79-85` |
| 11 | S→C | `0x005B` | **unicast** | Wind: strength `u8`, silent flag `u8`, bearing `u16`, `1` = set / `0` = add | SuperSS `tourney_base.cpp:221-229`; `MatchReplies.kt:88-96` |
| 12 | S→C | — | unicast | Remaining game time | SuperSS `tourney_base.cpp:232` (`sendRemainTime`) |
| 13 | C→S | `0x0011` | — | Hole load finished | SuperSS `PACKET/packet_func_sv.cpp:515-527`; `pangbox--server/game/packet/client.go:41`; `alter-pangya ClientPacketType.kt:56` |
| 14 | S→C | `0x0053` | **unicast** | Play this player's hole intro | SuperSS `tourney_base.cpp:271-275` (`session_send`); `MatchReplies.kt:108-114` |
| 15 | C→S | `0x0034` | — | Char intro finished | SuperSS `packet_func_sv.cpp:1040-1051`; `alter-pangya ClientPacketType.kt:71` |
| 16 | S→C | `0x0090` | unicast | Intro ack | `pangbox--server/game/server/conn.go:366-367`; `MatchReplies.kt:120-124` |

**The unicast/broadcast split is the defining difference from versus.** pangbox broadcasts
weather, wind and start-hole to the whole room (`game/room/room.go:896-916`), because in
versus everyone plays the same hole at the same time. SuperSS unicasts all three in a
tournament, because each player is on their own hole, on their own clock.

**There is no `0x0063` active-user announce in a tournament.** pangbox sends it every turn
(`game/room/room.go:731-733`); SuperSS's tournament path never emits it. Tournament is
simultaneous play — every participant is always "active".

### 3.3 Per shot

| # | Dir | Opcode | Cast | Meaning | Evidence |
|---:|---|---|---|---|---|
| 17 | C→S | `0x0013` | — | Aim rotate | `pangbox--server/game/packet/client.go:44` |
| 18 | S→C | `0x0056` | **unicast (echo to sender)** | Aim rotate | SuperSS `tourney_base.cpp:593-597` (`session_send`); PacketDoc `.../server/0056.ksy:15` "not relayed during tournaments" |
| 19 | C→S | `0x0015` / `0x0016` | — | Power / club change | `client.go:46-47` |
| 20 | S→C | `0x0058` / `0x0059` | **unicast** | Power / club change | SuperSS `tourney_base.cpp:648`, `:675`; PacketDoc `.../server/0058.ksy:15`, `.../server/0059.ksy:15` |
| 21 | C→S | `0x0017` | — | Item use | `client.go:48` |
| 22 | S→C | `0x005A` | **disputed** | Item use | SuperSS **broadcasts** (`tourney_base.cpp:722`, `:761`); `K4T--Py_Source_US/.../Game/GameBase.cs:965-971` **unicasts** when `GameType == TOURNEY`. `alter-pangya` broadcasts (`PracticeMatchDirector.kt:65-67`) |
| 23 | C→S | `0x0012` | — | Shot commit | `client.go:42` |
| 24 | S→C | `0x0055` | **not sent** | Shot commit relay | SuperSS's tournament path emits no `0x0055` at all; PacketDoc `.../server/0055.ksy:15` "not relayed during tournaments/simultaneous play" |
| 25 | C→S | `0x001B` | — | Shot sync: conn id, `x,y,z`, state, bunker, unknown, `pang`, `bonus_pang`, camera flags, shot flags, frames, GP penalties | `alter-pangya MatchPlayerShotSyncPacketHandler.kt:13-27` |
| 26 | S→C | `0x006E` | **broadcast** | Ghost marker: conn id, hole `u8`, `x`, `z`, shot flags, frames | SuperSS `tourney_base.cpp:2118-2140`; `alter-pangya MatchReplies.kt:160-172` ("used for drawing the white dots across the map"); PacketDoc `.../server/006e.ksy` |
| 27 | C→S | `0x012F` | — | Shot end location data | SuperSS `packet_func_sv.cpp:3220-3231`; `alter-pangya ClientPacketType.kt:91` (`MATCH_PLAYER_TOURNEY_SHOT`) |
| 28 | S→C | `0x01F7` | **broadcast** | Tournament shot summary: conn id, hole, shot geometry blob | SuperSS `tourney_base.cpp:502-512`; `alter-pangya MatchReplies.kt:174-181`; PacketDoc `.../server/01f7.ksy` |
| 29 | C→S | `0x001C` | — | Shot end / turn end | `client.go:43`; SuperSS `packet_func_sv.cpp:755-767` |
| 30 | S→C | `0x00CC` | **unicast** | Shot end, carrying collected coin/cube drops | SuperSS `tourney_base.cpp:2140-2156` |

Reading of §3.3 for implementation: **in a tournament the only shot packets another player
sees are `0x006E` and `0x01F7`.** Everything else is echoed to the shooter alone. This is
exactly why PacketDoc's shot-announce pages all carry the same sentence, and it is the
single largest behavioural difference from the two-player stroke lifecycle this project is
building.

`0x012F`/`0x01F7` is a **U.S. 852 unknown**: `0x012F` is absent from PacketDoc's client
index entirely, and SuperSS's own comment
(`GAME/coin_cube_location_update_system.cpp:64`) says `0x12F` is a *later* replacement that
"only works in tournament(base)", implying an earlier generation did this differently.

### 3.4 Per hole

| # | Dir | Opcode | Cast | Meaning | Evidence |
|---:|---|---|---|---|---|
| 31 | C→S | `0x0031` | — | Hole statistics, cumulative `user_course_result_data` | PacketDoc `.../client/0031.ksy`; `pangbox--server/game/server/conn.go:354-359` |
| 32 | S→C | `0x006D` | **broadcast** | Tournament user update: conn id, hole `u8`, total strokes `u8`, score `i32`, pang `u64`, bonus pang `u64`, finished-hole flag `u8` | SuperSS `tourney_base.cpp:2045-2064`; `alter-pangya MatchReplies.kt:214-225`; PacketDoc `.../server/006d.ksy` (which reads the two 8-byte fields as `u32` + 4 unknown) |
| 33 | S→C | `0x0199` | **unicast** | Sent when the player holes out on the *last* hole | SuperSS `tourney_base.cpp:2255-2257`; `pangbox--server/game/packet/server.go:93` (`ServerRoomPlayerFinished`, empty body, `:651-653`) |

**`0x0065` (versus "finish hole") is not part of the tournament flow.** pangbox broadcasts it
when the *room* advances a hole (`game/room/room.go:770`); a tournament has no shared hole
to advance. `alter-pangya` names it `versusFinishHole` (`MatchReplies.kt:155-160`) and its
practice/tournament director never calls it. This project currently sends `RetailFinishHole`
(`0x0065`) on the solo path (`crates/pangya-game/src/lib.rs:4499`,
`crates/pangya-protocol/src/us852_match.rs:415-430`), which is **probably wrong for a
tournament-shaped room** and is a specific thing to check against a client.

Layout conflict to note on `0x006D`: PacketDoc reads `user_pang` and `user_bonus` as `u32`
with a 4-byte unknown between them (`.../server/006d.ksy:27-34`); SuperSS and alter-pangya
both write `u64` fields (`tourney_base.cpp:2058-2059`, `MatchReplies.kt:221-222`). The
totals match (`4+4+4 == 8+4`, then 5 trailing bytes vs 1). Resolve by fixture, not by
argument.

### 3.5 End of round

| # | Dir | Opcode | Cast | Meaning | Evidence |
|---:|---|---|---|---|---|
| 34 | S→C | `0x006C` | **broadcast** | Player finished (`2`) or quit (`3`) | SuperSS `tourney_base.cpp:2161-2170`; PacketDoc `.../server/006c.ksy` |
| 35 | S→C | `0x0040` sub-type `0x11` | broadcast | Completion notice: nickname, score, pang, assist flag — immediately precedes `0x006C` | PacketDoc `.../server/0040.ksy:63-82` |
| 36 | C→S | `0x0006` | — | Match statistics, whole-match `user_course_result_data` | PacketDoc `.../client/0006.ksy` |
| 37 | S→C | `0x0045` | unicast | User statistics response | PacketDoc `.../client/0006.ksy:19-20` |
| 38 | S→C | `0x0133` | unicast | Treasure-point result. **In tournaments only the local player's items appear**, because each player has their own meter | PacketDoc `.../server/0133.ksy:16-18`; SuperSS `tourney_base.cpp:2211-2228` |
| 39 | S→C | `0x0134` | unicast | Treasure-point winnings, always sent | PacketDoc `.../server/0134.ksy:13-17` |
| 40 | S→C | `0x00CE` | unicast | **Tournament item winnings — tournament only** | PacketDoc `.../events/match_end.md:20-21`, `.../server/00ce.ksy`; SuperSS `tourney_base.cpp:2172-2186`; `alter-pangya MatchReplies.kt:203-210` |
| 41 | S→C | `0x0079` | unicast | Tournament placard: exp `u32`, trophy id `u32`, trophy won `u8`, winning team `u8`, 12 medal records, medal totals | SuperSS `tourney_base.cpp:2188-2209`; `alter-pangya MatchReplies.kt:186-201`. **Absent from PacketDoc entirely** |
| 42 | S→C | `0x0216` | unicast | User status update (items/achievements/mastery) | PacketDoc `.../events/match_end.md:11`, `.../server/0216.ksy` |
| 43 | S→C | `0x022E` / `0x0220` | unicast | Achievement unlocked / updated | `.../events/match_end.md:12-13` |
| 44 | S→C | `0x00C8` | unicast | Pang balance | `.../events/match_end.md:15`, `.../server/00c8.ksy` |
| 45 | S→C | `0x004A` | unicast | Room settings announce — the room is back in its waiting area | `.../events/match_end.md:23-25` |
| — | S→C | `0x008C` | unicast | Game timer expired | SuperSS `tourney_base.cpp:2230-2233`; `alter-pangya MatchReplies.kt:212` (`tourneyTimeout`). Absent from PacketDoc |

`0x00FA` (room bonus collectables) fires for a *match* and **not** for a tournament
(`.../events/match_end.md:17-18`), and `0x0066` — the versus standings packet this project
already implements as `RetailMatchFinish`
(`crates/pangya-protocol/src/us852_match.rs:371-413`) — is **not** in PacketDoc's match-end
event list and is not emitted by SuperSS's tournament path. A tournament's results screen is
built from `0x0079` + `0x00CE` + the per-player `0x006D` history, not from `0x0066`.

Early exit ("Tiki Report") is a real tournament feature and has its own opcode: client
`0x00AA` behaves as `0x0006` + `0x000F` + a simulated early end
(PacketDoc `.../client/00aa.ksy:13-22`), answered by `0x012A`, `0x0045`, `0x004C`, and the
whole match-end event. SuperSS implements it as `requestUseTicketReport`
(`tourney.cpp:325-…`, dispatched at `packet_func_sv.cpp:2084-2095`). It is out of scope for a
first tournament implementation but should be an explicitly refused opcode, not an ignored
one.

---

## 4. Scoring and rewards

### 4.1 Score

Score is strokes minus par, accumulated across holes. pangbox does it on the client's
hole-end packet:

```
pangbox--server/game/room/room.go:638-646
    pair.Value.Score += int32(pair.Value.Stroke) - int32(r.currentHole().Par)
    pair.Value.LastTotal = pair.Value.Stroke
    pair.Value.Stroke = 0
```

with par taken from the client's own hole-info packet (`room.go:687-698`, whose comment
concedes "It'd probably be better to not rely on the client for this if possible"). **This
project must not adopt that**: par is server-owned here (`OneHoleConfig::par`,
`crates/pangya-domain/src/lib.rs:1149-1199`) and configured, per the note in
[`../PROGRESS.md`](../PROGRESS.md) that course par is operator-declared.

Placement in a tournament is a plain sort by score. pangbox's versus tie rule — tied players
share the better place — is at `game/room/room.go:849-861` and is the right default.

### 4.2 Pang

Both references treat play-Pang as **client-reported and server-recorded**: it arrives inside
the `0x001B` shot sync (`alter-pangya MatchPlayerShotSyncPacketHandler.kt:20-21`;
`pangbox--server/game/room/room.go:666-668`, which copies `ShotSync.Pang` straight onto the
player). SPEC §12.4 forbids that outright — "The server MUST NOT credit client-claimed Pang,
bonus Pang, EXP, or items directly" — so this project must keep computing Pang server-side
from the settled stroke count, exactly as the solo and stroke paths already do.

### 4.3 Course clear bonus

```
pangbox--server/gameconfig/config.go:140-149
    return uint64(bonusRate * numHoles * (numPlayers - 1))
```

with the comment "TODO: this is probably only true for versus", applied at
`game/room/room.go:812-813` alongside `exp := int(clearBonus / 2)`.

For a **one-player tournament this formula yields zero**, which is almost certainly wrong for
tournaments and is flagged as such by its own author. SuperSS computes the clear bonus
differently and only for the player who finished the final hole, from static map data and the
hole count, gated on a `clear_bonus` display flag the client sets
(`tourney_base.cpp:2258-2275`, `sMap::calculeClear30s(*map, m_ri.qntd_hole)`).

**Recommendation:** define a tournament clear bonus that does not multiply by
`(players - 1)`, document it as `tourney-v1`, and keep it entirely server-side. Do not port
pangbox's formula.

### 4.4 EXP

SuperSS's tournament EXP is a placement-scaled function of field size, holes completed and
course difficulty:

```
Acrisio-Filho--SuperSS-Dev/.../GAME/tourney.cpp:533-536 (and the two identical branches at :545, :554)
    exp = (int)(1 * m_player_order.size() * (hole_seq > 0 ? hole_seq : 0) * stars);
    exp = (int)(exp * <item exp rate> * <server exp rate>);
    exp = (int)(exp * (1 - (i / m_player_info.size())));
```

where `i` is the finishing rank. Note that with one player the placement factor is
`1 - (0/1) = 1` and the field-size factor is `1`, so a solo tournament still earns EXP
proportional to holes completed × course stars. That is a usable shape for a `tourney-v1`
formula. A player at the level cap earns none (`tourney.cpp:538`).

### 4.5 Trophies and medals

Both are field-size gated, and **a single-player tournament earns neither**:

| Award | Threshold | Evidence |
|---|---|---|
| Medals | ≥ 18 players in the game | `tourney.cpp:603-604` |
| Trophies, 18 holes | ≥ 10 players; 10-14 → 1 bronze, 15-18 → +1 silver, 19-22 → +1 gold, 23-26 → 2 bronze, 27-30 → 2 silver + 3 bronze | `tourney.cpp:744-762` |
| Trophies, 9 holes | ≥ 15 players; 15-18 → 1 bronze, 19-26 → +1 silver, 27-30 → +1 gold | `tourney.cpp:763-775` |

The account-side trophy counters live in `0x0159` — 13 classes × (gold, silver, bronze) as
`u16` each (`.../server/0159.ksy:28-45`) — answered to a `0x002F` user-info request, with
Grand Prix trophies counted separately in `0x0257`.

**Implication for this project:** a tournament implementation aimed at a one- or two-player
local server needs **no trophy or medal persistence at all**. `0x0079` can carry a zero
trophy id and twelve zeroed medal records, exactly as `alter-pangya` does
(`MatchReplies.kt:186-201`), and that is a truthful report of "you won nothing", not a stub.

### 4.6 Ranking

No reference computes a durable tournament ranking server-side beyond the per-match
standings and the trophy counters above. Course records and player statistics remain the
Tier C item SPEC §3.1 already lists; nothing tournament-specific is required for them.

---

## 5. Multi-hole progression

### 5.1 Versus advances the room; tournament advances each player

pangbox's versus loop is room-scoped: when everyone has holed out, `endHole` advances
`CurrentHole`, broadcasts `0x0065`, clears per-player hole flags and re-sorts turn order
(`game/room/room.go:767-801`); the client then re-sends `0x0011`, which drives `startHole`
again and re-broadcasts weather/wind/`0x0053` (`room.go:558-566`, `:896-950`).

A tournament has **no room-level current hole**. SuperSS's per-shot check is per player:

```
Acrisio-Filho--SuperSS-Dev/.../GAME/tourney_base.cpp:2237-2286  (checkEndShotOfHole)
    if (pgi->shot_sync.state_shot.display.stDisplay.acerto_hole || pgi->data.giveup) {
        if (m_course->findHoleSeq(pgi->hole) == m_ri.qntd_hole) { <send 0x199, add clear bonus> }
        finishHole(_session);
        changeHole(_session);
    }
```

and `Tourney::changeHole` (`tourney.cpp:246-254`) either finishes that player's tournament or
broadcasts `0x006D` with the finished flag set. **There is no packet that tells the client
"go to hole N".** The client owns hole advance: it has the whole 18-slot plan from `0x0052`
up front, and after `0x006D` it simply starts the next hole by sending `0x001A` again. The
server answers with that player's weather/wind/`0x0053`.

That is why §3.2 rows 9-14 are a *loop*, not a prologue.

### 5.2 State a multi-hole tournament match must carry

Per match (immutable after start):

- ordered hole plan: for each of `hole_count` holes — hole number, pin variant, course, a
  per-hole random id, and the server-owned **par**;
- per-hole weather and wind derived from the match seed;
- match-wide random seed;
- hole progression mode and the derived hole ordering
  (`pangbox--server/game/room/room.go:453-479` shows all four modes);
- game timer deadline and start instant;
- captured roster.

Per participant (mutable):

- current hole index into the plan;
- strokes on the current hole, and cumulative strokes;
- cumulative score relative to par;
- per-hole completion set (which holes are settled), so a replayed `0x0031` is idempotent;
- terminal state: still playing / finished / forfeit / quit / timed out;
- server-computed pang and bonus pang accumulators.

Two invariants fall out that the existing exactly-two aggregate does not have:

1. **Participants progress independently.** There is no shared turn and no shared hole, so
   `active`, `turn`, and `turn_generation` in `StrokeMatchState`
   (`crates/pangya-game/src/stroke_state.rs:322-331`) have no tournament analogue. The
   per-turn deadline is replaced by the single whole-game deadline.
2. **Settlement is per player, at different wall-clock times.** SuperSS finishes each player
   as they hole out on the last hole and only runs the shared `finish()` once
   `AllCompleteGameAndClear()` holds (`tourney.cpp:265-317`). The aggregate must therefore
   support "this participant is terminal, settle their line" separately from "the match is
   over".

Both of these are structural, not incremental, which is the main input to §6.

---

## 6. Does this warrant a new aggregate?

**Yes — a new `TournamentMatchState`, not an extension of `StrokeMatchState`.**

`StrokeMatchState` is built around three things a tournament does not have:

- a fixed `[PlayerState; 2]` array and `[PlayerConnectionId; 2]` roster
  (`crates/pangya-game/src/stroke_state.rs:322-331`, `:41-74`);
- a single active player and a global turn counter, with `InvalidTurn` rejection on every
  relay (`stroke_state.rs:576-578`, `:616-618`);
- per-turn deadlines with generation fencing (`stroke_state.rs:759-810`).

Tournament inverts all three: 1..=30 participants, no turn ownership at all, and one
whole-game deadline. Bending the stroke aggregate to cover both would turn `InvalidTurn`
into a mode-conditional no-op — which is precisely the kind of shared-path defect blocker 23
in [`../PROGRESS.md`](../PROGRESS.md) records ("it would score one player and ignore the
other").

What *should* be shared:

- `deterministic_conditions` and the seed contract
  (`crates/pangya-game/src/match_state.rs:33-50`) — extended to derive **per-hole**
  conditions rather than one pair;
- `RelayDisposition` and the exact-replay/conflicting-replay discipline
  (`match_state.rs:139-145`, `stroke_state.rs:552-585`);
- the reserve → confirm → loading → in-game → results-pending → commit phase skeleton and its
  abort side-state, which both existing aggregates already share in shape;
- the domain identity types: `MatchId`, `MatchResultKey`, per-player `player_result_key`
  (`crates/pangya-domain/src/lib.rs:1750-1790`).

---

## 7. Staged implementation plan

Each stage ends with something provable. Stages are ordered so nothing depends on a
real-client answer that the previous stage has not obtained.

### T-0 — prerequisite (not part of this work)

Blocker 23 must land first: route `0x000E` onto a multi-participant lifecycle instead of
`BeginSoloMatch`. Tournament reuses that routing. Doing tournament first would mean building
the routing twice.

### T-1 — decide the start gate against a real client

Before any code: drive one real client, create a room with `room_type = 0x04`, and record
whether the master's button becomes Start with one occupant. This is a **half-day of client
driving that removes the largest unknown in this document** (§2.3/§2.4). Everything else in
the plan is written to be indifferent to the answer, but the answer decides whether the
feature is reachable at all with one client.

Deliverable: an entry under `docs/evidence/`, in the shape of the existing
`REAL_CLIENT_STARTUP_2026-08-07.md`.

### T-2 — protocol: multi-hole plan and the tournament frames

Crate: `pangya-protocol`, module `us852_match.rs` (extend) plus a new
`us852_tournament.rs` for frames with no versus counterpart.

Extend in `us852_match.rs`:

- `RetailMatchStart.room_ui_type` and `RetailMatchInfo.room_ui_type` are already fields
  (`us852_match.rs:54`, `:84`); nothing to add, only callers to fix.
- `RetailMatchInfo` already carries `Vec<RetailHole>` and pads to 18 collectible counts
  (`us852_match.rs:100-134`). It needs no change for multi-hole. Its existing test
  `match_info_always_carries_eighteen_collectible_counts` (`:442-469`) already pins that.

New in `us852_tournament.rs` (all `EncodePacket`):

| Type | Opcode | Body |
|---|---|---|
| `RetailTournamentUserUpdate` | `0x006D` | conn id `u32`, hole `u8`, strokes `u8`, score `i32`, pang, bonus pang, finished `u8` |
| `RetailTournamentUserFinish` | `0x006C` | conn id `u32`, state `u8` (2 finished, 3 quit) |
| `RetailTournamentShotGhost` | `0x006E` | conn id `u32`, hole `u8`, `x f32`, `z f32`, shot flags `u32`, frames `u16` |
| `RetailTournamentShotSummary` | `0x01F7` | conn id `u32`, hole `u32`, relayed client blob (opaque, like `RetailShotCommitRelay`) |
| `RetailTournamentPlacard` | `0x0079` | exp `u32`, trophy id `u32`, trophy `u8`, team `u8`, 12 × 8-byte medal records, medal totals |
| `RetailTournamentWinnings` | `0x00CE` | `u8` status, `u16` count, `count` × item id `u32` |
| `RetailTournamentTimeout` | `0x008C` | empty |
| `RetailTournamentPlayerFinishedLastHole` | `0x0199` | empty |

Per SPEC §9.4 every unclassified byte is an explicit `UnknownBytes<N>` or a documented zero;
the `0x006D` `u32`-vs-`u64` conflict (§3.4) is settled by fixture, and until it is, the
encoder writes the SuperSS/alter-pangya `u64` form with the disagreement recorded in the
doc comment.

The `0x006D` and `0x006E` types need golden fixtures with the metadata SPEC §19.2 requires,
sourced from the PacketDoc `.ksy` definitions with their TH-capture provenance recorded.

### T-3 — domain: multi-hole, N-participant match types

Crate: `pangya-domain`.

- `TournamentCourseConfig` — replaces `OneHoleConfig` for this mode: course id plus an
  ordered `Vec<TournamentHole { number, par, pin }>` of 1..=18 entries, with par validated by
  the existing `OneHoleConfig::par_in_range` (`crates/pangya-domain/src/lib.rs:1160-1167`) so
  the bound has one definition.
- `TournamentParticipant` — account id, roster order (`0..=29`), per-player result key;
  modelled on `StrokeParticipant` (`lib.rs:1750-1790`), with roster validation generalised
  from `validate_stroke_roster` (`lib.rs:1841-1852`) to a slice.
- `BeginTournamentMatch`, `MarkTournamentInGame`, `AbortTournamentMatch`,
  `CommitTournamentMatch`, `TournamentMatchResult`, `TournamentPlayerCommit` — direct
  analogues of the stroke family, with `Vec` where the stroke family has `[_; 2]` and a
  per-hole stroke vector where it has a scalar.
- `MatchRepository` gains `begin_tournament`, `mark_tournament_in_game`, `abort_tournament`,
  `commit_tournament_match`, each with the same default-`Unsupported` body the stroke methods
  already use (`crates/pangya-domain/src/lib.rs:2571-2604`).

### T-4 — storage: migration `0009_m9_tournament_matches.sql`

Against the existing schema (`crates/pangya-storage/migrations/0003…`, `0006…`):

- `matches.mode` check widens to include `'tournament'`
  (currently `('solo_practice', 'stroke_two')`, `0006_m6_stroke_records.sql:4-6`).
- `matches.reward_formula` check gains `'tourney-v1'` (`0006…:7-11`).
- `matches.hole` currently checks `hole = 1` (`0003…:22`). A tournament match spans holes, so
  the per-hole facts move to a new table rather than widening that column:
  **`match_holes`** — `(match_id, hole_ordinal, hole_number, par, pin, weather, wind_speed_tenths, wind_angle_degrees)`,
  primary key `(match_id, hole_ordinal)`.
- `match_players.participant_order` currently checks `IN (0, 1)` and `place` checks `IN (1, 2)`
  (`0006…:37-38`); both widen to `0..=29` and `1..=30`.
- **`match_player_holes`** — `(match_id, participant_order, hole_ordinal, strokes, completion)`
  for per-hole settlement and idempotent `0x0031` handling.
- `currency_ledger.reason` and `progression_ledger.reason` checks gain `'tourney-v1'`
  (`0006…:72-74`).

Every widening is additive; no existing row changes meaning, which keeps SPEC §19.5's
"migrate from every released snapshot" test cheap.

### T-5 — game: the `TournamentMatchState` aggregate

Crate: `pangya-game`, new module `tournament_state.rs`, sibling to `stroke_state.rs`.

Public surface, mirroring the two existing aggregates so the room actor's shape is unchanged:

```
TournamentStartPlan::new(begin, roster: Vec<PlayerConnectionId>, loading_timeout, game_timeout, max_strokes)
TournamentMatchState::{ new, phase, is_active, prepare_start, confirm_begin, cancel_begin,
                        loading_complete, confirm_in_game, accept_shot, hole_out,
                        submit_hole_statistics, give_up, disconnect, deadline_expired,
                        prepare_settlement, apply_commit, abort, prioritize_abort,
                        pending_abort, acknowledge_abort }
```

Differences from `StrokeMatchState` that the module must encode, each of which is a test:

- no `active` participant and no `InvalidTurn` rejection — a shot from any non-terminal
  participant is valid;
- `StrokeDeadline::Turn` has no analogue; only `Loading` and `Game` deadlines exist
  (contrast `stroke_state.rs:172-187`);
- `hole_out` advances *that participant's* hole cursor and returns
  `Advanced { next_hole }` / `PlayerFinished` / `Settlement`, rather than the current
  two-player `Waiting | Duplicate | Settlement` (`stroke_state.rs:211-220`);
- stroke cap is per hole, not per match;
- a participant reaching the last hole is terminal on its own, and the match settles when
  every participant is terminal **or** the game deadline expires.

### T-6 — game: wire the retail opcodes

Crate: `pangya-game`, `lib.rs`.

- `handle_retail_tournament_command`, sibling to `handle_retail_stroke_command`
  (`crates/pangya-game/src/lib.rs:4513`), selected on the joined room's `room_type` rather
  than on member count. Selection by room type is the point: it is what makes the three
  lifecycles disjoint instead of overlapping on "how many people are in the room".
- `send_retail_hole_intro` (`lib.rs:3928-3980`) currently hard-codes
  `room_ui_type: 0, hole_mode: 0, hole_count: 1` and a one-hole plan. Tournament needs those
  fed from the room's real settings and the plan's real holes; the weather/wind sends move
  from once-per-match to once per `0x001A`, **unicast**.
- `RETAIL_C2S_HOLE_LOAD_FINISHED` (`0x0011`) becomes a per-hole event, not a once-per-match
  one.
- Add `RETAIL_C2S_HOLE_INFO = 0x001a` to the retail match opcode set
  (`is_retail_match_opcode`, `lib.rs:5968-5978`) — it is not currently accepted, and it is
  the packet that drives every hole after the first.
- `0x0055` must **not** be sent, and `0x0056`/`0x0058`/`0x0059` must be echoed to the
  sender only.
- `0x00AA` (Tiki Report) is added to the *known-and-refused* set, not to the accepted-session
  allowlist, so a client that sends it produces a classified rejection rather than a silent
  drop.
- New `TournamentRuntimeConfig`, alongside `SoloRuntimeConfig` (`lib.rs:536-551`) and
  `StrokeRuntimeConfig` (`lib.rs:555-574`): course + per-hole par table, catalog fingerprint,
  loading timeout, game timeout, commit timeout, `max_strokes_per_hole`, startup recovery
  limit, shot packet budget, `min_participants` (default 1, see T-1), `max_participants`
  (≤ 30).

### T-7 — test matrix (SPEC §19, §27)

| Layer | Cases |
|---|---|
| Unit — protocol | Each new frame encodes to exact bytes; `0x006D` field widths; `0x00CE` count/limit rejection; 18-slot collectible padding preserved for a 9- and 18-hole plan; profile guard rejects non-US-852 |
| Unit — domain | Roster of 1, 2, 30 accepted; 0 and 31 rejected; duplicate account or duplicate result key rejected; hole plan of 3/6/9/18 accepted, 0 and 19 rejected; par bounds reuse `par_in_range` |
| Unit — aggregate | Table-driven per SPEC §19.4: every rejecting transition leaves state byte-identical (the pattern at `stroke_state.rs` tests and `match_state.rs:656-877`); duplicate shot is `Duplicate`, altered replay is `ConflictingReplay`; hole cursor advances only on hole-out; stroke cap terminates a hole not the match; last-hole hole-out is terminal for that participant only; settlement fires exactly when the last participant is terminal; game deadline settles unfinished participants as timeout forfeits; abort from **every** phase yields no reward; abort wins a concurrent settlement under `prioritize_abort` |
| Golden | Fixtures with SPEC §19.2 metadata for `0x0052` with a 9-hole and an 18-hole plan, `0x006D`, `0x006E`, `0x006C`, `0x00CE`, `0x0079`; provenance recorded as PacketDoc (ISC, TH capture) or reference-derived |
| Property | Any interleaving of N participants' shot/hole-out sequences reaches the same settlement as the sequential order; total strokes recorded equals total shots accepted; a fixed seed yields an identical hole plan and per-hole conditions |
| Fuzz | `0x001A`, `0x001B`, `0x012F` decoders added to the existing bounded targets |
| Integration | Real PostgreSQL: migration from empty and from the `0008` snapshot; begin/mark-in-game/commit idempotency under replay; concurrent commit of the same `result_key` settles once; `abort_incomplete_matches` recovers a tournament row at startup |
| E2E synthetic | One participant, 3 holes, played to settlement; three participants finishing in different orders; one participant disconnecting mid-round; game timer expiring with two participants unfinished |
| Differential / real client | §19.6 checklist items 7-12 against a tournament room: room created as `room_type = 0x04`, match starts, one hole loads, a shot is taken, a hole completes, the round settles, the results screen shows coherent values, and a replayed finish does not double-credit |

SPEC §27 additionally requires that "residual unknowns are recorded rather than hidden behind
magic constants" — the `0x006D` width conflict, the `0x0079` layout, and the `0x008C`
existence are exactly such unknowns and must ship as documented constants with citations, not
as bare numbers.

---

## 8. Unknowns that only a real client can resolve

Ordered by how much they block.

1. **Does the Start button appear in a `room_type = 0x04` room with one occupant?** §2. If
   not, does it appear with two? This decides whether the feature is reachable single-client.
2. **Does the client accept the short `0x0076` (`u8`, `u32`, 16-byte time) or does U.S. 852
   want pangbox's per-player roster form?** §3.1. This project ships the short form untested.
3. **`0x006D` field widths** — PacketDoc's `u32 + 4 unknown` vs SuperSS/alter-pangya's `u64`.
   §3.4.
4. **Does `0x0079` exist on U.S. 852, and is its body the SuperSS/alter-pangya layout?** It
   is absent from PacketDoc entirely. §3.5. If it does not exist, what draws the tournament
   results screen?
5. **Does `0x008C` exist on U.S. 852?** Same absence. §3.5.
6. **Is `0x012F` the U.S. 852 shot-end-data opcode?** SuperSS's own comment says `0x12F`
   replaced something. §3.3. If not, which packet feeds `0x01F7`?
7. **Is item use `0x005A` broadcast or unicast in a tournament?** SuperSS and Py_Source_US
   disagree. §3.3.
8. **Does the client tolerate `0x0065` in a tournament, or does it desynchronise the
   scoreboard?** This project currently sends it on the solo path. §3.4.
9. **Does the client re-send `0x0011` per hole, or only `0x001A`?** SuperSS answers `0x0011`
   with `0x0053` per hole; pangbox's versus loop also re-enters through `0x0011`. The exact
   per-hole client sequence has not been captured.
10. **What does a tournament room's `unknown_special_room_value` actually contain?**
    `00 00 xx 2C` — is `xx` the trophy class, the hole count, or something else? §1.3.
11. **What opcode carries a start refusal?** SuperSS uses `0x0253`, which PacketDoc assigns
    to a different meaning. §2.5.
12. **Does the client require a non-empty second roster slot to enable Start?** i.e. is
    SuperSS's visual bot cosmetic or load-bearing? §2.4.

---

## Open questions

- **Scope of the first tournament release.** One participant and three holes is enough to
  prove the whole lifecycle and needs one client; 10-30 participants needs infrastructure this
  project does not have. Recommendation: ship one-to-four participants, cap
  `max_participants` in config, and leave the 10/20/30 capacities the client offers as a
  room-record value the server does not have to fill.
- **Does tournament land before or after the two-player versus lifecycle (blocker 23)?** T-0
  assumes after, because tournament reuses that routing. If T-1 shows a tournament room starts
  with one occupant while a versus room still needs two, tournament becomes the *cheaper* path
  to a played multi-hole round, and the ordering is worth revisiting.
- **Should `room_ui_type` and `room_type` become distinct fields now?** §1.2. They are equal
  for every mode this project will implement in the near term, so splitting them is
  speculative — but leaving them fused is a latent bug the moment a practice room type is
  added.
- **Which reward formula name?** `tourney-v1` is assumed throughout §4 and §T-4. It must be
  fixed before the migration lands, because the check constraint pins it.
- **Par.** A multi-hole tournament needs par for every hole in the plan, and this project has
  no per-hole par source: [`../PROGRESS.md`](../PROGRESS.md) records that `Course.iff` carries
  none and per-hole par lives in the course's own PAK data. Operator-declared par tables per
  course are the assumed answer; if that is wrong, T-3 and T-4 both change shape.
