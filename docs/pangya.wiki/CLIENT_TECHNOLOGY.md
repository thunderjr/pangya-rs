# Client technology and implementation clues

## Scope and method

All **437 captured pages** were screened for file-format, executable, patcher, client, server, launcher, UI, hit-bar, resolution, rendering, connectivity, and crash clues. Relevant records and directly supporting gameplay pages were then read in full. This is corpus analysis, not reverse-engineering or independent verification.

**Confidence labels:** **High** = concrete syntax/value stated by the corpus; **Medium** = clear behavioral claim but no primary artifact/specification; **Low** = inference, stub-only evidence, or terminology explicitly described as uncertain.

## 1. Fresh UI: declarative client UI schema

Source: [Fresh (User Interface)](https://pangya.wiki/wiki/Fresh_(User_Interface)).

### Document and object model

**High confidence:** Fresh files are XML, shown with `encoding="euc-kr"`, and form a hierarchy:

```text
resource (document root)
└─ element (named definition: FRAME, FORM, INCLUDE, LAYOUT, MACROITEM)
   └─ item (widget: AREA, BUTTON, EDIT, etc.)
      └─ param name="..." var="..."
```

Concrete top-level example:

```xml
<?xml version="1.0" encoding="euc-kr" standalone="yes" ?>
<resource count="0">
  <element type="FORM" name="example" resource="DLGFRM">
    <item type="AREA" flag="0" rect="0 0 200 200">
      <param name="bgimg" var="example_img" />
    </item>
  </element>
</resource>
```

The `resource/@count` cannot be treated as authoritative: older files reportedly used the actual element count, while later files use `0` or an incorrect value. A parser should enumerate children and treat `count` as advisory.

Coordinates are represented in two distinct ways:

- `pos="X Y"`: top-left only; size may come from state imagery.
- `rect="L T R B"`: bounds, explicitly stated as left/top/right/bottom.

`param` is an open name/value property bag. Values are serialized strings even when semantically Boolean, integer, float, dimensions, filename, or ARGB/RGB color. Examples mix `0`/`1` and `true`/`false`, so a consumer needs tolerant coercion.

### Element schemas

| Element type | Concrete fields / children | Meaning and caveats |
|---|---|---|
| `FRAME` | `name`, `color`; children `layer(type,height,pos)`, `sfrm(filename)`, `bfrm(filename)`, `cfrm(filename)` | Defines a reusable tiled frame resource; it does not render alone. Only `bfrm` is confirmed by the article as used by forms; the other child semantics remain undocumented. |
| `FORM` | `name`, `size="W H"`, `resource` | Dialog. `name` is the internal lookup identity; `resource` references a `FRAME`. |
| `INCLUDE` | `name="frames.xml"` | Includes another file. Resolution rules, cycle handling, and relative base directory are not stated. |
| `LAYOUT` | `name`; child `base(width,height,color,background)` | Full screen/scene such as login, lobby, or room list. `color` is an unprefixed RGB hex triplet. Example base size is 800×600. |
| `MACROITEM` | `name`; nested items | Reusable UI region, particularly gameplay/shop top or bottom bars. |

### Frame asset algorithm and filenames

**High confidence as described, with one indexing caveat:** a frame filename is a base name expanded to numbered TGA assets, normally `00` through `09`, e.g. `main_bfrm00.tga`, `main_bfrm01.tga`, …. The article describes a classic 9-tile layout and says `01=top-left`, `02=top-center`, `03=top-right`, continuing in row order; center and edge images stretch to the requested dialog size.

**Caveat:** “00 through 09” implies ten possible files while a 9-slice has nine tiles, and the article only maps `01` onward. The role of `00` is not specified. Do not hard-code its role without examining assets/client behavior.

An item-level `FRAME` can override the referenced frame's `bfrm` with `param bgimg`; that override is still a numbered prefix (`sfrm00`, `sfrm01`, …), not a single image.

### Item/widget schemas and UI state

| Item type | Fields and state required by a renderer/controller |
|---|---|
| `AREA` | `flag?`, `name?`, `rect`; `bgimg`, `center`, `stretch`, `visible`. `flag` is unknown. |
| `BUTTON` | `name`, either `pos` or `rect`; images `normal`, `over`, `selected`, optional `blink`; `pushstyle`, `pushdelay`, `style`, `drawstyle`, `event` are present but semantics unknown. Requires normal/hover/selected/blink state plus blink timing. |
| `COMBOBOX` | `rect`; most `EDIT` properties plus `maxlistnum`, `selectcolor`, `show_scrollbar`, arrow images `normal`/`selected`/`over`. Requires selected row, open state, list rows, scroll state, and hover state. |
| `COMBOCTLEX` | `rect`; `normal`, `over`, `selected`, `drawstyle`, `liststyle`, `style`, `maxlistnum`, `align` (`0` left, `1` center, `2` right), `topmargin`, `fontcolor`, `fontcolor2`. Image-backed dropdown. |
| item `FRAME` | `name`, `resource`, `flag?`, `rect`; optional `bgimg` numbered-prefix override. |
| `GROUPBOX` | `name`, `rect`; nested items. Its rectangle establishes a new coordinate origin for children. |
| `STATIC` | `caption`, `pos`; `color`, `color2`, `style`, `align`, `scrolling`. Requires text/localization value and possibly scroll/animation state; `style` and `scrolling` behavior are unknown. |
| `EDIT` | `rect`; alignment (`align`, `valign`, `vcenteralign`), `symmetry`, `font`, colors (`fontcolor`, `fontcolor2`, `bgcolor`, `bordercolor`, `caretcolor`, `caretcolor2`), layout (`leftmargin`, `topmargin`, `lineheight`, `widthlimit`), constraints (`readonly`, `charlimit`, `multiline`, `password`), `caretmove`. Requires content, focus, selection/caret, masked/plain rendering, validation/limits, and scroll state for multiline. `font`, `symmetry`, `caretmove`, and exact `valign` domain are unknown. |
| `GAUGEBAR` | `rect`; `bgcolor`, `color`, `excolor2`, `border_thickness`. Requires current value and “extra section” value, neither binding nor numeric range is encoded in the shown XML. |
| `GAUGEBAREX` | `rect`; colors; left/right visibility and images (`left`, `normal`, `right`, `rnormal`); `steps`. Described as a range input; requires value, range, step, and button state, though min/max are not shown. |
| `GAUGEBARIMAGE` | `rect`; `normal`, `mask`. Requires current fill value and masked image composition. |
| `LISTBOX` | `rect`; `column`, `itemsize`, `use_key`, `show_scrollbar`, `lineheight`, `maxlistnum`, `normal`, `topmargin`. Repeated-row rendering is “probably hardcoded ingame,” so XML alone is insufficient; requires data rows, selection/focus, keyboard and scroll state. |
| item `MACROITEM` | `name`, `pos`, `resource`; instantiates a named macro definition. |
| `TEXTBUTTON` | `caption`, `rect`; `style`, `color`, `center`. Requires click/hover state even though state imagery is not documented. |
| `TABBUTTON` | `name`, `rect`; `sepImg`, `below_over`, `below_selected`. Requires active tab and hovered tab. |
| `VIEWER` | `name`, `rect`; `bgcolor`, `use_wheel`, `image`, `left`. Presenter for static or game-supplied values; binding contract and `left`/wheel semantics are unknown. |

### Architecture implications

- **Medium:** XML defines structure and visual resources, but behavior/data binding is partly or wholly hardcoded: form names are called internally, button `event` semantics are absent, list rendering is probably hardcoded, and viewers accept game-supplied values.
- **Medium:** asset lookup is convention-based (base names plus `.tga` numbering) rather than explicit per-tile paths.
- **Low:** “Fresh” itself is not publicly documented. The article infers the name from debug-file source paths and the later in-code name “Refresh.” It also says Fresh was only partially superseded by Refresh in [Pangya Fresh Up](https://pangya.wiki/wiki/Pangya_Fresh_Up); the Fresh Up corpus article contains no implementation details beyond being the final season.

## 2. DAT localization format and lookup algorithm

Source: [DAT](https://pangya.wiki/wiki/DAT).

### Binary schema

**High confidence:** a DAT is only a sequence of null-terminated strings from offset zero to end-of-file—no header, count, offsets, IDs, lengths, or per-entry metadata.

```text
DAT := StringZ[until EOF]
StringZ := encoded bytes followed by 0x00
```

Equivalent corpus definitions:

```cpp
std::string::NullString locale[while(!std::mem::eof())] @ 0x00;
```

```yaml
meta:
  id: dat
  file-extension: dat
  endian: le
seq:
  - id: locale
    type: strz
    encoding: euc-kr  # replace for target file
    repeat: eos
```

Endianness has no practical effect on single-byte null termination, but appears in the supplied Kaitai schema.

### Localization algorithm and invariants

1. At least two `.dat` files are present.
2. `korea.dat` is always the source/origin catalog because code/assets contain Korean strings.
3. Find the exact Korean source string's positional index in `korea.dat`.
4. Read the same index in the configured target catalog, e.g. `japan.dat`.
5. Decode each catalog with the matching regional encoding; EUC-KR is an example/source encoding, not a universal setting.

This is index-aligned parallel-array localization, only described as following gettext conceptually; it is **not stated to be GNU MO/PO format**. Correctness requires identical entry ordering and compatible entry counts. Missing, duplicated, or reordered Korean source strings are ambiguous or shift translations. There is no stated fallback, plural/context support, key namespace, checksum, or bounds behavior.

The article says most regions support one target language; Europe was a notable four-language target exception (English/German/French/Spanish). It does not name those European DAT filenames or explain runtime language selection.

## 3. QuickPatch content/update pipeline

Source: [QuickPatch](https://pangya.wiki/wiki/QuickPatch).

### Pipeline facts

**High confidence as UI/tool behavior stated by the corpus:** QuickPatch was an Ntreev patch-authoring/uploader, presumably also provided to regional publishers.

A likely operational sequence directly supported by the fields and descriptions is:

1. Select a predefined server or enter connection fields manually.
2. Choose full vs incremental mode: **Whole sync** versus **Additional upload**.
3. Set human patch version (example `KR.Q4.548.00`) and numeric patch number (example `306`).
4. Build an update list whose displayed format version is fixed/disabled at `20090331`.
5. Exclude denylisted development/debug files.
6. Encrypt the update listing with an XTEA key selected by region.
7. Upload patch artifacts over FTP to the configured remote directory.
8. Move previous patched files to a backup directory; if current source files are absent, fall back to backup copies. Default backup path: `C:\_QuickPatchBackUpFile`; default retention: one month.

Steps 4–7 are a synthesis of explicit features, not a fully documented internal execution order. Compression, hashing, diff generation, update-list filename/schema, XTEA mode/padding/endianness, and client application/rollback logic are absent.

### Server configuration schema

```text
ServerProfile {
  SERVER NAME: string
  URL: host-or-IP
  REMOTE DIR: path
  ACCOUNT: FTP username
  PASSWORD: FTP password
  PORT: integer = 21
  TEA KEY: enum {KR, JP, TH, EU, GB, U1, U2, U3, U4}
  WHOLE SYNC: boolean
  PASSIVE: unknown/likely FTP passive-mode boolean
}
```

Only predefined profiles can set the TEA key; manual connection entry does not expose every option. `PASSIVE` is explicitly unexplained by the article, so interpreting it as FTP passive mode is a low-confidence inference.

Settings can be imported/exported as `.reg` files and default to registry key:

```text
HKEY_LOCAL_MACHINE\Software\Ntreev\QUICKPATCH
```

### Deployment topology evidence

The leaked profile list shows separate QA, release, patch-test, and update-test destinations, plus region/publisher-specific FTP roots. Examples include Korean QA `/season4/patch/qa/`, Korean release `/season4/patch/release/`, internal `/client/patchtest/` and `/client/updatetest/`, Thailand production `/client_patch_S4/`, Europe `/patch-prod/`, Japan production `/patch/pangya_client/`, and corresponding test locations. This supports environment separation and region-specific encrypted update lists. It does **not** prove the game servers and patch FTP servers were the same systems.

The corpus contains historical IPs, usernames, and redacted passwords. They are evidence of profile shape, not safe/current endpoints; this report intentionally does not reproduce every credential-bearing profile.

### Exclusion list and concrete filenames

The default denylist is:

```text
QuickPatch.ini
QuickPatch_Eu.exe
QuickPatch_lv1.exe
QuickPatchExt_lv1.exe
DataSafe.exe
WestPak.exe
ClassED.exe
MakeFont.exe
PakOut.exe
PuppetMasterG.exe
WangED.exe
FxBox.exe
PangYa.iff
ProjectG.pdb
wangreal.pdb
LoadingRes.pdb
```

This reveals:

- multiple QuickPatch variants/levels;
- eight named internal-tool executables;
- `PangYa.iff`, an otherwise undocumented file in this corpus;
- three Windows program database/debug-symbol files, including one corresponding to the client executable family (`ProjectG`);
- a denylist-based release hygiene process.

The tool itself leaked because a patched variant apparently used a different filename not present in the denylist. **Medium-confidence process finding:** filename denylisting is brittle and fails closed only for known names; a release pipeline should positively select distributable artifacts or classify by location/type/signature instead.

## 4. Internal development tools

Sources: [ClassED](https://pangya.wiki/wiki/ClassED), [DataSafe](https://pangya.wiki/wiki/DataSafe), [FxBox](https://pangya.wiki/wiki/FxBox), [MakeFont](https://pangya.wiki/wiki/MakeFont), [PakOut](https://pangya.wiki/wiki/PakOut), [PuppetMasterG](https://pangya.wiki/wiki/PuppetMasterG), [WangED](https://pangya.wiki/wiki/WangED), and [WestPak](https://pangya.wiki/wiki/WestPak).

All eight articles are stubs. Their only concrete evidence is that the matching `.exe` appears in QuickPatch's default exclusion list (PuppetMasterG has the same content with headings placed after the category tag). No functionality, command-line interface, accepted format, algorithm, ownership, or tool-to-tool relationship is documented.

**Do not infer function from names.** `MakeFont`, `PakOut`, `WestPak`, `ClassED`, `WangED`, `FxBox`, `DataSafe`, and `PuppetMasterG` may suggest font generation, package work, editors, effects, or data protection, but the corpus does not attest any of those roles. The only defensible findings are their executable filenames, internal/distribution-excluded status, and association with the leaked QuickPatch environment.

## 5. Hit bar: client state and input algorithm

Primary sources: [Hit bar](https://pangya.wiki/wiki/Hit_bar), [Accuracy](https://pangya.wiki/wiki/Accuracy), [Control](https://pangya.wiki/wiki/Control), [Impact Zone](https://pangya.wiki/wiki/Impact_Zone), [Bad Shot](https://pangya.wiki/wiki/Bad_Shot), and [Power Shot](https://pangya.wiki/wiki/Power_Shot).

### Required visible state

The hit bar is a bottom-of-screen active-play control with:

- moving gray **toggle bar**;
- hit-zone bounds: white **Impact Zone**, pink **Accuracy Zone**, and outside **Bad Zone**;
- selected power and green numeric **power indicator**;
- calipers/Auto Caliper status;
- current club selection;
- current Comet condition;
- active-item selection.

Later UI behavior adds exact selected power with decimal precision ([Pangya United](https://pangya.wiki/wiki/Pangya_United)), user-adjustable hit-bar position and size ([GB.R5.701.00](https://pangya.wiki/wiki/GB.R5.701.00)), and a normal-size adjustment for larger resolutions ([GB.R5.702.00](https://pangya.wiki/wiki/GB.R5.702.00)). [GB.R5.614.01](https://pangya.wiki/wiki/GB.R5.614.01) documents a `New Hit Bar` setting suggested on older machines experiencing latency, alongside a graphics-engine upgrade.

A client implementation therefore needs at least:

```text
HitBarState {
  phase: outbound | return | resolved
  togglePosition: continuous/pixel position
  selectedPower: decimal
  club: selected club and max distance
  zones: { impactBounds, accuracyBounds, badBounds }
  controlStatEffective: capped value
  accuracyStatEffective: capped value
  powerShotMode: none | single | double
  commandBuffer: ordered directional inputs with timing/window
  impactPoint: centered/left/right and spin/curve state
  caliperMode: none | power-calipers | auto-caliper
  cometCondition
  activeItem
  ui: { position, size, resolutionMode/newHitBar }
}
```

This schema is an implementation-oriented synthesis, not a serialized format from the corpus.

### Timing and zone rules

- Control determines toggle speed: higher Control means slower motion; effective Control caps at 30. Bunker, Sand, Water, and Ice significantly increase speed; putting is unaffected by Control.
- Accuracy determines pink-zone size, caps at 30, and can be penalized when Power is too high. Extreme slope can shift the pink zone left or right depending on stance. Single/double Power Shot temporarily shrinks it.
- White Impact Zone defaults to **4 pixels** and can be enlarged conditionally by items/modes.
- On the return/leftward pass, landing outside pink is a Bad Shot: subtract **17 Combo Gauge units**, award no Pang, but still increment stroke count.
- White produces a straight/Pangya result; pink permits hook/slice and can still activate several special shots, though [Power Curve](https://pangya.wiki/wiki/Power_Curve) specifically says to land on the white Impact Zone.

The corpus does not give pixel-to-power mapping, motion curve, frame/tick timing, input tolerance, exact Accuracy/Control formulas, high-Power penalty formula, or authoritative client/server ownership of shot validation.

### Special-shot recognition state machine

Supporting sources: [Cobra](https://pangya.wiki/wiki/Cobra), [Spike](https://pangya.wiki/wiki/Spike), [Tomahawk](https://pangya.wiki/wiki/Tomahawk), and [Power Curve](https://pangya.wiki/wiki/Power_Curve).

Common recognizer flow:

1. Validate club/mode preconditions.
2. On outbound/rightward movement, commit power at **≥80%**.
3. During the return/leftward movement, after halfway but before the start, capture an ordered command.
4. Resolve when the toggle lands in the permitted zone.

Commands and preconditions:

| Shot | Mode / club | Command in return window | Required final zone |
|---|---|---|---|
| Cobra | Power Shot; Wood only | Right, then Up | white or pink |
| Spike | Power Shot; Wood only | Right, then Down | white or pink |
| Tomahawk | Power Shot; Wood/Iron/Wedge | Up, then Down | white or pink |
| Power Curve | normal-capable Wood/Iron/Wedge; impact point fully left/right | hold matching Left/Right | article says white Impact Zone |

Single Power Shot requires one full Combo Gauge segment and one `Alt` press; Double requires two segments and two `Alt` presses. They add 10 and 20 yards respectively (putter excluded), with a Power Potion exception of +15 yards for a double-like stance. Item activation can bypass gauge use. Power variants use the same directional recognizers from the Double stance.

The broader Season 4 behavior permits special-shot execution without Pangya but makes trajectory erratic ([Pangya Season 4: Delight](https://pangya.wiki/wiki/Pangya_Season_4:_Delight)). Tomahawk/Spike pages say failure randomizes Spin/Curve/direction. [GB.R3.433.02](https://pangya.wiki/wiki/GB.R3.433.02) records a power-bar glitch tied to caddie skins, evidence that equipped cosmetics could affect or interfere with bar state/rendering.

### Other concrete state clues

[Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac) shows mode-level overrides that the UI/gameplay state must support: Impact Zone `+1 pixel`, permanently full Power Gauge, infinite Auto Caliper, and preserving exact position/wind conditions after a miss. Advanced-mode success preserves only Power Shot stance, whereas Intermediate preserves spin/curve, Power Shot stance, and last club. This indicates explicit reset/persistence policies per mode/event, though not their storage location.

## 6. Additional client/server and delivery clues

These are behavioral clues, not enough to reconstruct protocols or server architecture.

- [GB.R3.432.01](https://pangya.wiki/wiki/GB.R3.432.01): identifies `Projectg.exe` as the client application; records client non-response after Start, an application error, and server-list population failure after login. Combined with `ProjectG.pdb` in the patch denylist, this links the shipped executable and internal debug symbols. It does not identify the crash cause or server-list protocol.
- [GB.R3.433.01](https://pangya.wiki/wiki/GB.R3.433.01): the client could initiate Points recharge through “Buy Points”; server objects have channel counts, access policy/audience, EXP modifiers, and capacity. The transport/payment integration is unspecified.
- [GB.R4.500.04](https://pangya.wiki/wiki/GB.R4.500.04), [GB.R5.616.01](https://pangya.wiki/wiki/GB.R5.616.01), and [GB.R7.838.02](https://pangya.wiki/wiki/GB.R7.838.02): server names, availability policy, and merging changed through operations/content updates. These imply server-list data is configurable, but not whether it is remote or patched locally.
- [GB.R4.549.01](https://pangya.wiki/wiki/GB.R4.549.01): server-list display depends on where the player is located. This implies location/region-sensitive selection or presentation; the corpus does not say whether detection is client-side, server-side, IP-based, or account-based.
- [GB.R4.525.01](https://pangya.wiki/wiki/GB.R4.525.01): Launcher and GameGuard were updated; launcher hardening could increase loading/patch time. No security mechanism is named.
- [GB.R4.506.02](https://pangya.wiki/wiki/GB.R4.506.02): a specific equipped set could crash the client, evidence of content-dependent runtime paths but no implementation details.
- [GB.R4.501.01](https://pangya.wiki/wiki/GB.R4.501.01), [GB.R4.502.01](https://pangya.wiki/wiki/GB.R4.502.01), and [GB.R4.503.01](https://pangya.wiki/wiki/GB.R4.503.01): repeated UI graphic/localization corrections and messenger/server stability changes show graphics, localized labels, and online subsystems shipped together in routine client patches. The statements are too generic for schemas.
- [GB.R4.528.01](https://pangya.wiki/wiki/GB.R4.528.01): connectivity incidents were represented in persistent quit counts that operators could later reduce. It does not establish whether disconnect adjudication is client- or server-owned.
- [GB.R7.813.00](https://pangya.wiki/wiki/GB.R7.813.00), [GB.R7.818.00](https://pangya.wiki/wiki/GB.R7.818.00), and [GB.R7.819.00](https://pangya.wiki/wiki/GB.R7.819.00): Fresh Up introduced a new interface and later patches say only “Updated various UI”; no Refresh schema is present.
- [Pangya Season 4: Delight](https://pangya.wiki/wiki/Pangya_Season_4:_Delight): Gacha used an external interface accessed in-game; Ghost Mode records an 18-hole player's data for later playback; replays could be saved from a shot with `R` and viewed in My Room, up to ten. No URL, IPC/browser embedding mechanism, replay/ghost filename, serialization, or trust model is given.
- [Pangya United](https://pangya.wiki/wiki/Pangya_United): “connected” international Pangya servers to align update timing. This supports coordinated release operations, not necessarily shared live gameplay infrastructure or cross-region sessions.

## 7. Risks, unknowns, and implementation cautions

1. **High severity — format completeness:** Fresh is a research-in-progress article. Unknown `flag`, style/event/draw properties, include resolution, data binding, frame tile `00`, type coercion, and unlisted widget types prevent a fully compatible renderer from this corpus alone.
2. **High severity — localization integrity:** DAT translations are positional. Any insertion/deletion/reordering or wrong decoder can silently map every later string incorrectly. Entry-count/order validation against `korea.dat` is essential.
3. **High severity — patch compatibility/security:** QuickPatch does not document update-list records, integrity hashes/signatures, artifact naming, compression/deltas, or XTEA parameters. XTEA encryption alone must not be assumed to authenticate updates. Historical endpoints/configuration should not be reused.
4. **Medium severity — release leakage:** negative filename denylisting demonstrably missed a renamed QuickPatch binary. Exact-filename exclusion is insufficient release hygiene.
5. **Medium severity — hit-bar determinism:** the corpus supplies user-visible rules but not formulas/timing or validation ownership. A reimplementation could look correct yet diverge in speed, zone width, special-shot acceptance, or network reconciliation.
6. **Medium severity — source provenance:** most technical claims are wiki research based on leaked/debug artifacts and patch notes, not vendor specifications. “Fresh” is explicitly an inferred name; QuickPatch's publisher distribution is “presumably”; all internal-tool roles are unknown.
7. **Low severity — corpus metadata:** the DAT and Fresh records carry 2026 revision timestamps in the supplied corpus. This report treats corpus content as the requested source but cannot independently attest those timestamps or later edits.

## 8. All analyzed titles

Deep/core articles: **Fresh (User Interface); DAT; QuickPatch; ClassED; DataSafe; FxBox; MakeFont; PakOut; PuppetMasterG; WangED; WestPak; Hit bar; Accuracy; Control; Impact Zone; Bad Shot; Power Shot; Power Curve; Cobra; Spike; Tomahawk; Terrain Types; Grand Zodiac; Pangya United; Pangya Season 4: Delight; Pangya Fresh Up.**

Patch/client/server clue articles: **GB.R3.432.01; GB.R3.433.01; GB.R3.433.02; GB.R4.500.04; GB.R4.501.01; GB.R4.502.01; GB.R4.503.01; GB.R4.506.02; GB.R4.525.01; GB.R4.528.01; GB.R4.549.01; GB.R5.614.01; GB.R5.616.01; GB.R5.701.00; GB.R5.702.00; GB.R7.813.00; GB.R7.818.00; GB.R7.819.00; GB.R7.838.02.**

The remaining corpus records were keyword/category screened; ordinary gameplay/content articles with incidental wiki media filenames or nontechnical uses of “server,” “file,” or “interface” were not misclassified as implementation articles.

## Review findings

- **blocker:** none for delivering this corpus analysis.
- **high — Fresh UI:** incomplete/unknown semantics mean the article is not a complete implementable Fresh specification.
- **high — DAT:** positional localization has a catastrophic misalignment risk unless catalogs are validated.
- **high — QuickPatch:** update-list schema, integrity/authentication, and XTEA parameters are absent, so a compatible or secure updater cannot be built solely from this evidence.
- **medium — internal-tool stubs:** executable existence is attested, functionality is not.
- **medium — hit-bar sources:** visible state/rules are concrete, but numerical formulas, timing tolerances, and client/server authority are unresolved.
