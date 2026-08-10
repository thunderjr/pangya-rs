# The retail contract

> Last updated: **2026-08-08**

This document states what the acquired U.S. 852 client establishes, what of the retail
protocol this server implements today, and which parts of the generated `0x7f**` synthetic
protocol are now redundant. It is the reference for removing the synthetic families without
losing coverage.

Two claim levels are used throughout and never mixed:

- **client-verified** — a real client was driven through it and behaved correctly. The
  evidence is named in [`PROGRESS.md`](PROGRESS.md) blockers 13-23.
- **reference-derived** — the layout comes from the vendored `pangbox--packetdoc` Kaitai
  definitions and/or `pangbox/server` (ISC), is unit-tested and proven over encrypted TCP in
  CI, and has not been put in front of a client. Provenance is recorded at each packet's
  definition.

Nothing here is *capture*-derived. No packet capture is committed, and none was used.

---

## 1. What the real artifacts establish

### 1.1 The client

| Fact | Value | Evidence |
|---|---|---|
| Distribution archive | `PangYa_Client_US_851.zip`, 2,362,368,969 bytes | `evidence/US_CLIENT_ACQUISITION_2026-08-07.md` |
| Content patch level | 851 | client crash log; `client_web.patch_number = 851` |
| Protocol/build version on the wire | `852.00` | the client raises `SERVER_VERSION_MISSMATCH` against anything else; `US852_SERVER_VERSION` in `crates/pangya-protocol/src/us852_bootstrap.rs:16` |
| Packet version reported | `2016110200` | client crash log |
| Executable protection | WinLicense; a debugger is terminated on attach | `evidence/REAL_CLIENT_STARTUP_2026-08-07.md` |
| Rugburn identification | US 852 | `docs/RUNNING_THE_CLIENT.md` §5 |

"851" and "852.00" are both correct and name different things: 851 is the content patch
level, 852.00 is the protocol version the wire carries. `CompatibilityProfile::US_852`
(`crates/pangya-protocol/src/profile.rs:80`) is the only profile the codebase implements, and
every retail packet family gates on `require_us852` before touching a byte.

The client is not committed and never may be. It lives under the gitignored `local-data/`
tree; `scripts/check-proprietary-assets.sh` fails the build if any of it, or any `.iff`,
`.pak`, `.exe`, `.dll`, or capture file, is staged anywhere outside the approved fixture
directories.

### 1.2 The startup HTTP contract

The client makes 33 HTTP requests and mounts its 84-archive PAK series **before it opens any
socket**. `[client_web]` serves all of them: a base64 string catalog, an XTEA-encrypted patch
`updatelist`, and theme documents plus their images. This is client-verified
(`evidence/REAL_CLIENT_STARTUP_2026-08-07.md`, ADR-0015).

Three properties are load-bearing and each was found by satisfying the previous one:

| Property | Consequence if wrong |
|---|---|
| `patch_number` must not exceed the client's own patch level | the client renders its scene and never offers a login dialog |
| Every resource must be a loose file in the client directory (41,192 files, no base-name collisions) | `WAppException("Cannot open file.")`, then an access violation on the first cursor image |
| `HKLM\SOFTWARE\WOW6432Node\Ntreev USA\Pangya\IntegratedPak` must exist | "Plesae re-install the game…" and exit, before the `updatelist` is fetched |

`[client_web]` is a separate listener from `[http]` precisely so the client-reachable surface
carries no health, readiness, or metrics endpoint. That separation is verified by 404s from
the client's own machine.

The generated `updatelist` is byte-identical to an independent encoder's output for the real
client directory, pinned in CI by a golden fixture. The cipher and the nonstandard file
CRC-32 are reproduced exactly rather than normalised; both are documented at their
definitions in `crates/pangya-updater` and in `PROVENANCE.md`.

### 1.3 The PAK chain and the catalog

The client does **not** read its item tables from a loose file. It resolves `pangya_gb.iff`
through its PAK chain, where a later PAK supersedes a same-named entry in an earlier one.
This install carries `projectg700gb+.pak` through `projectg851gb.pak`. A catalog built from a
superseded copy loads and validates cleanly and is still wrong — it silently lacks every item
added since. The tell is a purchase refused with `stage: "not_in_catalog"` for an id that
sits inside a family's range (PROGRESS blocker 19).

`pangya_gb.iff` is itself a ZIP holding 39 per-family tables. Six of them feed the catalog.
Loading is `manifest_version = 3` (`CLIENT_MANIFEST_VERSION`,
`crates/pangya-data/src/lib.rs:39`); the parser differs from the synthetic schemas in three
measured ways — the type ID lives at record offset four, the header's `binding` carries no
family meaning, and record width is whatever the header arithmetic yields.

| Table | `kind` | Family tag(s) | What it yields |
|---|---|---|---|
| `Character.iff` | `character` | `0x04` | identity only; **no** economy definition (`client_definition`, `crates/pangya-data/src/lib.rs:611`) |
| `ClubSet.iff` | `club_set` | `0x10` | identity + Pang price, unique stacking |
| `Ball.iff` | `ball` | `0x14` | identity + Pang price, unique stacking |
| `Item.iff` | `consumable` | `0x18`, `0x1a`, `0x1b` | identity + Pang price, stackable to `CLIENT_CONSUMABLE_MAX_STACK` |
| `Part.iff` | `character_part` | `0x08` | identity + Pang price, unique stacking |
| `Course.iff` | `course` | `0x28` | identity only; **carries no par** |

Record counts and the resulting offer count are recorded in PROGRESS blocker 19; the winning
copy yields **3,109** priced offers, against 2,664 from the superseded one.

Three things are stated server policy rather than data read from the table, and are marked as
such in the code:

- nothing is durable (`ItemDurability::Nondurable` for every record);
- every part is compatible with any character (`ItemCompatibility::Any`);
- the per-item stack limit (`CLIENT_CONSUMABLE_MAX_STACK = 100`).

Course par is likewise **operator-declared**. The client's `Course.iff` record is a
presentation row; per-hole par lives in the course's own PAK data. A par declared for a course
the catalog does not have is rejected at startup, and leaving it zero with a real-client
catalog fails startup with a message naming the problem rather than guessing.

An operator override, `data.price_override_pang`, reprices every item the client already
sells. It warns loudly at startup and deliberately cannot make an unavailable item
purchasable. It changes what the server *charges*, never what the client *displays* — the
shop's names, prices, and listing are rendered from the client's own tables.

### 1.4 Regenerating the artifacts

Operator-side only; nothing produced here is committed.

```bash
# 1. Extract the winning copy of the item tables from the PAK chain.
scripts/extract-client-iff.py --client-dir local-data/us851 --region us --list
scripts/extract-client-iff.py --client-dir local-data/us851 --out local-data/us851-data/pak-iff

# 2. Unzip pangya_gb.iff — standard unzip skips its entries; use a permissive reader.
python3 -c "
import zipfile, os
z = zipfile.ZipFile('local-data/us851-data/pak-iff/pangya_gb.iff')
os.makedirs('local-data/us851-data/pak-iff/iff', exist_ok=True)
for n in z.namelist():
    open('local-data/us851-data/pak-iff/iff/' + n, 'wb').write(z.read(n))
"

# 3. Compute each table's header values and digest for the manifest.
python3 -c "
import struct, hashlib
for n in ['Character','ClubSet','Ball','Item','Part','Course']:
    d = open(f'local-data/us851-data/pak-iff/iff/{n}.iff','rb').read()
    c,b,v = struct.unpack_from('<HHI', d, 0)
    print(n, 'count=%d binding=%d version=%d record_size=%d' % (c,b,v,(len(d)-8)//c),
          hashlib.sha256(d).hexdigest())
"
```

Then write `manifest.toml` beside the tables with `manifest_version = 3` and one `[[files]]`
block per table, and point `data.iff_directory` at that directory. The full procedure,
including the theme and translation-catalog inputs `[client_web]` needs, is in
[`RUNNING_THE_CLIENT.md`](RUNNING_THE_CLIENT.md).

---

## 2. The retail packet surface implemented today

Counts: **38** distinct client→server opcodes accepted (28 with behaviour, 10 accepted
silently), **45** distinct server→client opcodes emitted, plus two unframed hellos.

### 2.1 LoginService — client to server

The LoginService protocol is retail throughout; there is no synthetic LoginService. All five
are client-verified end to end (PROGRESS blockers 14, 15).

| Opcode | Packet | Handler | Provenance |
|---|---|---|---|
| `0x0001` | `LoginRequest` | `crates/pangya-login/src/runtime.rs:742` | `pangbox/packetdoc` `loginservice/client/0001.ksy` |
| `0x0003` | `SelectServer` | `crates/pangya-login/src/runtime.rs:950` | PacketDoc `loginservice/client/0003.ksy` |
| `0x0006` | `SetNickname` | `crates/pangya-login/src/runtime.rs:817` | PacketDoc `loginservice/client/0006.ksy` |
| `0x0007` | `CheckNickname` | `crates/pangya-login/src/runtime.rs:795` | PacketDoc `loginservice/client/0007.ksy` |
| `0x0008` | `SelectCharacter` | `crates/pangya-login/src/runtime.rs:875` | PacketDoc `loginservice/client/0008.ksy` |

Anything else in `0x0001 | 0x0003 | 0x0006 | 0x0007 | 0x0008` used in the wrong state closes
the connection (`is_known_opcode`, `crates/pangya-login/src/runtime.rs:1273`).

### 2.2 LoginService — server to client

| Opcode | Packet | Definition | Notes and provenance |
|---|---|---|---|
| hello | `us852_login_hello` | `crates/pangya-protocol/src/login.rs` | `pangbox/server` `login/conn.go`; fixture `us852-login-hello` |
| `0x0001` | `LoginResult::Success` | `crates/pangya-protocol/src/login.rs:199` | also resent with `success` after character selection, which is what unblocks the client's "Waiting for server's response." |
| `0x0001` | `LoginResult::NeedSetNickname` | `login.rs:215` | status `0xd8`. PacketDoc's TH captures use `0xd9`; U.S. 852 is `0xd8` (`pangbox/server` `login/msgserver.go`, confirmed by the client) |
| `0x0001` | `LoginResult::NeedSelectCharacter` | `login.rs:219` | status `0xd9`, same correction |
| `0x0001` | `LoginResult::Error` | `login.rs:220` | status `0xe3` plus an opaque code |
| `0x0002` | `GameServerList` | `login.rs:318` | each entry is **93** bytes for U.S. 852, not PacketDoc's 92: every entry ends with a channel-count byte. Upstream sends the count with **zero** channels. Without it the client reports "Server is full" |
| `0x0003` | `SessionKey` | `login.rs:386` | |
| `0x0006` | `ChatMacros` | `login.rs:404` | nine fixed 64-byte macros |
| `0x0009` | `EmptyMessageServerList` | `login.rs:421` | |
| `0x000e` | `NicknameCheckResult` | `login.rs:240` | |
| `0x0010` | `LoginKey` | `login.rs:452` | the session key is sent **twice**; it is this copy the client stores and later echoes in the GameService auth. Sending only `0x0003` is what produced the empty `login_key` in blocker 15 |

### 2.3 GameService — client to server, with behaviour

All are gated on `game.retail_bootstrap = true` unless noted.

| Opcode | Packet / constant | Handler | Status | Provenance |
|---|---|---|---|---|
| `0x0002` | `RetailGameAuth` | `crates/pangya-game/src/lib.rs:1250`, decoded at `:2087` | client-verified | `us852_bootstrap.rs:98`; PacketDoc `gameservice/client/0002.ksy` |
| `0x0004` | `RetailSelectChannel` | `lib.rs:1271` | client-verified | `game.rs:147`; PacketDoc `gameservice/client/0004.ksy`, a **one-byte** sub-server ID |
| `0x0008` | `RetailRoomCreate` | `lib.rs:5123` | client-verified | `us852_room.rs:101` |
| `0x0009` | `RetailRoomJoin` | `lib.rs:5172` | client-verified | `us852_room.rs:141` |
| `0x000a` | `RetailRoomSettingsUpdate` | applies validated identity, course/card shape, progression, timers, capacity, artifact id, and natural-wind edits atomically; unsupported repeat selectors (types 11/12) are refused without disconnecting | reference-derived | `us852_room.rs`; `pangbox--packetdoc` `gameservice/client/000a.ksy`; `pangbox/server` `RoomSettingsChange`/`RoomListRoom`; the checked corpus has no authoritative type-11/12 semantics |
| `0x000d` | `RETAIL_C2S_ROOM_READY` | `lib.rs:5218` | client-verified (blocker 21) | one byte, zero meaning ready; the reply is the census, not an acknowledgement |
| `0x000e` | `RETAIL_C2S_START_MATCH` | `lib.rs:4527` (stroke), `lib.rs:4384` (solo) | client-verified for start; the versus hole is CI-proven only | the room is read first: two members run the stroke aggregate, one runs solo |
| `0x000f` | `RETAIL_C2S_ROOM_LEAVE` | `lib.rs:5234` | client-verified | |
| `0x0011` | `RETAIL_C2S_HOLE_LOAD_FINISHED` | `lib.rs:4558` / `:4424` | reference-derived | |
| `0x0012` | `RETAIL_C2S_SHOT_COMMIT` | `lib.rs:4591` / `:4451` | reference-derived | payload relayed unchanged; nothing in it is decoded |
| `0x001b` | `RETAIL_C2S_SHOT_SYNC` | `lib.rs:4618` / `:4447` | reference-derived | relayed in a versus hole, accepted without reply in solo |
| `0x001c` | `RETAIL_C2S_SHOT_END` | `lib.rs:4629` / `:4451` | reference-derived | both clients send this barrier; the aggregate decides who owns the turn |
| `0x001d` | `RetailPurchaseRequest` | `lib.rs:1365` → `handle_retail_purchase` `:4934` | client-verified (blocker 20) | `us852_room.rs:281`; `pangbox/server` `game/packet/client.go` `ClientBuyItem` |
| `0x0020` | `RetailEquipmentUpdate` | `lib.rs:1349` → `handle_retail_equipment_update` `:4872` | client-verified (blocker 17) | `us852_room.rs:454`; tagged union over eight equipment kinds |
| `0x0031` | `RETAIL_C2S_HOLE_FINISH` | `lib.rs:4653` / `:4479` | reference-derived | a completion, not a shot — counting it as a stroke scored every hole one over |
| `0x0034` | `RETAIL_C2S_FIRST_SHOT_READY` | `handle_retail_stroke_command`, answered with `0x0090` | reference-derived | `pangbox/server` `game/server/conn.go` `ClientFirstShotReady`; the client waits for the reply before the first shot |
| 13 cosmetic in-match opcodes | `RETAIL_ACCEPTED_MATCH_OPCODES` (`game.rs`) | accepted without a reply | reference-derived | aim, meter, power, club, item, relief, hole info, active-player ack, pause, arrow, load progress, game end, last-player-leave. Upstream relays each to the other participants; this server does not yet, so an opponent's aim does not animate |
| `0x0081` | `RETAIL_C2S_MULTIPLAYER_JOIN` | `lib.rs:5100` | client-verified | opens the room directory; answered with the list then the acknowledgement |
| `0x0082` | `RETAIL_C2S_MULTIPLAYER_LEAVE` | `lib.rs:5116` | client-verified | |
| `0x009c` | `RetailPlayerHistoryRequest` | `lib.rs:1321` | client-verified (blocker 16) | `game.rs:307`; `pangbox/server` `game/packet/client.go` `ClientRequestPlayerHistory` |
| `0x00b5` | `RetailMyRoomEnter` | `lib.rs` retail lobby service | reference-derived | `us852_room.rs`; target account is loaded from PostgreSQL, so own and visitor rooms use the target projection |
| `0x00b7` | `RetailMyRoomInventoryRequest` | `lib.rs` retail lobby service | reference-derived | `us852_room.rs`; serves persisted `0x012d` furniture and the target's complete 513-byte equipped-character block |
| `0x0073` | `RetailMascotMessageUpdate` | `lib.rs` retail lobby service | reference-derived | authenticated owner update persists the mascot message; success is `0x00e2` status 4, with no unsolicited visitor-room result |
| `0x00b9` / `0x00c9` | UCC requests | `lib.rs` retail lobby service | explicit refusal | no upload service is configured; both return `0x0153` status 1 with `0x05100100` |
| `0x00cc` | `RetailLockerCombinationAttempt` | `lib.rs:1380` → `:4814` | reference-derived | `us852_room.rs:796`; `ClientLockerCombinationAttempt` |
| `0x00d3` | `RetailLockerInventoryRequest` | `lib.rs:1380` → `:4811` | reference-derived | `us852_room.rs:746`; `ClientLockerInventoryRequest` |
| `0x0140` | `RetailShopJoin` | `lib.rs:1380` → `:4810` | client-verified | `us852_room.rs:568`; `ClientShopJoin` |
| `0x016e` | `RetailLoginBonusRequest` | `lib.rs:1321` | client-verified (blocker 16) | `game.rs:193`; PacketDoc `gameservice/client/016e.ksy` |

### 2.4 GameService — client to server, accepted with no reply

`RETAIL_ACCEPTED_SESSION_OPCODES`, `crates/pangya-protocol/src/game.rs:273`. Ten opcodes
upstream accepts and answers with an empty case; the client neither expects nor waits for a
reply. Enumerated from `pangbox/server` `game/packet/client.go` rather than discovered one
client restart at a time.

`0x0007` online status · `0x0018` typing indicator · `0x0032` idle status · `0x0033`
client-side exception report · `0x004f` unclassified · `0x0069` chat macro set · `0x0088`
unclassified · `0x008b` messenger list request · `0x00c1` unclassified · `0x00fe`
unclassified

Room and match opcodes are deliberately excluded, and a unit test
(`game.rs:568`) pins that exclusion: those have real state handlers, and silently accepting
one would hide a gap instead of surfacing it. This allowlist is what makes the shipped
`unknown_opcode_policy = "disconnect"` survivable in the lobby.

### 2.5 GameService — server to client

| Opcode | Packet | Definition | Emitted at | Provenance / notes |
|---|---|---|---|---|
| hello | `us852_game_hello` | `game.rs:66` | `lib.rs:1124` | nine bytes `00 06 00 00 3f 00 01 01 <key>`, unframed. `pangbox/server` `game/server/auth.go`, `game/packet/server.go` `ConnectMessage` |
| `0x0044` | `HandoverControl::Progress` | `us852_bootstrap.rs:183` | `lib.rs:5319` | subtype `0xd2`; three ticks release the client's loading bar |
| `0x0044` | `HandoverReply` | `us852_bootstrap.rs:934` | `lib.rs:5382` | subtype `0x00`; must carry PString `"852.00"` |
| `0x0070` | `IffContainerChunk::CharacterRoster` | `us852_bootstrap.rs:319` | `lib.rs:5391` | chunked, 50 entries |
| `0x0071` | `IffContainerChunk::CaddieRoster` | `us852_bootstrap.rs:321` | `lib.rs:5394` | sent empty; the client still expects the container |
| `0x0072` | `RetailEquipment` | `us852_bootstrap.rs:249` | `lib.rs:5396` | |
| `0x0073` | `IffContainerChunk::Inventory` | `us852_bootstrap.rs:323` | `lib.rs:5406` | |
| `0x004d` | `ServerChannelList` | `us852_bootstrap.rs:287` | `lib.rs:5409` | |
| `0x0095` | `RetailPangBalance` | `us852_bootstrap.rs:740` | `lib.rs:5426` | the lobby header reads its balances from here, not from the statistics block |
| `0x0096` | `RetailPointBalance` | `us852_bootstrap.rs:768` | `lib.rs:5433`, `:5026` | |
| `0x004e` | `RetailChannelJoined` | `game.rs:170` | `lib.rs:1306` | a single `0x01`; PacketDoc `gameservice/server/004e.ksy` |
| `0x01f6` | `RetailChannelJoinNotice` | `game.rs:249` | `lib.rs:1307` | four zero bytes, carried verbatim. `pangbox/server` `game/server/conn.go` |
| `0x0248` | `RetailLoginBonusStatus` | `game.rs:219` | `lib.rs:1339` | "already collected" form; reporting the uncollected form would advertise a reward nothing can grant |
| `0x010e` | `RetailPlayerHistory` | `game.rs:333` | `lib.rs:1341` | five zeroed 52-byte slots — an empty history, not invented opponents |
| `0x020e` | `RetailShopJoined` | `us852_room.rs:589` | `lib.rs:4810` | `Server020E` |
| `0x012b` | `RetailMyRoomEntered` | `us852_room.rs:643` | `lib.rs:4826` | |
| `0x012d` | `RetailMyRoomLayout` | `us852_room.rs` | `lib.rs` retail lobby service | PacketDoc exact option/count/27-byte Furniture.iff entries, loaded from PostgreSQL |
| `0x00e2` | `RetailMascotMessageResult` | `us852_room.rs` | `lib.rs` retail lobby service | result of the established `0x0073` update flow; not emitted as a visitor-room projection |
| `0x0153` | `RetailUccUploadKeyRefusal` | `us852_room.rs` | `lib.rs` retail lobby service | explicit unsupported-upload response; no URL or key fabricated |
| `0x0168` | `RetailPlayerInfo` | `us852_room.rs:724` | `lib.rs:4845` | carries the same 341-byte record as the room census |
| `0x0170` | `RetailLockerInventoryResponse` | `us852_room.rs:771` | `lib.rs:4812` | |
| `0x016c` | `RetailLockerCombinationResponse` | `us852_room.rs:817` | `lib.rs:4815` | |
| `0x006b` | `RetailEquipmentUpdated` | `us852_room.rs:523` | `lib.rs:4925` | reports the equipment this server **holds**, not an acknowledgement of the requested change |
| `0x00c8` | `RetailPangSpent` | `us852_room.rs:328` | `lib.rs:5018` | `ServerPangBalanceData` |
| `0x0068` | `RetailPurchaseResponse` | `us852_room.rs:365` | `lib.rs:5028`, `:5046` | |
| `0x00f5` | `RetailMultiplayerJoined` | `us852_room.rs:841` | `lib.rs:5113` | empty body; `ServerMultiplayerJoined`, PacketDoc `00f5.ksy` |
| `0x00f6` | `RetailMultiplayerLeft` | `us852_room.rs:862` | `lib.rs:5120` | empty body |
| `0x0047` | `RetailRoomList` | `us852_room.rs:883` | `lib.rs:5105`, `:5297` | 210-byte room records |
| `0x0048` | `RetailRoomCensus::List` | `us852_room.rs:2115` | `lib.rs:4520`, `:7908`, and every room snapshot while in the room | 854-byte player records (341-byte identity plus 513-byte character block). **Only the `List` form is emitted**; `Add`/`Remove`/`Update` are modelled but never sent. A snapshot re-sends the whole roster rather than a delta |
| `0x0049` | `RetailRoomJoinResult` | `us852_room.rs:914` | `lib.rs:6720`, `:6950` | success writes a `u16` status then the room record; rejection writes one byte. The widths genuinely differ |
| `0x004c` | `RetailRoomLeave` | `us852_room.rs:1008` | `lib.rs:6830` | `0xffff` means the lobby; also sent to a kicked member |
| `0x0086` | `RetailRoomInformationResponse` | `us852_room.rs:2110` | `lib.rs:6934`, `:6960` | PacketDoc exact 18-byte user records: connection id, rank, five opaque bytes, title badge, four opaque bytes; do not reuse the 341-byte census identity |
| `0x007d` | `RetailTeamChangeAnnounce` | `us852_room.rs:385` | room event fanout | one announce plus one census per team mutation |
| `0x0083` | `RetailRoomInviteNotification` | `us852_room.rs:466` | room event fanout | invitee notification; do not reuse request opcode `0x00ba` |
| `0x012f` / `0x0130` | `RetailRoomInviteResponse` / `RetailRoomInviteInfoResponse` | `us852_room.rs:429` / `:408` | `lib.rs` invite handlers | `0x00ba` / `0x0029` request pairing respectively |
| `0x0076` | `RetailMatchStart` | `us852_match.rs:60` | `lib.rs:3948` | |
| `0x0052` | `RetailMatchInfo` | `us852_match.rs:100` | `lib.rs:3956` | must always write **eighteen** collectible count bytes regardless of hole count |
| `0x009e` | `RetailHoleWeather` | `us852_match.rs:143` | `lib.rs:3973` | |
| `0x005b` | `RetailHoleWind` | `us852_match.rs:167` | `lib.rs:3976` | the trailing `1` sets the wind outright; `0` would accumulate |
| `0x0053` | `RetailPlayerStartHole` | `us852_match.rs:192` | `lib.rs:3460`, `:3523` | |
| `0x0063` | `RetailTurnStart` | `us852_match.rs:213` | `lib.rs:3467`, `:3542`, `:4472` | |
| `0x00cc` | `RetailTurnEnd` | `us852_match.rs:234` | `lib.rs:3532`, `:4465` | |
| `0x0055` | `RetailShotCommitRelay` | `us852_match.rs:285` | `lib.rs:3565` | the client's own shot payload, relayed unchanged |
| `0x0064` | `RetailShotSync` | `us852_match.rs:316` | `lib.rs:3573` | `ServerRoomShotSync`, `game/room/room.go` `handleRoomGameShotSync` |
| `0x0065` | `RetailFinishHole` | `us852_match.rs:846` | `lib.rs:4608`, `:5440` | empty body; nonterminal holes reply to the caller, while the terminal room event sends exactly one to every captured player |
| `0x0090` | `RetailFirstShotReady` | `game.rs` | `handle_retail_stroke_command` | empty body; `ServerPlayerFirstShotReady`. Its arrival is the whole message |
| `0x0066` | `RetailMatchFinish` | `us852_match.rs:802` | `lib.rs:4612` | the durable server-side settlement, never anything the client claimed. Every captured player receives one terminal `0x0065` immediately before this frame. `ServerRoomFinishGame` |

### 2.6 Retail types defined but not routed

| Item | Definition | Why it is not emitted |
|---|---|---|
| `RetailAimRotate` (`0x0056`) | `us852_match.rs:258` | aim rotation is never relayed; no handler decodes `0x0013` |
| `RetailRoomCensus::Add` / `Remove` / `Update` | `us852_room.rs:1110` | lobby broadcasts are dropped in retail mode (`lib.rs:3425`), so a room does not update live |
| `RetailRoomSettingChange::Artifact` | `us852_room.rs:118` | parsed but refused without mutating the room: PacketDoc carries the id, while the checked references provide no authoritative gameplay/reward effect |
| `HandoverRejection` (all eight codes) | `us852_bootstrap.rs:36` | a failed handover closes the connection instead of naming a client-visible reason |
| `RetailRoomType::Chat` / `Tournament` / `Battle` | `us852_room.rs:41` | decoded from the create request, then discarded; every room runs the versus lifecycle |
| `RetailRoomSettingChange::RepeatHole` / `FixedRepeatHole` | `us852_room.rs:112-117` | decoded for compatibility but deterministically refused without a disconnect; the checked references define no semantics for types 11/12, so the room remains unchanged |

### 2.7 Retail coverage in CI

Four end-to-end tests drive the retail wire over encrypted TCP against real PostgreSQL, all
in `crates/pangya-server/tests/game_e2e.rs`:

| Test | Line | Covers |
|---|---|---|
| `game_retail_bootstrap_emits_the_reference_derived_sequence` | 5871 | hello, `0x0002` auth, the `0x0044`×4 / `0x0070` / `0x0071` / `0x0072` / `0x0073` / `0x004d` / `0x0095` / `0x0096` order, and the `"852.00"` version string |
| `game_retail_room_management_over_tcp` | 5968 | create/join/settings, census, team, resync, paired invites, kick/`0x004c`, malformed rejection, and leave |
| `game_retail_match_plays_and_settles_one_hole` | 6114 | one-player Practice compatibility path: start, hole intro, turn frames, hole finish, settlement |
| `game_retail_two_players_play_and_settle_one_versus_hole` | 6273 | two authenticated clients: alternating turns, shot relay, and 18-hole whole-card settlement with per-hole `0x0053` introductions and one Pang/EXP ledger row each |

**Retail paths with no end-to-end test**: `0x001d` purchase, `0x0020` equipment update,
`0x0140`/`0x00b5`/`0x00b7`/`0x00d3`/`0x00cc` lobby services, `0x016e` login bonus, `0x009c`
player history. They are covered by 26 unit tests in `us852_room.rs` and
`us852_bootstrap.rs`, and by the client itself (blockers 16, 17, 20), but nothing in CI
exercises them over the wire. There are also **no retail golden fixtures** — every fixture
under `crates/pangya-protocol/tests/fixtures/` is either a LoginService packet or synthetic.

---

## 3. The synthetic surface

64 items: 52 `SYNTHETIC_*` opcode constants, 8 M3 bootstrap types in
`crates/pangya-protocol/src/game.rs`, and 4 registry builders. Classification below is
**remove now** (the retail path covers it and a retail test proves it), **keep until X** (a
named retail gap must close first), or **keep permanently** (test-only scaffolding).

Nothing retail depends on a synthetic *packet*. Three synthetic *value objects* are a
different matter and are called out in §3.6.

### 3.1 M3 synthetic bootstrap — all remove now

Defined in `crates/pangya-protocol/src/game.rs`. Four of these carry opcode meanings that
both PacketDoc and `pangbox/server` independently confirm are **wrong** for retail
(PROGRESS blocker 10), so they collide with the retail family on the same opcode.

| Item | Opcode | Definition | Retail replacement | Class |
|---|---|---|---|---|
| `synthetic_game_hello` | — | `game.rs:40` | `us852_game_hello` (`game.rs:66`) | remove now |
| `GameAuth` | `0x0002` | `game.rs:96` | `RetailGameAuth` | remove now |
| `SelectChannel` | `0x0004` | `game.rs:121` | `RetailSelectChannel` | remove now |
| `ChannelJoined` | `0x004e` | `game.rs:501` | `RetailChannelJoined` | remove now |
| `PlayerInfo` | `0x0070` **wrong meaning** | `game.rs:362` | `HandoverReply`; `0x0070` is the character roster | remove now |
| `CharacterInfo` | `0x0072` **wrong meaning** | `game.rs:396` | container `0x0070`; `0x0072` is equipment | remove now |
| `EquipmentInfo` | `0x004d` **wrong meaning** | `game.rs:477` | `RetailEquipment` (`0x0072`); `0x004d` is the channel list | remove now |
| `InventorySegment` | `0x0073` | `game.rs:436` | container `0x0073` (correct opcode, wrong body) | remove now |

Covered by `game_retail_bootstrap_emits_the_reference_derived_sequence`.

### 3.2 M4 lobby and rooms — `crates/pangya-protocol/src/m4_room.rs`

| Constant | Opcode | Retail replacement | Class |
|---|---|---|---|
| `SYNTHETIC_M4_C2S_LIST` | `0x7f00` | `0x0081` → `0x0047` | remove now |
| `SYNTHETIC_M4_C2S_CREATE` | `0x7f01` | `0x0008` | remove now |
| `SYNTHETIC_M4_C2S_JOIN` | `0x7f02` | `0x0009` | remove now |
| `SYNTHETIC_M4_C2S_LEAVE` | `0x7f03` | `0x000f` | remove now |
| `SYNTHETIC_M4_C2S_READY` | `0x7f05` | `0x000d` | remove now |
| `SYNTHETIC_M4_S2C_LIST` | `0x7f80` | `0x0047` | remove now |
| `SYNTHETIC_M4_S2C_STATE` | `0x7f81` | `0x0048` `List` | remove now |
| `SYNTHETIC_M4_C2S_SETTINGS` | `0x7f04` | none | keep until retail `0x000a` room settings is implemented |
| `SYNTHETIC_M4_C2S_CHAT` | `0x7f06` | none | keep until retail room chat is implemented |
| `SYNTHETIC_M4_S2C_CHAT` | `0x7f84` | none | keep until retail room chat is implemented |
| `SYNTHETIC_M4_C2S_KICK` | `0x7f07` | none | keep until a retail kick path exists |
| `SYNTHETIC_M4_C2S_STATE` | `0x7f08` | none | keep until the census can be requested, not only pushed |
| `SYNTHETIC_M4_S2C_COMMAND_RESULT` | `0x7f82` | `0x0049` rejection, create/join only | keep until every retail room command can report a typed refusal |
| `SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT` | `0x7f83` | `0x0048` `List`, re-sent on every membership or ready change | remove now |
| `synthetic_m4_registry` | — | — | keep until the family is gone |

7 remove now, 8 keep until.

### 3.3 M5 solo practice — `crates/pangya-protocol/src/m5_solo.rs`

| Constant | Opcode | Retail replacement | Class |
|---|---|---|---|
| `SYNTHETIC_M5_C2S_START_SOLO` | `0x7f20` | `0x000e` with one room member | remove now |
| `SYNTHETIC_M5_C2S_LOADING_COMPLETE` | `0x7f21` | `0x0011` | remove now |
| `SYNTHETIC_M5_C2S_SHOT_ACTION` | `0x7f22` | `0x0012` | remove now |
| `SYNTHETIC_M5_C2S_SHOT_RESULT` | `0x7f23` | `0x001c` | remove now |
| `SYNTHETIC_M5_C2S_FINISH_HOLE` | `0x7f24` | `0x0031` | remove now |
| `SYNTHETIC_M5_S2C_MATCH_STARTED` | `0x7fa0` | `0x0076`/`0x0052`/`0x009e`/`0x005b` | remove now |
| `SYNTHETIC_M5_S2C_MATCH_PHASE` | `0x7fa1` | `0x0053`/`0x0063` | remove now |
| `SYNTHETIC_M5_S2C_SHOT_ACTION_RELAY` | `0x7fa2` | dropped in retail — self-echo in a solo hole | remove now |
| `SYNTHETIC_M5_S2C_SHOT_RESULT_RELAY` | `0x7fa3` | dropped in retail | remove now |
| `SYNTHETIC_M5_S2C_HOLE_RESULT` | `0x7fa4` | `0x0065` | remove now |
| `SYNTHETIC_M5_S2C_BALANCE_UPDATE` | `0x7fa5` | none after settlement | keep until a retail post-match balance push exists |
| `SYNTHETIC_M5_S2C_COMMAND_RESULT` | `0x7fa6` | none | keep until a retail match refusal frame exists |
| `SYNTHETIC_M5_S2C_MATCH_ABORTED` | `0x7fa7` | none (`lib.rs:3585`) | keep until a retail abort frame exists |
| `synthetic_m5_registry` | — | — | keep until the family is gone |

10 remove now, 4 keep until.

### 3.4 M6 two-player stroke — `crates/pangya-protocol/src/m6_stroke.rs`

| Constant | Opcode | Retail replacement | Class |
|---|---|---|---|
| `SYNTHETIC_M6_C2S_START_STROKE_TWO` | `0x7f30` | `0x000e` with two room members | remove now |
| `SYNTHETIC_M6_C2S_LOADING_COMPLETE` | `0x7f31` | `0x0011` | remove now |
| `SYNTHETIC_M6_C2S_SHOT_ACTION` | `0x7f32` | `0x0012` | remove now |
| `SYNTHETIC_M6_C2S_SHOT_RESULT` | `0x7f33` | `0x001c` | remove now |
| `SYNTHETIC_M6_S2C_MATCH_STARTED` | `0x7fb0` | `0x0076`/`0x0052`/`0x009e`/`0x005b` | remove now |
| `SYNTHETIC_M6_S2C_PHASE` | `0x7fb1` | dropped in retail (`lib.rs:3511`) | remove now |
| `SYNTHETIC_M6_S2C_TURN_STARTED` | `0x7fb2` | `0x0053`/`0x0063`/`0x00cc` | remove now |
| `SYNTHETIC_M6_S2C_ACTION_RELAY` | `0x7fb3` | `0x0055` | remove now |
| `SYNTHETIC_M6_S2C_RESULT_RELAY` | `0x7fb4` | `0x0064` | remove now |
| `SYNTHETIC_M6_S2C_STANDINGS` | `0x7fb5` | `0x0066` | remove now |
| `SYNTHETIC_M6_C2S_GIVE_UP` | `0x7f34` | none | keep until retail `0x0130` player quit is routed |
| `SYNTHETIC_M6_S2C_COMMAND_RESULT` | `0x7fb6` | none | keep until a retail match refusal frame exists |
| `SYNTHETIC_M6_S2C_MATCH_ABORTED` | `0x7fb7` | none | keep until a retail abort frame exists |
| `SYNTHETIC_M6_S2C_BALANCE_UPDATE` | `0x7fb8` | none after settlement | keep until a retail post-match balance push exists |
| `synthetic_m6_registry` | — | — | keep until the family is gone |

10 remove now, 5 keep until.

### 3.5 M7 economy — `crates/pangya-protocol/src/m7_economy.rs`

| Constant | Opcode | Retail replacement | Class |
|---|---|---|---|
| `SYNTHETIC_M7_C2S_SHOP_PAGE` | `0x7f40` | none needed — the retail client renders the shop from its own tables (`RUNNING_THE_CLIENT.md:399`) | remove now |
| `SYNTHETIC_M7_S2C_SHOP_PAGE` | `0x7fc0` | none needed, same reason | remove now |
| `SYNTHETIC_M7_C2S_PURCHASE` | `0x7f41` | `0x001d` | keep until a retail-wire purchase E2E test exists |
| `SYNTHETIC_M7_S2C_PURCHASE_COMMITTED` | `0x7fc2` | `0x00c8`+`0x0096`+`0x0068` | keep until a retail-wire purchase E2E test exists |
| `SYNTHETIC_M7_C2S_EQUIP` | `0x7f42` | `0x0020` **does not persist** (`lib.rs:4864`) | keep until retail equipment changes are durable |
| `SYNTHETIC_M7_S2C_EQUIPMENT_CHANGED` | `0x7fc4` | `0x006b` reports stored state, not the change | keep until retail equipment changes are durable |
| `SYNTHETIC_M7_C2S_CONSUME` | `0x7f43` | none | keep until a retail consume path exists |
| `SYNTHETIC_M7_S2C_INVENTORY_CHANGED` | `0x7fc3` | none | keep until a retail inventory-delta frame exists |
| `SYNTHETIC_M7_C2S_REPAIR` | `0x7f44` | none; the client catalog yields `Nondurable` for every record | keep until durability is located in the client records |
| `SYNTHETIC_M7_S2C_REPAIR_COMMITTED` | `0x7fc5` | none, same reason | keep until durability is located in the client records |
| `SYNTHETIC_M7_S2C_COMMAND_RESULT` | `0x7fc1` | `0x0068` status covers purchase refusal only | keep until every economy outcome has a retail expression |
| `synthetic_m7_registry` | — | — | keep until the family is gone |

2 remove now, 10 keep until.

### 3.6 What retail actually depends on

No retail handler decodes or encodes a synthetic packet. It does, however, route through
synthetic *command and result value objects*, because the room actor and the match aggregates
are protocol-agnostic and those types are their vocabulary:

| Type | Defined in | Used by retail at |
|---|---|---|
| `ShotAction`, `ShotResult`, `LoadingComplete`, `Lie` | `m5_solo.rs` | `record_retail_stroke`, `lib.rs:4780`, `:4786`; `lib.rs:4430` |
| `StrokeShotAction`, `StrokeShotResult`, `StrokeLoadingComplete` | `m6_stroke.rs` | `lib.rs:4598`, `:4636`, `:4560` |
| `MatchAbortReason`, `Weather`, `Wind`, `SoloPhase`, `StrokePhaseKind` | `m5_solo.rs`, `m6_stroke.rs` | lobby and match-state plumbing throughout |

Deleting the M5/M6 *wire layouts* is therefore not the same as deleting those modules. The
value objects must move to a wire-free module first — they are domain types that happen to
live in a packet file.

### 3.7 Keep permanently

| Item | Location | Why |
|---|---|---|
| Synthetic loopback TCP hello/login harness | `crates/pangya-protocol/tests/foundation.rs:407` | M1 test scaffolding over the retail LoginService packets; no wire contract of its own |
| `connect_game` / `send_packet` / `receive_packet` E2E helpers | `crates/pangya-server/tests/game_e2e.rs:652`, `:693`, `:701` | shared with `connect_game_retail` (`:668`); only `connect_game`'s four-byte hello is synthetic and it retires with §4 step 1 |

---

## 4. Removal plan

`docs/SPEC.md` §19.1 requires an "End-to-end synthetic — TCP LoginService→GameService→room→
result flow" test layer. The requirement is a **headless TCP path**, not a synthetic
protocol; `connect_game_retail` (`game_e2e.rs:668`) already provides one. Step 6 renames the
row. **No step may leave the project without a headless end-to-end path**, so each step below
names the retail-wire test that replaces the coverage it deletes, and that test must exist and
pass *before* the deletion lands.

### Step 1 — M3 synthetic bootstrap (§3.1, 8 items)

**Removes**: `crates/pangya-protocol/src/game.rs` synthetic half; the `game.retail_bootstrap`
branch in `crates/pangya-game/src/lib.rs` at `:1124`, `:1274`, `:1303`, `:2087`, `:4244`; the
`retail_bootstrap` field on `GameRuntimeConfig` (`lib.rs:608`).

**Blast radius**
- Crates: `pangya-protocol`, `pangya-game`, `pangya-server` (config).
- Config: `game.retail_bootstrap` disappears — the retail bootstrap becomes the only one.
  `config/local.example.toml`, `config/retail-local.example.toml`, `docs/CONFIGURATION.md:61`.
- Tests: six `game_e2e.rs` tests connect with the synthetic hello via `connect_game`
  (`:1315`, `:1451`, `:1599`, `:1743`, `:1816`, `:1900`) and read the synthetic bootstrap
  (`read_bootstrap`, `:991`). All must move to `connect_game_retail`.
- Fixtures: `game-out-hello-synthetic` and `crates/pangya-protocol/tests/game_fixtures.rs`.
- Docs: `PROVENANCE.md` row 12, `docs/protocol/M3_SYNTHETIC_GAME_FLOW.md`, ADR-0010,
  `docs/data/M3_SYNTHETIC_CATALOG.md`, README.

**Evidence required first**
1. A golden fixture pinning the nine-byte `us852_game_hello`, replacing the synthetic one.
2. The six tests above pass through `connect_game_retail`.
3. `game_retail_bootstrap_emits_the_reference_derived_sequence` unchanged and green — it is
   the replacement for `login_bearer_to_game_snapshot_catalog_segments_and_channel_is_real_db`.

**Why first**: it is the only step that removes an opcode *collision*. While `PlayerInfo`
claims `0x0070` and `EquipmentInfo` claims `0x004d`, the codebase asserts two contradictory
meanings for the same opcode, and PROGRESS blocker 10 stays open.

### Step 2 — M5 solo wire layouts (§3.3, 14 items)

**Precondition**: move `ShotAction`, `ShotResult`, `LoadingComplete`, `Lie`, `Weather`,
`Wind`, `MatchAbortReason`, `SoloPhase` out of `m5_solo.rs` into a wire-free module. This is
a mechanical move with no behaviour change and should land as its own commit.

**Blast radius**
- Crates: `pangya-protocol` (`m5_solo.rs`), `pangya-game` (`is_known_solo_opcode`,
  `handle_solo_command` at `lib.rs:2460`-`2640`, the synthetic branches of the room-event
  encoder at `lib.rs:3594`+).
- Config: `[game.solo_practice]` **stays** — the retail one-member start still uses it
  (`lib.rs:4380`).
- Tests: `game_m5_encrypted_tcp_happy_path_persists_once_and_restarts_projection` (3092),
  `game_m5_unclean_in_game_restart_recovers_before_fresh_auth` (3386),
  `game_m5_shot_sequence_and_fixed_window_limits_are_independent` (3469),
  `game_m5_encrypted_tcp_abort_timeout_malformed_and_shutdown_paths_do_not_reward` (3541);
  10 protocol tests in `crates/pangya-protocol/tests/m5_solo.rs`; 5 fixtures.
- Docs: `PROVENANCE.md` row 14, `docs/protocol/M5_SYNTHETIC_SOLO_FLOW.md`, ADR-0012,
  `docs/evidence/M5_SYNTHETIC_SOLO_2026-08-05.md` (mark historical, do not delete).

**Evidence required first**
1. The retail Practice and two-player paths cover their respective happy paths; the two-player path now carries a full-card plan and per-hole progression.
2. **New**: retail-wire equivalents of the abort matrix — loading timeout, disconnect
   mid-hole, malformed shot payload, shutdown during commit, restart recovery — asserting
   that nothing is awarded. This is the largest gap in the step; the retail path has no abort
   frame, so the assertions must be on the durable ledger, not on the wire.
3. **New**: a retail-wire shot-rate-limit test replacing the 3469 test.

### Step 3 — M6 stroke wire layouts (§3.4, 15 items)

**Precondition**: same value-object move for `StrokeShotAction`, `StrokeShotResult`,
`StrokeLoadingComplete`, `StrokeCompletion`, `StrokePhaseKind`, `StrokeAbortReason`.

**Blast radius**
- Crates: `pangya-protocol` (`m6_stroke.rs`), `pangya-game` (`is_known_stroke_opcode`,
  `handle_stroke_command` at `lib.rs:2648`-`2900`).
- Config: `[game.stroke_two]` **stays** — the retail two-member start requires it
  (`lib.rs:4538`, `:4704`).
- Tests: six `game_m6_*` tests (3921, 4213, 4219, 4316, 4618); 11 protocol tests in
  `crates/pangya-protocol/tests/m6_stroke.rs`; 14 fixtures.
- Docs: `PROVENANCE.md` row 15, `docs/protocol/M6_SYNTHETIC_STROKE_FLOW.md`, ADR-0013.

**Evidence required first**
1. `game_retail_two_players_play_and_settle_one_versus_hole` covers the happy path already.
2. **New**: retail-wire forfeit coverage. Retail has no give-up opcode routed, so a forfeit
   can only be driven by disconnect or turn timeout — both must be proven to produce the same
   truthful `WinnerByForfeit` settlement the synthetic path proves today.
3. **New**: retail-wire deadline coverage (loading timeout, game timeout) and the
   shutdown-priority abort, currently only proven at 3921 and 4213/4219.

### Step 4 — M4 room wire layouts (§3.2, 15 items)

Deliberately after the match families: the M4 protocol is the one with the most *unreplaced*
behaviour, and it is also what the match tests use to reach a room.

**Blast radius**
- Crates: `pangya-protocol` (`m4_room.rs`), `pangya-game` (`is_known_room_opcode` at
  `lib.rs:5996`, `handle_room_command` at `lib.rs:2227`-`2440`, and the entire synthetic arm
  of the room-event encoder at `lib.rs:3594`+, which becomes unreachable).
- Tests: `game_m4_tcp_room_lifecycle_authority_password_capacity_and_cleanup` (2036),
  `game_m4_m5_m6_unknown_policies_continue_or_close_and_known_wrong_state_always_closes`
  (2552), `game_m4_command_chat_and_outbound_queues_are_bounded` (2845); 11 protocol tests in
  `crates/pangya-protocol/tests/m4_room.rs`; 4 fixtures.
- Docs: `PROVENANCE.md` row 13, `docs/protocol/M4_SYNTHETIC_LOBBY_ROOM_FLOW.md`, ADR-0011.

**Evidence required first** — this step needs *implementation*, not only tests:
1. Retail `0x000a` room settings, routed and covered.
2. Retail room chat, routed and covered.
3. A retail kick path, or an explicit decision that kick is out of scope with the M4
   coverage retired rather than replaced.
4. `RetailRoomCensus::Add`/`Remove`/`Update` emitted on membership change — this closes the
   "a room does not update while you are sitting in it" gap at `lib.rs:3425` and is the
   replacement for `SYNTHETIC_M4_S2C_MEMBERSHIP_EVENT`.
5. Retail-wire replacements for the owner-only mutation, capacity race, password gating,
   disconnect cleanup, and actor-failure isolation assertions at 2036 — the room actor's
   invariants are protocol-agnostic and must keep their proof.
6. The unknown-opcode-policy test at 2552 must be re-expressed against retail opcodes, since
   its whole subject is which opcodes are known.

### Step 5 — M7 economy wire layouts (§3.5, 12 items)

**Blast radius**
- Crates: `pangya-protocol` (`m7_economy.rs`), `pangya-game` (`is_known_economy_opcode` at
  `lib.rs:5562`, `economy_command_for_opcode` at `:5573`, `handle_economy_command`).
- Config: `[game.economy]` **stays** — retail purchase reads it (`lib.rs:4944`,
  `max_purchase_quantity`, `command_timeout`).
- Tests: seven `game_m7_*` tests (5200, 5398, 5471, 5668, 5741, 5767, 5806); 5 protocol tests
  in `crates/pangya-protocol/tests/m7_economy.rs`; 11 fixtures.
- Docs: `PROVENANCE.md` rows 16-17, `docs/protocol/M7_SYNTHETIC_ECONOMY_FLOW.md`, ADR-0014.

**Evidence required first**
1. **New**: a retail-wire `0x001d` purchase E2E covering commit, idempotent replay (the
   operation id is derived at `lib.rs:5866`), insufficient Pang, not-in-catalog, quantity
   bound, and the economy-disabled refusal. This is the single largest missing retail test.
2. **New**: a retail-wire `0x0020` equipment E2E per slot, asserting that the reply reports
   stored state.
3. **New**: retail-wire economy rate limiting and pre-auth/pre-channel rejection, replacing
   5741 and 5767.
4. A decision on consume and repair: implement retail paths, or retire the coverage
   explicitly. Repair in particular has no basis in the real data — every client record loads
   as `Nondurable`.

### Step 6 — documentation and spec

- `docs/SPEC.md` §19.1: rename the "End-to-end synthetic" layer to "End-to-end retail-wire",
  keeping the same requirement (TCP LoginService→GameService→room→result).
- `docs/protocol/M[3-7]_SYNTHETIC_*.md`: mark historical, retain for the record.
- ADRs 0010-0014: add a superseded-by note; ADRs are not deleted.
- `PROVENANCE.md`: fold rows 11-17 into a single historical row; the fixtures they describe
  no longer exist.
- `docs/PROGRESS.md`: close blocker 10 and the M3-M7 synthetic milestone rows.
- `README.md`, `docs/CONFIGURATION.md`, `docs/RUNNING_THE_CLIENT.md`: drop
  `game.retail_bootstrap` and the synthetic-mode language.

---

## 5. What the synthetic path still proves and the retail path does not

Honest and specific. Each of these is a real capability that disappears the day its synthetic
family does, unless the named retail work lands first.

1. **Typed refusals.** Every synthetic command answers with a result carrying the exact
   failure (`0x7f82`, `0x7fa6`, `0x7fb6`, `0x7fc1`). Retail has two status fields in total —
   `0x0049`'s join rejection and `0x0068`'s purchase status — so the ten distinct M7 wire
   outcomes and every room-command refusal have no retail expression. A retail client that
   asks for something impossible is told nothing, or the connection closes.
2. **Aborts.** `MatchAborted` (`0x7fa7`, `0x7fb7`) distinguishes loading timeout, disconnect,
   turn timeout, shutdown, and forfeit. Retail sends nothing at all on abort
   (`lib.rs:3585`); the client simply finds itself back in the room.
3. **Live room state.** Membership add/remove, chat, settings change, kick, and an explicit
   state query. In retail mode **all** lobby broadcasts are dropped (`lib.rs:3425`), so the
   census is re-sent in full on every room snapshot, so it is correct while a client sits in
   the room; it is not sent during a hole, where it would contradict the match.
4. **Post-settlement balances.** `0x7fa5`/`0x7fb8` push the new Pang and EXP the moment a
   match settles. Retail pushes `0x0095`/`0x0096` only during bootstrap and after a purchase,
   so a grant or a match reward is visible on the next login rather than immediately.
5. **Give-up.** `0x7f34` lets any participant concede, producing a truthful
   `WinnerByForfeit` with no fabricated score. Retail `0x0130` is documented and not routed.
6. **Consume and repair.** `0x7f43`/`0x7f44` with catalog-owned durability and repair rates.
   No retail handler exists, and the real client records yield `Nondurable` for everything, so
   the retail path could not currently repair anything even if it were wired.
7. **Server-side shop paging.** `0x7f40`/`0x7fc0` enumerate offers on the wire. The retail
   client never asks — it renders the shop from its own tables — so this is the one synthetic
   capability with no retail counterpart *by design* rather than by omission.
8. **Sequence discipline.** M6 proves independent per-player sequence numbers and bit-exact
   duplicate coalescing. The retail relay is opaque: a shot frame is counted as one stroke and
   nothing in it is decoded, so a duplicated retail frame is a duplicated stroke.
9. **Golden-fixture and registry coverage.** The four `synthetic_m*_registry` builders and 34
   generated fixture/provenance pairs are the only registry and byte-level coverage the room,
   match, and economy layouts have. **No retail fixture exists for any of them.**

Items 1-4 are the ones a real player would notice. Item 9 is the one that would quietly
reduce test depth, and is the reason step 6's spec rename must be accompanied by retail
fixtures rather than by nothing.

---

## 6. Contradictions found in the current documentation

Recorded here rather than fixed, because this document is analysis. Each is checkable.

| # | Claim | Where | What is actually true |
|---|---|---|---|
| 1 | "Room census (`0x0048`) is the retail replacement and is not implemented yet" | `crates/pangya-game/src/lib.rs:3428` | it is implemented and routed at `:5211`, `:5230`, `:5279`. Only the `Add`/`Remove`/`Update` forms are unemitted |
| 2 | Retail room packets "are **not routed in the runtime**" | `docs/protocol/US852_RETAIL_BOOTSTRAP.md:234` | routed since 2026-08-07 at `lib.rs:5085`-`5245` |
| 3 | Retail match packets "are **not routed**", and the solo-vs-stroke design decision is still open | `docs/protocol/US852_RETAIL_BOOTSTRAP.md:292` | routed at `lib.rs:4370`-`4665`; the decision was taken — the retail start reads the room and picks the stroke aggregate for two members, solo for one (PROGRESS blocker 23) |
| 4 | State table stops at first-character setup, gated on blocker 14 | `docs/RUNNING_THE_CLIENT.md:13`-`23`, and §7 at `:288`-`294` | blocker 14 is resolved; the client reaches the lobby, shops, creates and joins rooms (PROGRESS blockers 14-21) |
| 5 | "A second instance on the same host is not a workaround" | `docs/RUNNING_THE_CLIENT.md:427`-`436` | contradicted 12 lines later by the same file at `:438`-`485` and by PROGRESS blocker 22: `AllowMultipleInstances` in Rugburn does exactly this |
| 6 | `allowed_character_type_ids = [67108864]` — a one-entry allowlist | `config/retail-local.example.toml:191` | PROGRESS blocker 14 records the real client picking `0x0400000b` (67108875) and being refused for exactly this reason. The gate is still live at `crates/pangya-login/src/runtime.rs:876`, so the shipped **retail** example still refuses the client's own first-character pick |
| 7 | "Real client GameService auth ⛔ Blocker 15" in the snapshot table | `docs/PROGRESS.md:42` | blocker 15 is marked resolved at `docs/PROGRESS.md:291` |
| 8 | "Implement equipment update `0x0020`" listed as an immediate next action | `docs/PROGRESS.md:370` | resolved as blocker 17 in the same file and implemented at `lib.rs:4872`. The list also numbers three separate items "2" |
| 9 | README describes the project as synthetic-only, M2-M7 | `README.md` (before this change) | superseded; rewritten alongside this document |

Item 6 is the only one that would cost an operator a debugging session today.
