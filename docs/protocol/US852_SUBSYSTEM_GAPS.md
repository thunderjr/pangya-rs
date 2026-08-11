# U.S. 852 client subsystems this server does not serve — gap analysis

> Compiled 2026-08-08 against the tree at `9c2772b`.

## Claim boundary

Nothing in this document is a capture. Every opcode, layout hint, and response
ordering below is read out of a vendored open-source reference, and each claim
names the file and line it came from. Where two references disagree the
disagreement is recorded rather than resolved; where no reference speaks the row
says **unknown**.

The four reference classes and what each may be used for:

| Reference | Target build | License | Usable for |
|---|---|---|---|
| `pangbox--packetdoc` | see below — **gameservice captures are TH.R6.829.01 only** | ISC | Field layouts, response ordering, opcode identity |
| `pangbox--server` | GB/US 852 aspiration; TH-informed | ISC | Handler behaviour, opcode identity, constants |
| `hex-agon--alter-pangya` | GB.852 | **no license grant** | Protocol facts only (opcode values, field order). No code copied |
| `K4T--Py_Source_US` | ProjectG 852.00 / 824.00 GB | GPL-3.0 | Behaviour and opcode identity only. **No layout, no code** — the project's GPL boundary in [`../PROVENANCE.md`](../PROVENANCE.md) stands |
| `hsreina--pangya-server` | PangYa FreshUp (different season) | Apache-2.0 | Opcode identity, cross-check only |

### The largest single trap, stated up front

`opensource-references/pangbox--packetdoc/examples/` contains **no GB/US 852
GameService capture at all**. Every `examples/gameservice/{client,server}/`
directory is `TH.R6.829.01`; the only 852 examples in the repository are
`loginservice/client/GB.R7.852.00` and `messageservice/client/GB.R7.852.00`. The
`.ksy` files carry no `us_852`/`th_829` version guard either — `grep -rl us_852
src/packets/gameservice/` returns nothing, even though `src/packets/common/version.ksy:6-24`
declares an eleven-region enum including `us_852`.

Therefore: **every GameService layout in PacketDoc is TH 829 evidence presented
without a version qualifier.** It is the best structural evidence available and
should be used, but a layout that a real U.S. 852 client rejects is a PacketDoc
bug, not a server bug, and must be corrected against the client rather than
argued from the `.ksy`. This is the same posture
[`US852_RETAIL_BOOTSTRAP.md`](US852_RETAIL_BOOTSTRAP.md) already takes.

---

## 1. What this server answers today

Read out of `crates/pangya-game/src/lib.rs` and `crates/pangya-protocol/src/game.rs`.

| Class | Opcodes | Where |
|---|---|---|
| Session state machine | `0x0002`, `0x0004` | `lib.rs:1250`, `lib.rs:1271` |
| Lobby chatter answered | `0x016e`, `0x009c` | `lib.rs:1324-1325` |
| Equipment / economy | `0x0020`, `0x001d` | `lib.rs:1349`, `lib.rs:1365` |
| Lobby services | `0x0140`, `0x00b5`, `0x00b7`, `0x00d3`, `0x00cc` | `lib.rs:1383-1387` |
| Room | `0x0008`, `0x0009`, `0x000f`, `0x000d`, `0x0081`, `0x0082` | `lib.rs:5984-5996` |
| Match | `0x000e`, `0x0011`, `0x0012`, `0x001b`, `0x001c`, `0x0031` | `lib.rs:5968-5978` |
| Accepted, never answered | `0x0007`, `0x0018`, `0x0032`, `0x0033`, `0x004f`, `0x0069`, `0x0088`, `0x008b`, `0x00c1`, `0x00fe` | `game.rs:273-284` |

**Everything else disconnects on the first frame.** Both shipped configs set
`unknown_opcode_policy = "disconnect"` (`config/local.example.toml:123`,
`config/retail-local.example.toml:153`), and `unknown_decision`
(`crates/pangya-game/src/lib.rs:5541-5546`) makes that policy strike-independent:
`disconnect: true` on the *first* unrecognized opcode, ignoring
`unknown_opcode_strikes`. The strike budget only applies under `ignore` or
`capture`.

So the consequence column in §2 is not "the feature is missing". It is: the
session dies the moment the player touches that part of the UI. Upstream behaves
the same way — `pangbox--server/game/server/conn.go:879-880` returns
`fmt.Errorf("unexpected message: %T")` from its dispatch default, which tears the
connection down.

---

## 2. Unhandled client→server opcodes, by subsystem

Opcode identity columns: **PD** = `pangbox--packetdoc/src/packets/gameservice/client/index.ksy`;
**PB** = `pangbox--server/pangya/game/packet/client.go`; **AP** =
`hex-agon--alter-pangya/game-server/src/main/kotlin/work/fking/pangya/game/net/ClientPacketType.kt`;
**PY** = `K4T--Py_Source_US/Src/Py_Game/Py_Game/Defines/PangyaEnums.cs`.

Consequence is derived from PacketDoc's own `doc:` prose (`The response is X` ⇒ the
client blocks on a reply; `There is no response` ⇒ fire-and-forget) combined with
the shipped disconnect policy. Under the shipped config the *transport* outcome is
always `disconnect`; the column records what would happen once the opcode is
merely accepted, because that is the decision that matters when scoping work.

### 2.1 Chat and lobby social — **breaks a working session today**

| Opcode | Meaning | Evidence | S→C | If accepted but unanswered |
|---|---|---|---|---|
| `0x0003` | public chat send | PD:122; PB:28; conn.go:170 | `0x0040` sub 0x00 | Silent no-op — the sender sees nothing echoed back, because the client renders only what the server relays (`server/0040.ksy:68-73`) |
| `0x002a` | whisper send | PD:148 (stub); PY:633 | `0x0084` | Silent drop; whisper UI shows no delivery |
| `0x003c` | offline note send (costs 10 Pang) | PD:152; PY (`PLAYER_REQUEST_CHAT_OFFLINE`) | `0x0095` money update | Balance never moves; client may resend |
| `0x0029` + `0x00ba` | room invite (sent as a pair) | PD:147,175; PY:—; `client/00ba.ksy` doc | `0x0083` to invitee, `0x012f`/`0x0130` to inviter | Invite silently vanishes |
| `0x0038` | change nickname in-game | PY:550 only | unknown | **unknown** — no layout in any permissive source |
| `0x0007` | user status / gift eligibility | PD:125; `UserStatusRequest` | `0x00a1` body `0x02` | Implemented: validates discriminator `1` + username and answers the mandatory one-byte response. |
| `0x00c1` | **accepted-silent today**; PY calls it `PLAYER_REQUEST_WEB_COOKIES`, PB calls it my-room-related (conn.go:687) | PD:— ; PY:— ; AP:84 | unknown | Two references disagree on meaning. Currently harmless because the client reaches the lobby, but the cash-shop path is untested |

`0x0003` is the highest-value single row in this document: it is one packet in,
one packet out, no durable state, and its absence kills a session that is
otherwise fully working.

### 2.2 Room, beyond create/join/leave/ready

| Opcode | Meaning | Evidence | S→C | Consequence |
|---|---|---|---|---|
| `0x000a` | room settings change | PD:128; PB:34; conn.go:483; AP:51 | `0x004a` | Host edits the room and nothing applies |
| `0x000b` | equipment update **in lobby** | PD:129; AP:52 | `0x004b` | See trap 5.1 — PB:35 calls this `ClientTutorialStart` |
| `0x000c` | equipment update **in game room** | PD:130; PB:36; AP:53 | `0x004b` | Changing clubs inside a room does not propagate to the roster |
| `0x0010` | team change | PD:133; PY | `0x007d` | Team modes unusable |
| `0x0026` | room kick | PB:55; conn.go:475; PY (`PLAYER_MASTER_KICK_PLAYER`) | room census refresh | SPEC §22.3 requires owner-restricted kick |
| `0x002d` | room info request | PD:149; PB:56; conn.go:373; PY:— | `0x0086` | Room detail popup in the directory is blank/hangs |
| `0x0063` | lounge/room emote action | PD:156; PB:68; conn.go:224; AP:75 | `0x00c4` | Emotes silently dropped |
| `0x00eb` | lounge per-user query, one per occupant | PD:184 | `0x0196` | Lounge (PSQUARE) rooms unreachable |

### 2.3 Match, beyond the six wired opcodes

This whole group is downstream of PROGRESS blocker 23. It is listed because the
two-player match cannot be validated without it, not as separate breadth.

| Opcode | Meaning | Evidence | S→C |
|---|---|---|---|
| `0x0006` | full-match statistics submit | PD:124; PB:30 | `0x0045` |
| `0x0013` | shot rotate | PD:135; PB:41; conn.go:310; AP:58 | `0x0056` |
| `0x0014` | shot meter input | PD:136; PB:42 (conn.go:315 = TODO); AP:59 | none observed |
| `0x0015` | power toggle | PD:137; PB:43; conn.go:319; AP:60 | `0x0058` |
| `0x0016` | club change | PD:138; PB:44; conn.go:324; AP:61 | `0x0059` |
| `0x0017` | item use | PD:139; PB:45; conn.go:329; AP:62 | `0x005a` |
| `0x0019` | comet relief / ball drop | PD:141; PB:48; conn.go:347 | `0x0060` |
| `0x001a` | hole info (tee/pin/par) | PB:49; conn.go:454; AP:63 | none |
| `0x0022` | active-user acknowledge | PD:146; PB:54; conn.go:367; AP:68 | `0x0063` |
| `0x0030` | pause | PB:58 (conn.go:365 = TODO) | unknown |
| `0x0034` | first-shot-ready / finish preview | PD:—; PB:62; conn.go:371; AP:71 | `0x0090` |
| `0x0037` | last player leaves game | PB:63; conn.go:470 | room leave |
| `0x0042` | shot arrow | PB:64 (conn.go:317 = TODO); AP:72 | unknown |
| `0x0048` | load progress | PD:155; PB:66; conn.go:282; AP:73 | `0x00a3` |
| `0x0065` | time booster | PD:157; AP:76 | `0x00c7` |
| `0x0184` / `0x0185` | assist toggle / activate | PD:210,211; PB:94; conn.go:261 | `0x026a` / `0x026b` |
| `0x00aa` | tiki-report early exit | PD:171 | `0x012a`, `0x0045`, `0x004c` |
| `0x012f` | tourney shot | AP:91; PY:609 — **not in PacketDoc** | unknown |
| `0x0130` | match player quit | AP:86; PY:612 — **not in PacketDoc** | unknown |
| `0x0141` | hole-repeat wind change | AP:88; PY:619 — **not in PacketDoc** | unknown |

The last three are the strongest 852-specific signal in this table: two
independent GB/US-852 servers agree on opcodes PacketDoc's TH corpus never saw.

### 2.4 Gacha — papel, scratchy, memorial, lootbox

| Opcode | Meaning | Evidence | S→C sequence |
|---|---|---|---|
| `0x0098` | rare shop open | PD:168; PB:74; conn.go:493; AP:80 | `0x010b` |
| `0x014b` | Black Papel play | PD:195; PB:88; conn.go:575; AP:92 | `0x0216`, `0x00fb`, `0x021b`, `0x0216`, `0x022e`, `0x0220` |
| `0x0186` | Big Black Papel play | PD:212; PB:95; conn.go:541 | `0x00c8`, `0x0216`, `0x00fb`, `0x026c`, `0x0216`, `0x022e`, `0x0220` |
| `0x012a` | scratchy menu open | PD:189; PB:84; conn.go:874 | `0x01eb` |
| `0x0070` | scratchy play | PD:159; PY:657 | `0x0216`, `0x00dd` |
| `0x0071` | scratchy serial entry | PY:658; hsreina `PLAYER_ENTER_SCRATCHY_SERIAL` — **not in PacketDoc** | unknown |
| `0x017f` | memorial coin play | PD:209; PY | `0x0216`, `0x0264`, `0x0216`, `0x022e`, `0x0220` |
| `0x00ef` | lootbox open | PD:185; PY:— (`PLAYER_OPEN_BOX = 0x00EF`) | `0x00a7`, `0x00aa`, `0x019d`, `0x0216`, `0x022e`, `0x0220` |
| `0x00ec` | "Aztec box" | PY:641 only — **collides with server `0x00ec` item-transact**; client space is separate, but the collision makes cross-source confusion easy | unknown |

`0x0098` has a known client bug worth recording: upstream sends `UnknownA = 50`
with the comment *"Prevents 'No Draws Left' bug with Big Papel Shop"*
(`conn.go:494-496`), while PacketDoc's `server/010b.ksy` documents all three
fields as `-1, -1, 0` from TH captures. Another TH/852 divergence candidate.

Every one of these response chains ends in `0x0216` + `0x022e` + `0x0220`, which
means **achievements are not an independent subsystem** — they are a tail on
every reward path. See §3.

### 2.5 Cards and character mastery

| Opcode | Meaning | Evidence | S→C |
|---|---|---|---|
| `0x00ca` | cardholic pack open | PD:176; PY:719 | `0x0154` |
| `0x00bd` | card special | PY:720 only | unknown |
| `0x0155` | card exchange (3→1) | PD:200 (`tiki_shop_card_exchange`); PY:724 (`PLAYER_LOLO_CARD_DECK`) — **names disagree** | `0x0229`, `0x0216`, `0x022a`, … |
| `0x018a` | apply card to mastery slot | PD:216; PY | `0x0271`, `0x0216`, `0x022e`, `0x0220` |
| `0x018b` | put bonus card | PY:722 only | unknown |
| `0x018c` | remove card | PY:723 only | unknown |
| `0x0187` | mastery slot unlock | PD:213 | `0x026e`, `0x0216`, `0x022e`, `0x0220` |
| `0x0188` | mastery upgrade | PD:214; PY | `0x026f`, `0x0216`, `0x022e`, `0x0220`, `0x00c8` |
| `0x0189` | mastery downgrade | PD:215; PY | `0x026f`, … |

Bootstrap dependency: the card inventory container `0x0138` is part of the
login sequence (`alter-pangya .../packet/outbound/CardholicReplies.kt:10`), and a
character record carries twelve card slots
(`.../player/Character.kt:16,67,89`). Card state is therefore visible before any
card opcode is ever sent.

### 2.6 Rings

| Opcode | Meaning | Evidence |
|---|---|---|
| `0x015d` | ring proc / ring effects request | AP:90 (`PLAYER_RING_PROC`); PY:624 (`PLAYER_REQUEST_RING_EFFECTS`) — **absent from PacketDoc client space**, where `0x015d` is a *server* guild response |
| `0x015c` | "animalhand effect" request | PY only — same collision shape |

Two 852-targeting sources agree on `0x015d`; no permissive source documents the
body. **Layout unknown.** The rare shop that dispenses rings is `0x0098` (§2.4);
the ring *inventory* rides the normal `0x0073` container.

### 2.7 Caddies and mascots

| Item | State today | Evidence |
|---|---|---|
| Caddie roster `0x0071` | Container exists, **always sent empty** | `crates/pangya-protocol/src/us852_bootstrap.rs:320-321,524-527` |
| Caddie equip | Works — `0x0020` slot 1 is decoded and answered | `us852_room.rs:455` (`0x0020` decode), `us852_room.rs:524` (`0x006b` reply) |
| Caddie renewal `0x006b` | Unhandled | PY:746 (`PLAYER_REQUEST_CADDIE_RENEW`) only |
| Mascot roster `0x00e1` | **Not implemented at all** — part of the login sequence | `alter-pangya .../packet/outbound/MascotRosterPacket.kt:9`; hsreina `PacketsDef.pas` `PLAYER_MASCOTS_DATA = $00E1` |
| Mascot equip | `0x0020` slot **8** — the repo calls it `UnknownEight` | `us852_room.rs:405,421`; identified as `MASCOT(8)` in `alter-pangya .../MyRoomEquipmentUpdateReplies.kt:15` |
| Mascot equip reply | The `0x006b` encoder has no slot-8 arm at all | `us852_room.rs:476-503` — `RetailEquipmentUpdated` covers only slots 1–5 |
| Cut-in equip | `0x0020` slot **9** — the repo calls it `UnknownNine` | same file, `CUT_IN(9)` |
| Mascot text `0x0073` | Unhandled | PY:651; hsreina `PLAYER_SET_MASCOT_TEXT = $0073` |
| Cut-in call `0x00e5` | Unhandled | PY:726 only |

Two of the repo's own "unclassified" equipment slots are now identified by an
852-targeting reference. That is a cheap, high-confidence doc/naming fix
independent of any feature work.

### 2.8 Quests and achievements

| Opcode | Meaning | Evidence | S→C |
|---|---|---|---|
| `0x0151` | quest status request | PD:196; PB:89; conn.go:864 | `0x0216`, `0x0225` |
| `0x0152` | accept offered quest lineup | PD:197 | `0x0226` |
| `0x0153` | submit completed quest | PD:198 | `0x0216`, `0x0227`, `0x0216`, `0x022e`, `0x0220` |
| `0x0154` | dismiss quest | PD:199; hsreina `PLAYER_GIVEUP_DAILY_QUEST` | `0x0228` |
| `0x0157` | achievement status request | PD:201; PB:90; conn.go:498; AP:89 | `0x022d` (**segmented, 20 per frame**), `0x022c` |

Upstream's achievement reply is annotated *"Actually support achievements. This
is the bare minimum not to hang/crash the game"* (`conn.go:500`) and ships five
fabricated achievement rows with 2018/2019 timestamps. That is a strong signal
that `0x0157` **blocks the client** rather than no-opping, and that an
empty-but-well-formed reply may not be enough — which is exactly the shape of
risk that needs a real client to settle.

Bootstrap dependency: `0x021d` (progress, segmented 300 per frame,
`server/021d.ksy:3-10`) and `0x021e` are documented as responses to the login
`0x0002`, not to `0x0157`.

### 2.9 Daily and login rewards

| Opcode | State | Evidence |
|---|---|---|
| `0x016e` status request | **Handled** — answered with the "already collected, nothing claimable" form | `game.rs:206-237` |
| `0x016f` claim | Unhandled | PD:204; AP:97; PY:— (`PLAYER_REQUEST_ITEM_DAILY = 0x016F`) |

The current design is deliberately safe: `client/016f.ksy` states the claim is
*"always and only called after `0x0248` shows an unclaimed bonus"*, so answering
`0x0248` with `bonus_collected = 1` makes `0x016f` unreachable. Implementing
login bonuses means implementing both halves together or not at all — shipping
`0x0248`'s uncollected branch alone advertises a reward whose claim disconnects
the client.

### 2.10 Mail / inbox

| Opcode | Meaning | Evidence | S→C |
|---|---|---|---|
| `0x0143` | mailbox page request | PD:190; PB:86; conn.go:385 | `0x0211` |
| `0x0144` | read one mail | PD:191; PB:87; conn.go:397 | `0x0212` |
| `0x0145` | send mail (100 Pang; +500/attachment, first −100) | PD:192; hsreina | `0x0213` |
| `0x0146` | take attachments to inventory | PD:193 | `0x0216`, `0x0214`, then client re-sends `0x0143` |
| `0x0147` | delete mail | PD:194 | `0x0215` |
| — | unread list at login | `server/0210.ksy:3-5` — response to `0x0002` | `0x0210` |

Upstream stubs `0x0143`/`0x0144` with a hardcoded single message from
`@Pangbox` and a `// TODO: need new sql message table` (`conn.go:386`). Whether
the client tolerates a genuinely empty mailbox (`NumMessages = 0`) is
**unknown** — no reference ships that case.

### 2.11 Guilds

| Opcode | Meaning | Evidence |
|---|---|---|
| `0x0108` | guild directory page | PD:186; PB:83; conn.go:869; hsreina |
| `0x0109` | guild search | PD:187; hsreina |
| `0x0101`,`0x0102`,`0x0104`,`0x0105`,`0x0106`,`0x0107`,`0x010a`,`0x010c`,`0x010d`,`0x010e`,`0x0110`–`0x0116` | create / name check / data / notice / intro / destroy / log / join / cancel / accept / promote / self-intro / member / leave / kick / emblem upload | PY:679-700 — declared but **entirely commented out** in that source's dispatch (`GPlayer.cs:558-594`), and absent from PacketDoc |

Upstream answers `0x0108` with `ServerGuildListPage{Status: 1}` and a `// TODO`
(`conn.go:869-873`), i.e. an error status, which the client evidently tolerates.
`server/01bc.ksy:3` notes *"The number of guilds on a page is set at 15 in
PangyaTH"* — another explicitly TH-derived constant.

Guild identity also leaks into packets already implemented: `conn.go:66,80` hardcodes
`GuildEmblemImage: "guildmark"` in both the lobby and room player entries, and
`0x015d` is a per-user guild response inside the `0x002f` burst.

### 2.12 MyRoom, UCC, furniture

| Opcode | State | Evidence |
|---|---|---|
| `0x00b5` / `0x00b7` | **Handled** | `us852_room.rs:617,673` |
| `0x012d` MyRoom layout | **Sent, furniture count 0** | `us852_room.rs:698`; upstream does the same (`conn.go:670-673`) |
| `0x00b9` custom asset (UCC) request | Unhandled | PD:174; PY:— (`PLAYER_AFTER_UPLOAD_UCC = 0x00B9`) |
| `0x00c9` upload key request | Unhandled | PY:595 only |
| furniture place / remove / buy | **No client opcode found in any reference** | — |

Furniture placement is the clearest **unknown** in this document. `Furniture.iff`
and `FurnitureAbility.iff` both exist in the client's catalog
([`../data/US_CLIENT_IFF_STRUCTURE.md`](../data/US_CLIENT_IFF_STRUCTURE.md)),
and `FurnitureAbility` is header-only (`count = 0`), but no reference documents a
placement packet. UCC additionally needs an HTTP upload endpoint that
`pangya-updater`'s `[client_web]` listener does not expose.

### 2.13 Locker

| Opcode | State | Evidence | S→C |
|---|---|---|---|
| `0x00cc` combination attempt | **Handled** | `us852_room.rs:797` | `0x016c` |
| `0x00d3` inventory request | **Handled** | `us852_room.rs:747` | `0x0170` |
| `0x00cd` page request | Unhandled | PD:178; hsreina | `0x016d` |
| `0x00ce` deposit item | Unhandled | PD:179 | `0x0139`, `0x00ec` type 1, `0x016e` |
| `0x00cf` withdraw item | Unhandled | PD:180 | `0x00ec` type 0, `0x016f` |
| `0x00d4` Pang deposit/withdraw | Unhandled | PD:182; hsreina | `0x0171`, `0x00c8`, `0x0172` |
| `0x00d5` locker Pang balance | Unhandled | PD:183; hsreina | `0x0172` |
| `0x00d0` first-time locker setup | Unhandled | PY:661 only | unknown |
| `0x00d1` change locker password | Unhandled | PY:664; hsreina | unknown |

Note the server-side opcode collision: locker deposit-B is server `0x016e` while
client `0x016e` is the login-bonus request. Both are already in this codebase's
vocabulary in *different* directions; the codec keys on direction, so this is
safe, but it is an easy source of review confusion.

### 2.14 Rentals and time-limited items

| Opcode | Meaning | Evidence |
|---|---|---|
| `0x00e6` | renew rental | PY:732 only — **collides with server `0x00e6` user-shop-inventory** |
| `0x00e7` | delete/cancel rental | PY:733 only — same collision shape |

Catalog side: the client ships `TimeLimitItem`, `SubscriptionItemTable`, and
`ShopLimitItem` tables ([`../data/US_CLIENT_IFF_STRUCTURE.md`](../data/US_CLIENT_IFF_STRUCTURE.md)),
none of which `pangya-data` parses. Expiry is explicitly deferred by SPEC §22.2.
**Layouts unknown; single GPL source only.**

### 2.15 Events / Grand Prix

| Opcode | Meaning | Evidence | S→C |
|---|---|---|---|
| `0x0176` | event mode join (opens GP directory) | PD:205; PB:92; conn.go:434 | `0x0250` |
| `0x0177` | event mode leave | PD:206; PB:93; conn.go:437 | `0x0251` |
| `0x0179` | event room join | PD:207; PY | `0x0049`, `0x0253` |
| `0x017a` | event room leave | PD:208; PY | `0x0254` |

Server-side tournament traffic that has no client trigger: `0x006c` user finish,
`0x006d` user update, `0x006e`/`0x01f7` unclassified announces, `0x00ce` item
winnings, `0x0040` sub 0x0c countdown / sub 0x11 completion
(`server/0040.ksy:78-95`), `0x0257` GP trophy.

`client/0176.ksy:2-5` documents mode-join as *sticky*: set when the GP directory
opens, cleared only when the player leaves every event room **and** the
directory. That is a session flag the current `0x0081`/`0x0082` multiplayer-mode
handling would need to mirror.

### 2.16 Friends / messenger / presence

| Opcode | State | Evidence |
|---|---|---|
| `0x008b` messenger server list request | Empty request answered with `0x00fc` count `0` | `client/008b.ksy`, `server/00fc.ksy` |
| `0x00fc` messenger server list | Implemented zero-server response | `social.rs:MessageServerList` |

`client/008b.ksy:2-4` is precise about when it fires: *"only sent if the client
has failed to connect to the message server at login"*. So the current silent
accept is behaviourally correct **only** because LoginService already answers
`0x0009` with an empty message-server list — the client never had a server to
fail against. Once a MessageService exists, `0x008b` must answer `0x00fc` or the
messenger recovery path dead-ends.

MessageService itself is a separate listener with its own eight-opcode client
table (`packetdoc/src/packets/messageservice/client/index.ksy`): `0x0012`
credential declaration, `0x0014` hello, `0x0016` goodbye, `0x0017` user-id
lookup, `0x0018` friend request, `0x001d` status declaration, `0x001e` send
message, `0x0023` server declaration; server side is only `0x002f` and `0x0030`.
That is the smallest complete subsystem in this entire document — and the only
one whose `examples/` directory holds **GB.R7.852.00** captures.

### 2.17 Statistics, profile, records, rankings

| Opcode | Meaning | Evidence |
|---|---|---|
| `0x002f` | user information request | PD:150; PB:57; conn.go:186; AP:69 |
| `0x0006` / `0x0031` | match / hole statistics submit | PD:124,151 |
| `0x003d` | cookie balance request | PD:153; hsreina |
| `0x00a2`, `0x0195` | cookie-menu chatter | PD:170,218 |
| `0x0066`, `0x0067` | top notice / notice cookie | PY:742 only |

`0x002f` with `request_type == 5` triggers a **thirteen-packet** response burst
(`client/002f.ksy:2-20`): `0x0157` name, `0x015e` character, `0x0156` equipment,
`0x0158` statistics, `0x015d` guild, `0x015c` × 3 (natural-wind, Grand Prix,
original course records), `0x015b`, `0x015a`, `0x0159` trophies, `0x0257` GP
trophy, `0x0089` terminator. Upstream sends only four of these and annotates
`// TODO: Missing a lot of responses` (`conn.go:218`). Whether the client
tolerates the short form is **unknown** — upstream is TH-informed and this is the
kind of thing 852 diverges on.

No ranking opcode exists in any client table. SPEC §10.5's RankingService is
therefore out-of-band (web/HTTP), consistent with `0x015c` carrying *per-user*
course records rather than a leaderboard.

### 2.18 Tutorial

| Opcode | Meaning | Evidence |
|---|---|---|
| `0x000b` | PB:35 `ClientTutorialStart`, conn.go:612 replies with a 380-byte `0x004b` blob — **contradicted** by PD:129 and AP:52, which both call `0x000b` an equipment update | see trap 5.1 |
| `0x00ae` | tutorial clear/mission | PB:76; conn.go:651 → `0x011f`; PY (`PLAYER_TUTORIAL_MISSION = 0x00AE`); hsreina `PLAYER_CLEAR_QUEST` — **absent from PacketDoc** |
| `0x011f` | tutorial status (server) | PB server table; not in PacketDoc server index |

Tutorial is the worst-evidenced subsystem here: its only detailed reference is
the one that is most likely wrong about `0x000b`, and its server reply is a
hardcoded opaque byte array.

### 2.19 Personal shop (lounge), club workshop, misc

Included because the opcode table implies them, not because they were requested.

| Opcode(s) | Meaning | Evidence |
|---|---|---|
| `0x0075`–`0x007d` | personal shop: close, edit, enter, name, visitor count, income, item list, buy | PD:160-162 (`0x0077`/`0x0078`/`0x007d`); PY:580-587; hsreina — **`0x0078` disputed**: PD says "user shop leave", PY says "create visitors count" |
| `0x004b` | clubset upgrade | AP:74; PY:683 (`PLAYER_UPGRADE_CLUB_SLOT`); hsreina (`PLAYER_UPGRADE`) |
| `0x0164`–`0x0167` | club upgrade / accept / cancel / rank-up | PY:672-675 |
| `0x0167`–`0x0169` | club workshop rank-up / decline / accept | AP:93-95 — **conflicts with the PY numbering above** |
| `0x016b`,`0x016c`,`0x016d` | clubset abbot / transfer club point / clubset power | PY only |
| `0x0158` | Cadie's Magicbox convert | PD:202; PY:644 |
| `0x018d` | recycle items to Tiki Points | PD:217; PY; hsreina | 
| `0x0064` | delete item from inventory | PY:640; hsreina `PLAYER_DELETE_ITEM` |
| `0x0043` | server list request | PD:154; PB:65; conn.go:232; hsreina |
| `0x0083` | sub-server connect (multiplayer mode) | PD:165; PY:— |
| `0x0119` | new session key for server switch | PD:188; PY:— | `0x01d4` |
| `0x005c` | server time request | PY:634; hsreina |
| `0x008f`, `0x003e`, `0x0041`, `0x0057`, `0x0060`, `0x0061` | GM commands | PY; hsreina — out of scope, but they must not fall through to `disconnect` for an operator account |

---

## 3. `0x0216` is the primitive everything else needs

Read the S→C column of §2.4, §2.5, §2.8, §2.10, §2.14 together: **`0x0216`
User Status Update appears in every single reward path.** It is the packet that
adds and removes items, advances quest counters, advances achievement counters,
and moves character mastery — `server/0216.ksy:12-17` describes it as *"add or
subtract items, quest/achievement progress, character mastery, and likely
others"* and then, verbatim, *"This packet is a complete mess."*

Its shape: a 4-byte timestamp, a `u4` change count, then per change a `u1`
subtype (`0x02` items/achievements/quests, `0xc9` character mastery, `0xcc`
unknown-72-bytes) with `status_id`, `status_slot`, `amount_old`, `amount_new`,
signed `amount_delta`, and 25 trailing unknown bytes.

That structure maps cleanly onto SPEC §14.3: `amount_old`/`amount_new`/`delta`
per change is a ledger row on the wire. Building `0x0216` once, backed by the
existing `item_ledger` and `economy_operations` tables, unlocks a large fraction
of §2 — and building any of §2.4/§2.5/§2.8 *without* it means building it twice.

The paired achievement tail — `0x022e` unlocked, `0x0220` update — appears just as
universally. A no-op-but-well-formed `0x022e`/`0x0220` is a prerequisite for
every gacha and quest path even if achievements themselves are never scored.

---

## 4. Per-subsystem minimum viable contract

Authority column cites SPEC §12.4 (client-computed shot data may be relayed but
never credited) and §14.3 (currency invariants: nonnegative, ledgered in the same
transaction, idempotency-keyed, checked integers, never packet-provided).

| Subsystem | Minimum in / out | Durable state | Must be server-authoritative | New migration | New catalog data | Effort | Risk |
|---|---|---|---|---|---|---|---|
| **Chat** `0x0003` | in `0x0003`; out `0x0040` sub 0x00 to room or lobby | none (transient) | length + rate limits per SPEC §22.3; sender identity from the session, never from the packet's `user_nickname` field | no | no | **XS** | Low |
| **Room settings / equipment-in-room** `0x000a`,`0x000b`,`0x000c` | out `0x004a`, `0x004b` | reuses `equipment_sets` | ownership + compatibility check before applying; settings authorization = room owner | no | no | S | Med — `0x000b` semantics disputed |
| **Match completion set** (§2.3) | 18 opcodes in, ~12 out | `matches`, `match_players`, `course_records` exist | §12.4 in full: participant check, active-player check, phase check, finite floats, bounds, idempotent sequences; rewards computed server-side | maybe (multi-hole plans) | no | **XL** | High — blocker 23 |
| **Rare shop + Papel** `0x0098`,`0x014b`,`0x0186` | out `0x010b`; `0x0216`+`0x00fb`+`0x021b`/`0x026c` | prize pool table, draw audit rows | price from catalog only; RNG server-side with a persisted seed per §12.6; Pang debit and item grant in one transaction with an idempotency key per §14.3 | **yes** | **yes** — no `Papel`/`Bongdari` table exists in the client's 39; pool is server-defined | M | Med |
| **Scratchy** `0x012a`,`0x0070`,`0x0071` | out `0x01eb`; `0x0216`+`0x00dd` | card consumption + audit | same as Papel | **yes** | `ScratchRewardSetting` exists but is **header-only, `count = 0`** in the real client — rewards are server-defined | M | Med |
| **Memorial / lootbox** `0x017f`,`0x00ef` | out `0x0216`,`0x0264` / `0x00a7`,`0x00aa`,`0x019d` | coin/box consumption | same as Papel | **yes** | `SpecialPrizeItem` (unparsed) | M | Med |
| **Cards + mastery** `0x00ca`,`0x0155`,`0x0187`–`0x018a` | out `0x0154`,`0x0229`/`0x022a`,`0x026e`/`0x026f`/`0x0271`, all with `0x0216` | card inventory, per-character 12 card slots, per-character mastery counters | mastery deltas and card consumption server-side; Pang cost from catalog | **yes** | **yes** — `Card.iff` unparsed | L | Med |
| **Rings** `0x015d` | unknown | unknown | — | ? | ? | **?** | **High — layout unknown** |
| **Caddies** | fill `0x0071`; `0x0020` slot 1 already works | `inventory_items` suffices for ownership; caddie experience/level needs a column | equip must be owned | small | **yes** — `Caddie.iff`, `CaddieItem.iff` unparsed | S | Low |
| **Mascots** `0x00e1`,`0x0073`, `0x0020` slot 8 | out `0x00e1` roster at login; `0x006b` slot 8 ack | equipped mascot + text | owned check; text length limit | small | **yes** — `Mascot.iff` unparsed | S | Low |
| **Quests** `0x0151`–`0x0154` | out `0x0216`,`0x0225`–`0x0228` | per-account quest slots, offered lineup, expiry | progress counters server-derived from match results, never packet-claimed; rewards per §14.3 | **yes** | **yes** — `QuestStuff.iff`/`QuestItem.iff` unparsed (note: PacketDoc says `Quest.iff`, which the U.S. client does **not** ship) | L | Med |
| **Achievements** `0x0157` + `0x021d`/`0x021e`/`0x0220`/`0x022e` | segmented `0x022d` (20/frame), `0x022c`; login-time `0x021d` (300/frame) | per-account achievement progress | all counters server-derived | **yes** | **unknown** — the U.S. client ships **no `Achievement.iff`**; where the definitions live is unresolved | L | **High — no catalog source found** |
| **Login bonus** `0x016e`+`0x016f` | out `0x0248` uncollected branch, then `0x0216`+`0x0249` | per-account claim ledger keyed by day | claim exactly once per period; grant per §14.3 | **yes** | `LevelUpPrizeItem`? unclear | S | Low — but must ship both halves together |
| **Mail** `0x0143`–`0x0147`, `0x0210` | out `0x0211`–`0x0215`, `0x0216` for attachments | messages, attachments, read/unread | send cost (100 Pang + 500/attachment) charged server-side; attachment transfer is a transactional item move per §14.5 shape | **yes** | no | M | Low |
| **Guilds** `0x0108`,`0x0109` (+ ~17 more) | out `0x01bc`,`0x01bd`,`0x015d` | guilds, members, roles, emblem blob | membership/role authorization on every mutation | **yes**, large | no | **XL** | Med — SPEC §22.6 explicitly defers guilds |
| **MyRoom furniture / UCC** `0x00b9`,`0x00c9` | out `0x012e`; layout via `0x012d` | furniture placements, UCC asset blobs | ownership; asset size/format bounds; UCC needs an HTTP upload surface | **yes** | **yes** — `Furniture.iff` unparsed | L | **High — placement opcodes unknown** |
| **Locker** `0x00cd`–`0x00d5` | out `0x016d`,`0x0139`,`0x00ec`,`0x016e`,`0x016f`,`0x0171`,`0x0172`,`0x00c8` | locker item rows, locker Pang balance, combination hash | Pang move between wallet and locker is a §14.3 currency mutation with a ledger entry; combination stored hashed | **yes** | no | M | Low |
| **Rentals** `0x00e6`,`0x00e7` | unknown | expiry timestamps on `inventory_items` | expiry evaluated server-side | **yes** | **yes** — `TimeLimitItem`, `SubscriptionItemTable` | M | **High — GPL-only source** |
| **Events / Grand Prix** `0x0176`,`0x0177`,`0x0179`,`0x017a` | out `0x0250`,`0x0251`,`0x0049`+`0x0253`,`0x0254` | event room registry, GP results | separate room registry from the multiplayer lobby; sticky mode flag | **yes** | `Match.iff` unparsed | L | Med |
| **Friends / messenger** | MessageService listener: 8 in, 2 out; plus `0x008b`→`0x00fc` on GameService | friends, blocks, presence, direct messages | presence is authoritative from live sessions | **yes** | no | M | **Low — the only subsystem with 852 captures** |
| **Statistics / profile** `0x002f`,`0x0006`,`0x0031` | out the 13-packet burst; `0x0045` | `course_records` exists; needs a per-account statistics projection | statistics derived from committed match results per §12.4, never from the submitted blob — the client's own `0x0006`/`0x0031` payloads are a *report*, not a source of truth | **yes** | no | L | Med |
| **Tutorial** `0x000b`?/`0x00ae` | out `0x004b` blob, `0x011f` | tutorial completion flags | completion server-recorded | small | no | S | **High — evidence contested** |
| **Personal shop / club workshop** | §2.19 | shop stock, club upgrade state | prices from the seller's declared stock, verified server-side; upgrade RNG server-side | **yes** | `Enchant.iff` unparsed | L | Med — numbering conflicts |

---

## 5. Traps

### 5.1 `0x000b` — tutorial start or equipment update?

- `pangbox--server/pangya/game/packet/client.go:35` maps `0x000B` to
  `ClientTutorialStart`, and `game/server/conn.go:612-650` answers it with a
  380-byte hardcoded `0x004b` blob.
- `packetdoc/.../client/index.ksy:129` maps it to
  `gameservice_client_000b_user_equipment_change`, and `client/000b.ksy` documents
  a `u1` equipment type with case `0x04 = equipped_character`, responding `0x004b`.
- `alter-pangya/.../ClientPacketType.kt:52` maps it to
  `EQUIPMENT_UPDATE_IN_LOBBY` and routes it through the *same handler* as
  `0x000c` with an `inGameRoom = false` flag.
- `K4T--Py_Source_US/.../PangyaEnums.cs` names it `PLAYER_SAVE_BAR` and dispatches
  it jointly with `PLAYER_CHANGE_EQUIPMENT` (`GPlayer.cs:408-409`).

Three sources against one, including both 852-targeting servers, say
**equipment**. Note that both readings answer `0x004b`, which is probably why the
confusion survived: upstream's blob happens to be a valid-looking `0x004b` and
the client does not visibly complain.

This remains **live-capture blocked** for the U.S. 852 client: no checked-in
trace proves the `0x000b` body or a tutorial interpretation. The real-client
Practice report only records that loading sends both `0x000c` and `0x000b`
(`docs/evidence/REAL_CLIENT_PRACTICE_2026-08-09.md:32-42`); it preserves no
body bytes, subtype, or matching `0x004b`. The active PacketDoc and multi-source
server evidence support the equipment handler only. Do not implement a
tutorial-start body until an immutable U.S. capture supplies those facts.

### 5.1.1 Tutorial implementation hold

Do not derive a U.S. 852 tutorial wire/reward contract from the checked-in K4T
source. Its enum declares `Advancer = 16128`
(`opensource-references/K4T--Py_Source_US/Src/Py_Game/Py_Game/Defines/PangyaEnums.cs:3-9`),
but its mission switch handles only Rookie/NewRookie and Beginner
(`opensource-references/K4T--Py_Source_US/Src/Py_Game/Py_Game/Functions/TutorialCoreSystem.cs:60-149`).
Its status writer has distinct 19-byte login and 6-byte completion branches
(`opensource-references/K4T--Py_Source_US/Src/Py_Game/Py_Game/GameTools/PacketCreator.cs:166-185`),
without a U.S. capture selecting one in each context. Its reward mapping is
source behavior, not a client reward catalog
(`opensource-references/K4T--Py_Source_US/Src/Py_Game/Py_Game/Functions/Mail/MailSender.cs:88-207`).

Before implementation, preserve a redacted U.S. 852 trace for login/bootstrap
`0x011f`, each Rookie/Beginner/Advancer `0x00ae` mission, and each post-clear
`0x011f`, including direction, body length, bytes, and order. It must be paired
with an authoritative per-mission and completion reward matrix. Until then,
do not add tutorial protocol bytes, reward tables, migrations, or tests that
encode one of these unsupported choices.

### 5.2 `0x0065` — time booster body conflict (deferred)

The vendored PacketDoc `gameservice/client/0065.ksy` defines a `u4 item_id` and pairs it
with `0x00c7` fields `(item_id, connection_id)`. The U.S.-targeting SuperSS-Dev
`GAME/versus_base.cpp:2069-2118` instead reads an `f32 velocidade`, validates a
server-side passive-item counter, and broadcasts `0x00c7` as `(f32, connection_id)`.
K4T Py_Source_US `GameBase.cs:979-989` consumes neither packet field and emits
`0x00c7` with `f32 3.0`. Without a live U.S. 852 capture these are irreconcilable
claims; the GameService therefore accepts `0x0065` inertly (no strike, response, or
economy mutation) rather than guessing a body or consuming an item. Authoritative
consumption/effect remains deferred to issue #26/live capture.

### 5.3 `0x0088` — three incompatible meanings

- PacketDoc `client/0088.ksy`: the response to server `0x00d7` authentication
  keep-alive challenge.
- pangbox `conn.go:491`: *"Unknown tutorial-related message."*
- Py_Source_US: `PLAYER_KEEPLIVE = 0xF4`, and `0x0088` is not in its enum at all.

This server currently accepts `0x0088` silently, which is safe **only because it
never sends `0x00d7`**. If a keep-alive challenge is ever added, the accept must
become a real handler. If the PacketDoc reading is wrong and it is genuinely a
tutorial message, the silent accept is hiding a subsystem.

### 5.4 Rank-up request is refusal-only

`0x0167` is the one club-workshop rank opcode shared by the conflicting 852
numbering tables, but the surrounding operation meanings disagree (see below).
It is therefore admitted as an explicit no-mutation refusal: no rank, inventory,
or Pang state changes and no unknown-opcode strike.

### 5.5 Club workshop opcode numbering disagrees between the two 852 sources

`alter-pangya:93-95` uses `0x0167`/`0x0168`/`0x0169` for rank-up / decline /
accept. `Py_Source_US:672-675` uses `0x0164` upgrade, `0x0165` accept, `0x0166`
cancel, `0x0167` rank-up. Both claim 852. At most one is right;
**neither should be implemented without client confirmation.**

### 5.4 PacketDoc constants that are explicitly Thai

Anywhere a `.ksy` names a number "in PangyaTH", treat it as a TH capture artifact:

- `server/01bc.ksy:3` — guild directory page size 15.
- `client/0003.ksy` — chat censoring marker `$A8` "(PangyaTH)".
- `server/010b.ksy` — rare shop `-1, -1, 0`, vs upstream's `50`, which upstream
  added specifically to fix a Big Papel client bug (`conn.go:495`).
- `server/0225.ksy` — quest ids sourced from `Quest.iff`, a filename the U.S.
  client does not ship (it has `QuestStuff` and `QuestItem` instead).

### 5.5 Client/server opcode-space collisions that read like contradictions

Client and server tables are separate namespaces, but the same number carries
unrelated meanings across them and a careless cross-reference will produce a wrong
handler. Known collisions touching this work: `0x00ec` (C: Aztec box / S: item
transact), `0x00e6`/`0x00e7` (C: rental renew/delete / S: user-shop inventory and
leave), `0x016c`–`0x016f` (C: club point transfer, clubset power, login-bonus
request, login-bonus claim / S: locker combination, locker page, locker deposit-B,
locker withdraw), `0x015c`/`0x015d` (C: ring effects / S: course records, guild),
`0x012e`–`0x0130` (C: tourney shot, match quit / S: custom asset, invite responses),
`0x0070`/`0x0071` (C: scratchy play, scratchy serial / S: character roster, caddie
roster).

### 5.6 Reward-path shape is TH-derived and pangbox's gacha is knowingly unsound

`conn.go:546-549` carries four TODOs on Big Papel alone: *"make Pang interaction
transactional"*, *"make items show up in inventory"*, *"make sure not to subtract
pang if a ticket is used"*, *"Fix pang able to go negative"*. It also hardcodes
−10000 Pang for Big Papel and −500 for Black Papel, and `conn.go:555` admits
*"Unknown if this is actually how the number of items are actually chosen."*
None of the costs, draw counts, or rarity rules in that file are evidence.
SPEC §14.3 forbids all four of those defects outright.

### 5.7 `0x0007` now answers the mandatory PacketDoc reply

`client/0007.ksy` defines discriminator `1` plus a username and requires server
opcode `0x00a1`; the implementation validates that body and emits the exact
one-byte `0x02` response. The previous accepted-silent gap is closed without
claiming any unproven gift/status semantics.

### 5.8 There is no `Achievement.iff`

The U.S. client's `pangya_gb.iff` ZIP holds 39 tables and none of them is an
achievement definition table
([`../data/US_CLIENT_IFF_STRUCTURE.md`](../data/US_CLIENT_IFF_STRUCTURE.md)).
Achievement ids in `0x022d` are 32-bit values grouped under a group id, and
upstream fabricates five of them. Where the client gets its achievement names and
thresholds is **unknown** — most likely a script or string table inside the PAK
series rather than the IFF catalog. This must be resolved before achievements can
be anything other than a well-formed empty response.

---

## 6. Prerequisites for SPEC §19.6 versus pure breadth

SPEC §19.6 has twelve steps. Steps 1–6 and 10–12 are already evidenced
([`../PROGRESS.md`](../PROGRESS.md)). Steps 7–12 as a set are blocked, and the
question is which of §2 they actually require.

**Hard prerequisites for §19.6:**

| Subsystem | Which step | Why |
|---|---|---|
| Match completion set (§2.3) | 7, 8, 9 | Blocker 23. `0x0013`,`0x0015`,`0x0016`,`0x0017`,`0x0019`,`0x0022`,`0x0048`,`0x0034` are what turn a solo hole into an arbitrated two-player hole. Without `0x0022`/`0x0034` there is no turn handoff at all |
| Room settings + equipment-in-room (`0x000a`,`0x000b`,`0x000c`) | 7 | A versus room requires capacity ≥ 2 (PROGRESS blocker 22); reaching a startable state means the host can edit the room and both players can change equipment inside it |
| Chat `0x0003` | 7–12, defensively | Not named by §19.6, but a human driving two clients through a room *will* type. Under `disconnect` policy that ends the test run. It is a two-hour fix protecting a multi-day validation |
| Statistics submit `0x0006`/`0x0031` | 9 | The client reports its own totals at hole and match end. Step 9 is "result/balance screen displays coherent values"; the server must at minimum accept these and answer `0x0045` |
| Live room census | 7 | Already on the PROGRESS next-step list. Two players cannot see each other in a room otherwise |

**Everything else in §2 is Tier C or Tier D breadth** under SPEC §3.1, and none
of it blocks a first playable release:

- Tier C (SPEC §3.1: "useful private server"): friends/messenger, mail,
  rankings/statistics depth, shop/equipment IFF validation, course records.
- Tier D ("broad legacy feature parity", named explicitly in §3.1): guilds,
  personal shop, locker, cards, caddies, mascots, MyRoom furniture/UCC, quests,
  achievements, daily/login rewards, memorial/papel/scratch, rentals, events,
  drop/treasure.

Two nuances the tier list does not capture:

1. **Caddies and mascots are cheaper than their tier suggests.** Caddie equip
   already works; the roster is an empty container away from being real, and
   mascots are one container plus two already-decoded `0x0020` slots. They are
   Tier D by name and Tier B by cost.
2. **Achievements are load-bearing for Tier D, not optional within it.** Every
   gacha, quest, and mastery response chain ends in `0x022e`+`0x0220`. A
   well-formed no-op pair is a prerequisite for all of §2.4, §2.5, and §2.8.

---

## 7. Recommended order

Ordered by unblocking value first, then by evidence quality, then by cost.
Migration and catalog columns are the ones that make an item expensive to
retrofit later.

| # | Work | Why here | Migration | New IFF parsing |
|---|---|---|---|---|
| 1 | **Chat `0x0003` → `0x0040`** | One packet each way, zero state, and it stops a working session from dying the first time a human types. Cheapest risk reduction in the document | no | no |
| 2 | **Match completion set (§2.3) + live room census** | Blocker 23; the only thing between here and §19.6 steps 7–12. Everything else is optional until this lands | maybe | no |
| 3 | **Room settings `0x000a` + equipment-in-room `0x000c`; treat `0x000b` as equipment** | Required to reach a startable two-player room. Fold in the `0x0020` slot 8/9 renaming (`UnknownEight`→mascot, `UnknownNine`→cut-in) since it is documentation-only | no | no |
| 4 | **Statistics submit `0x0006`/`0x0031` → `0x0045`; `0x002f` burst** | §19.6 step 9. Accept and answer, deriving nothing from the client's numbers per §12.4. `0x002f` also fixes the "click another player" path | yes | no |
| 5 | **`0x0216` User Status Update as a first-class primitive** | The single reusable dependency of §2.4, §2.5, §2.8, §2.10, §2.14. Build it against the existing `item_ledger`/`economy_operations` tables before any consumer. Pair it with no-op-but-well-formed `0x022e`/`0x0220` | no (reuses M7) | no |
| 6 | **Caddie + mascot rosters (`0x0071`, `0x00e1`)** | Disproportionately cheap given equip already works. Needs `Caddie.iff`/`Mascot.iff` parsing, which is the same loader path `Character`/`Ball` already use | small | **yes** |
| 7 | **Locker (`0x00cd`–`0x00d5`)** | Two of its five opcodes already answer. Its Pang transfer is a clean §14.3 exercise on an existing wallet, with no catalog dependency and no RNG | yes | no |
| 8 | **Mail (`0x0143`–`0x0147`, `0x0210`)** | SPEC §22.6 Tier C, well documented, no RNG, and the attachment path is a §14.5-shaped transactional item move — good preparation for gacha grants | yes | no |
| 9 | **MessageService + `0x008b`→`0x00fc`** | The only subsystem in this document with **GB.R7.852.00 captures**, and the smallest complete packet table (8 in, 2 out). Best evidence-to-effort ratio of anything Tier C | yes | no |
| 10 | **Login bonus (`0x016e` uncollected branch + `0x016f`)** | Small, self-contained, and the current safe stub is one flag away from correct. Ship both halves in one change | yes | maybe |
| 11 | **Rare shop + Papel + scratchy + memorial + lootbox** | Now cheap, because item 5 exists. Server-defined prize pools (the client ships no Papel table and an *empty* `ScratchRewardSetting`), seeded RNG per §12.6, one transaction per draw per §14.3. Do **not** port upstream's costs or draw counts — trap 5.6 | yes | yes (`SpecialPrizeItem`) |
| 12 | **Quests (`0x0151`–`0x0154`)** | Needs item 5 and `QuestStuff`/`QuestItem` parsing. Progress must be derived from committed match results, so it is naturally downstream of item 2 | yes | yes |
| 13 | **Achievements (`0x0157`, `0x021d`, `0x021e`)** | Deliberately after quests: trap 5.8 must be resolved first, and until it is, the correct implementation is the well-formed empty pair from item 5 | yes | **unresolved** |
| 14 | **Cards + character mastery** | Needs item 5, `Card.iff`, and per-character mastery columns. The bootstrap container `0x0138` and the 12 card slots per character mean partial implementations are visible at login | yes | yes |
| 15 | **Events / Grand Prix (`0x0176`,`0x0177`,`0x0179`,`0x017a`)** | Needs a second room registry and the sticky mode flag; only worth doing once item 2's match lifecycle generalizes beyond one hole | yes | yes (`Match.iff`) |
| 16 | **Personal shop, club workshop, rentals, guilds, MyRoom furniture/UCC** | Deferred on evidence, not on effort. Club workshop numbering conflicts between the two 852 sources (trap 5.3); rentals have a single GPL source; furniture placement opcodes are undocumented; UCC needs an HTTP upload surface; SPEC §22.6 defers guilds explicitly | yes | yes |
| — | **Rings (`0x015d`)** | Unplaceable. Two 852 sources agree the opcode exists; none documents the body. Needs client observation before it can be scoped | ? | ? |

### Two changes worth making regardless of order

**Reconsider the shipped `unknown_opcode_policy` for retail runs.** With
`disconnect`, `unknown_opcode_strikes = 3` is dead configuration
(`lib.rs:5541-5546` ignores the strike count on that branch), and every gap above
is a hard session kill during exactly the manual driving that §19.6 requires.
`capture` records a bounded metadata digest and only disconnects at the strike
limit — which is what `crates/pangya-game/src/lib.rs:970`'s
`unknown_opcode_captures` exists for, and it would turn each of these gaps into
evidence instead of a lost session. This is an operator-facing default change and
belongs to whoever owns `config/retail-local.example.toml`, not to this document.

**Correct the `0x0007` allowlist comment** (trap 5.7). The row is probably still
fine to accept silently, but the stated justification does not match
`client/0007.ksy`, and §27 requires residual unknowns be recorded rather than
hidden.
