# Gameplay and modes

## Scope, provenance, and auditable coverage

Source reviewed: the captured pangya.wiki corpus (revision timestamps span 2024-03-28 through 2026-05-24). This is secondary, community-authored evidence, not executable or official server documentation. “Exact” below means exact as asserted by these pages; unknowns and contradictions are not silently filled in.

I analyzed all 49 corpus articles categorized as **Game Mechanics, Stats, Scoring, Glossary, Special Shots, Game Modes, Multiplay Modes, Single Player Modes, or Tournament**, plus the gameplay article **Bad Shot** (`Shot Types`) and the case-variant redirect **Experience points**: **51 titles total**.

**Analyzed titles (51):**

1. [+4](https://pangya.wiki/wiki/%2B4)
2. [+5](https://pangya.wiki/wiki/%2B5)
3. [Albatross](https://pangya.wiki/wiki/Albatross)
4. [Accuracy](https://pangya.wiki/wiki/Accuracy)
5. [Approach Battle](https://pangya.wiki/wiki/Approach_Battle)
6. [Bad Shot](https://pangya.wiki/wiki/Bad_Shot)
7. [Birdie](https://pangya.wiki/wiki/Birdie)
8. [Beam Impact](https://pangya.wiki/wiki/Beam_Impact)
9. [Bogey](https://pangya.wiki/wiki/Bogey)
10. [Cobra](https://pangya.wiki/wiki/Cobra)
11. [Chip In](https://pangya.wiki/wiki/Chip_In)
12. [Chip-In Practice Mode](https://pangya.wiki/wiki/Chip-In_Practice_Mode)
13. [Control](https://pangya.wiki/wiki/Control)
14. [Curve](https://pangya.wiki/wiki/Curve)
15. [Course Practice](https://pangya.wiki/wiki/Course_Practice)
16. [Double Bogey](https://pangya.wiki/wiki/Double_Bogey)
17. [Drive](https://pangya.wiki/wiki/Drive)
18. [Eagle](https://pangya.wiki/wiki/Eagle)
19. [Experience Points](https://pangya.wiki/wiki/Experience_Points)
20. [Experience points](https://pangya.wiki/wiki/Experience_points) (redirect to preceding title)
21. [Give Up](https://pangya.wiki/wiki/Give_Up)
22. [GM Event](https://pangya.wiki/wiki/GM_Event)
23. [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac)
24. [Guild Battle](https://pangya.wiki/wiki/Guild_Battle)
25. [Hit bar](https://pangya.wiki/wiki/Hit_bar)
26. [Hole In One](https://pangya.wiki/wiki/Hole_In_One)
27. [Hole Order](https://pangya.wiki/wiki/Hole_Order)
28. [Hole Out](https://pangya.wiki/wiki/Hole_Out)
29. [Hole Repeat](https://pangya.wiki/wiki/Hole_Repeat)
30. [Impact Zone](https://pangya.wiki/wiki/Impact_Zone)
31. [Long Putt](https://pangya.wiki/wiki/Long_Putt)
32. [Lounge](https://pangya.wiki/wiki/Lounge)
33. [Over Drive](https://pangya.wiki/wiki/Over_Drive)
34. [Over Par](https://pangya.wiki/wiki/Over_Par)
35. [Pang](https://pangya.wiki/wiki/Pang)
36. [Pang Battle](https://pangya.wiki/wiki/Pang_Battle)
37. [Par](https://pangya.wiki/wiki/Par)
38. [Power](https://pangya.wiki/wiki/Power)
39. [Power Curve](https://pangya.wiki/wiki/Power_Curve)
40. [Power Shot](https://pangya.wiki/wiki/Power_Shot)
41. [Short Game](https://pangya.wiki/wiki/Short_Game)
42. [Special Shuffle Course](https://pangya.wiki/wiki/Special_Shuffle_Course)
43. [Spike](https://pangya.wiki/wiki/Spike)
44. [Spin](https://pangya.wiki/wiki/Spin)
45. [Team Tournament](https://pangya.wiki/wiki/Team_Tournament)
46. [Terrain Types](https://pangya.wiki/wiki/Terrain_Types)
47. [Tomahawk](https://pangya.wiki/wiki/Tomahawk)
48. [Tourney](https://pangya.wiki/wiki/Tourney)
49. [Triple Bogey](https://pangya.wiki/wiki/Triple_Bogey)
50. [Tutorial](https://pangya.wiki/wiki/Tutorial)
51. [Versus](https://pangya.wiki/wiki/Versus)

Course and walkthrough articles were not included merely because they describe playable geography; Grand Zodiac was included because it is explicitly also a Game Mode. Patch notes were not treated as gameplay-focused articles.

## 1. Core shot and hole state machine

### Hit input and shot resolution

The hit bar contains a moving toggle, selected power, club/item state, and three return zones. The **Impact Zone** (white, default width **4 pixels**) yields a straight “Pangya” shot; the pink Accuracy Zone permits a non-Bad slice/hook; outside pink is a **Bad Shot**. A Bad Shot still increments strokes by 1, earns no Pang for that shot, and removes **17 Combo Gauge units**. Terrain may change effective shot strength and toggle speed. Putting ignores Control and uses a forced putter whose scale adjusts by cup distance, capped at **40 yd**. Sources: [Hit bar](https://pangya.wiki/wiki/Hit_bar), [Impact Zone](https://pangya.wiki/wiki/Impact_Zone), [Bad Shot](https://pangya.wiki/wiki/Bad_Shot), [Control](https://pangya.wiki/wiki/Control), [Terrain Types](https://pangya.wiki/wiki/Terrain_Types).

Suggested server states:

`AIMING -> POWER_SELECTED -> RETURN_SWEEP -> {PANGYA_HIT | PINK_HIT | BAD_HIT} -> BALL_IN_FLIGHT -> TERRAIN_CONTACT/HAZARD -> {BALL_AT_REST | HOLE_OUT}`.

A shot consumes one stroke even if Bad. O.B., water hazard, and unplayable outcomes add an additional penalty/stroke according to the terrain rules below. The corpus does not specify whether the ordinary shot increment and stated “one stroke penalty” are represented as one combined increment or two separate increments; ordinary golf behavior suggests two total strokes for a hit into a hazard, but this must be packet/client-tested.

### Power, distance, and stat caps

* Base 1W drive formula: **`drive_yards = 200 + 2 × Power`**. Example: Power 20 = 240 yd. A Crimson Ring example makes that 244 yd, implying additive item distance after the base formula. [Power](https://pangya.wiki/wiki/Power), [Drive](https://pangya.wiki/wiki/Drive).
* Power penalty threshold is described as **20 plus 1 for every class above Rookie**. Each Power point above that level-dependent threshold removes **1 Control and 1 Accuracy**. Thus the likely rule is `effectiveControl = rawControl - max(0, Power - threshold(level))`, likewise Accuracy, but the class-to-offset chart and lower bounds are absent. [Power](https://pangya.wiki/wiki/Power).
* Accuracy, Control, Curve, and Spin effects all cap at **30 effective points** even if inventory totals exceed 30. Accuracy controls pink-zone width (and Assist trajectory-circle precision), Control slows the toggle, Curve controls impact-point curve magnitude, and Spin controls top/back spin magnitude. Power/Double Power temporarily shrink Accuracy. [Accuracy](https://pangya.wiki/wiki/Accuracy), [Control](https://pangya.wiki/wiki/Control), [Curve](https://pangya.wiki/wiki/Curve), [Spin](https://pangya.wiki/wiki/Spin).
* Top spin: lower flight/less wind, longer roll; Power Spin adds forward travel after stopping. Back spin: higher flight/more wind, earlier stop; Power Spin reverses after stopping. [Spin](https://pangya.wiki/wiki/Spin).

### Combo gauge and powered-shot transitions

* Normal -> Power Shot requires >=1 full gauge segment and one Alt press; firing consumes 1 segment and adds **10 yd** to every non-putter club.
* Normal -> Double Power Shot requires >=2 segments and two Alt presses; firing consumes 2 segments and normally adds **20 yd**. Power Potion is the explicit exception at **+15 yd**.
* Active items may enter these stances without gauge use. Power stance permits Tomahawk/Cobra/Spike; Double permits their Power variants. [Power Shot](https://pangya.wiki/wiki/Power_Shot).

The pages do not define gauge segment size, gauge gain events, cancellation/refund behavior, or precisely when a stance consumes its segment(s). Treat those as unresolved protocol/gameplay data.

### Terrain transitions and effective shot-strength ranges

| Terrain | Strength | Rest/contact behavior |
|---|---:|---|
| Tee | 98–100% | Starting area; tee shot |
| Fairway | 97–100% | Normal bounce/roll; tee landing raises Fairway Success Rate |
| Green | 100% | If it stays, force putter next; tee landing also raises Fairway Success Rate |
| Grass/ash/dirt rough | 82–100% | Deep rough stops sooner; shallow can be 100% |
| Snow | 87–90% | Between shallow/deep rough |
| Rock/metal | 97–100% | High first bounce, then damped |
| Wood | 97–100% | Extra bounces, slow stop |
| Bunker / desert Sand | 52–85% | Immediate stop; toggle much faster |
| Ice | 97–100% | Wider/more bounces, long roll; toggle much faster |
| Hot-spring Water | 82–85% | Submerged/no bounce; toggle much faster; not a hazard |
| Magic Carpet | 92–95% | Repeated wall-aided bouncing; no Bound Bonus; does not react to non-bouncing Tomahawk/Spike |

Hazards: **Water Hazard** makes the ball unretrievable, applies one-stroke penalty, and relocates it behind the liquid; front Power Spin may skip across it. Liquid outside course bounds is O.B. **O.B.** applies one-stroke penalty and restores the exact pre-shot position. **Unplayable** requires “Move Comet” to nearby fairway/tee for one stroke; if landed there directly, restore pre-shot position and still apply one-stroke penalty. Source: [Terrain Types](https://pangya.wiki/wiki/Terrain_Types).

## 2. Scoring and Pang

### Score formula and terminal states

For a normal hole-out, numeric score is **`strokes_taken - par`**. Names: Birdie -1, Eagle -2, Albatross -3, Par 0, Bogey +1, Double Bogey +2, Triple Bogey +3, +4, and +5. HIO is always one stroke and therefore is -2 on Par 3, -3 on Par 4, -4 on Par 5; the wiki calls it the best possible result and uses HIO rather than Eagle/Albatross naming on Par 3/4. Albatross is consequently described as Par-5-in-2 only. Sources: [Par](https://pangya.wiki/wiki/Par), [Birdie](https://pangya.wiki/wiki/Birdie), [Eagle](https://pangya.wiki/wiki/Eagle), [Albatross](https://pangya.wiki/wiki/Albatross), [Hole In One](https://pangya.wiki/wiki/Hole_In_One), [Bogey](https://pangya.wiki/wiki/Bogey), [Double Bogey](https://pangya.wiki/wiki/Double_Bogey), [Triple Bogey](https://pangya.wiki/wiki/Triple_Bogey), [+4](https://pangya.wiki/wiki/%2B4), [+5](https://pangya.wiki/wiki/%2B5).

**Give Up:** normally triggers at **Par + 5 strokes** (example: on Par 4, when player is “at the 9th stroke”) and records +5. A rare +6 physical-stroke situation can occur if the ball is moved before Give Up. A separate +5 hole-out is reportedly possible by reaching one stroke before Give Up, moving back to Tee, then chipping in. This language is too imprecise to place the check before or after stroke increment; implement the score cap separately from physical-stroke accounting and verify with captures. [Give Up](https://pangya.wiki/wiki/Give_Up), [+5](https://pangya.wiki/wiki/%2B5).

All positive results are **Over Par**. When a stroke can produce Over Par, cup beam becomes black (blue in Short Game), Spinning Cubes disappear, and “many” Pang bonuses are disabled. Double/Triple explicitly award no chip-in or long-putt Pang; +4 explicitly awards no hole-out Pang. [Over Par](https://pangya.wiki/wiki/Over_Par), [Double Bogey](https://pangya.wiki/wiki/Double_Bogey), [Triple Bogey](https://pangya.wiki/wiki/Triple_Bogey), [+4](https://pangya.wiki/wiki/%2B4).

### Pang formulas/conditions available in corpus

* **Over Drive** only on a tee shot ending non-O.B.: `max(0, floor(actual_distance_yards - current_club_drive_yards))` Pang, as implied by 232→233 = 1 and 234 = 2. Current drive includes Power Shot increase. Super PangYa doubles OD. [Over Drive](https://pangya.wiki/wiki/Over_Drive).
* **Beam Impact:** ball enters directly without ground contact; **100 Pang**, only when not Over Par. [Beam Impact](https://pangya.wiki/wiki/Beam_Impact).
* **Long Chip-In:** non-putter hole-out from **>=17 yd**; page says extra Pang but supplies no formula. [Chip In](https://pangya.wiki/wiki/Chip_In).
* **Long Putt:** only the first non-Over-Par putt qualifies; distance must be **>17 yd** (strictly farther, unlike chip-in wording), and success awards **2 Pang per yard**. Later putts never qualify. Rounding is unspecified. [Long Putt](https://pangya.wiki/wiki/Long_Putt).
* Pang sources also include Nice Approach, hole-out at Par or better, course-record awards, and Lounge sales, but this corpus has no Nice Approach/hole-out/clear-bonus tables. [Pang](https://pangya.wiki/wiki/Pang).
* Successful Tomahawk/Cobra/Spike gives **5 Pang if item-triggered, 10 if gauge-triggered**, even when Pangya impact fails; same values for Power variants. Special-shot pages cited below.
* Shuffle order gives **+10% Pang at game end** for Versus, Tournament, Pang Battle, and Grand Prix; excluded in Approach and SSC. Rounding/base inclusion unspecified. [Hole Order](https://pangya.wiki/wiki/Hole_Order).

## 3. Special shots

All command inputs occur on the return sweep **after halfway but before the start**, require selected power **>=80%**, and then require landing in white or pink except Power Curve, which requires white Impact Zone. [Tomahawk](https://pangya.wiki/wiki/Tomahawk), [Cobra](https://pangya.wiki/wiki/Cobra), [Spike](https://pangya.wiki/wiki/Spike), [Power Curve](https://pangya.wiki/wiki/Power_Curve).

| Shot | Stance / clubs | Command | Resolution |
|---|---|---|---|
| Tomahawk | Power; Wood/Iron/Wedge | Up, Down | High immediate arc; no bounce on direct ground impact |
| Cobra | Power; Wood only | Right, Up | Low then rises; reduced bounces and roll |
| Spike | Power; Wood only | Right, Down | Slow rise then steep plunge; no bounce |
| Power Curve | No Power stance stated; Wood/Iron/Wedge; impact point fully L/R | hold matching Left/Right | Swings far outward then back; strength tied to Curve |

Power Tomahawk/Cobra/Spike use identical commands from Double Power stance. On a non-Pangya but pink special-shot hit, each special shot still activates and gets its Pang bonus, but random Spin and Curve produce a random/unintended direction. Cobra and especially Spike become more erratic with Curve. Uphill ground can interfere with Cobra’s initially straight path and Spike’s incline. Cobra/Spike height and range are greatest on 1W, lower on 2W, lowest on 3W; Spike landing distance also shifts closer for positive elevation and farther for negative elevation. These are qualitative—no ballistic coefficients are supplied. Sources: same four pages.

## 4. Hole order and general room options

Orders: **Front** starts 1 sequentially; **Back** starts 10 sequentially; **Random** chooses a start then sequentially wraps 18→1; **Shuffle** randomly permutes holes. Availability: Versus 3/6/9 supports all four, 18 Front/Shuffle; Tournament and Course Practice 9 all four, 18 Front/Shuffle; Pang Battle 6/9 all four, 18 Front/Shuffle; Approach 3/6/9 Shuffle only; SSC 18 Shuffle only; Grand Prix 3/6 Front only, 9/18 Front/Shuffle. [Hole Order](https://pangya.wiki/wiki/Hole_Order).

## 5. Multiplayer modes

### Versus

2–4 players, 3/6/9/18 holes, alternating turns. **Stroke:** lowest total strokes wins; tie-break higher Pang. Clear Bonus depends on course/holes/remaining players. If disconnect leaves one, survivor may stop and collect accrued EXP/Pang without penalty or continue for Pang but no further EXP. **Match:** exactly 2 or 4, Red vs Blue; with 4, teammates alternate and share active-play features. Hole win = 1 team point, draw/loss = 0. Most holes wins, then Pang tie-break. **Dormie** ends early when lead exceeds holes remaining and gives Dormie Bonus. Any disconnect makes that team forfeit immediately. [Versus](https://pangya.wiki/wiki/Versus).

### Pang Battle

Exactly 2 or 4 players; 6/9/18 holes; per-hole Match scoring. All players stake Pang each hole. Unique lowest score wins opponents’ stake. Tied best carries pot to next hole and doubles it, capped at **8×**. If final hole is tied, play one Approach shot; closest without holing wins carried pot. Wind power may change per shot but angle does not. End fee is **5% of Pang won from battle stakes only**; rounding unspecified. Every fifth completed game awards **1,000 Pang pouch**, then 5-coin progress resets. No EXP or Treasure Points; wind-power-changing cards ignored. [Pang Battle](https://pangya.wiki/wiki/Pang_Battle).

### Approach Battle

Minimum 4, capacity 30; 3/6/9 holes; random course + forced Shuffle; no entry cost despite room image. Each hole assigns random cup distance and **40 seconds**. Players shoot once, then wait until timer expiry. Lowest remaining distance ranks best; per-hole and final total-distance rankings award treasure boxes, with thresholds/counts dependent on population but no numeric reward table. **OUT** if cup, O.B., Water Hazard, or no shot by timeout: add fixed **50 yd** to cumulative distance and block ordinary boxes for that hole. Missions can override cup-OUT reward exclusion. No records, combo increment, EXP, EXP-item consumption, or quit-rate effect. [Approach Battle](https://pangya.wiki/wiki/Approach_Battle).

Active mission predicates in the article: first-place distance odd/even; chip in; first uses mascot; exact rank; sum of all distances <= threshold; specified character and <10 yd; majority parity; >=50% chip in; own distance <= threshold; rank N exact distance; “more than four” mission whose explanation instead says 4 chip-ins/first four rewarded; any exact distance; highest remaining distance; first place remaining time <=10 sec; caddie and <20 yd; first shot; own parity; last shot; named player chips in. Six gender missions are marked removed since Season 4.5; three (fastest chip, specified-special-shot chip, total remaining time) are marked never used. Do not place removed/unused entries in the live pool.

### Lounge

Free movement/chat mode. Position states: standing -> PageDown sit -> PageDown lie; PageUp reverses one step. W/A/S/D, arrows, or click move. Player shops only on Dolfini: max open shops is floor-like listed **2/3 room capacity** (10→6, 20→13, 30→20), max **6 distinct items**, max unit/listing price **10,000,000 Pang**, **5% sales tax**. While own shop is open, owner cannot view other shops or leave; closing shop unlocks both. Room name is immutable after creation; password cannot later change/remove. [Lounge](https://pangya.wiki/wiki/Lounge).

## 6. Tournament-family modes

* **Tourney:** minimum 4; 9 or 18 holes; capacities 10/20/30. In-progress join allowed until 5 minutes after room creation for 9 holes or 10 minutes for 18. Time options: 9 = 15/20/25/30 min; 18 = 30/35/40/45/50. Natural and Artifact configurable. [Tourney](https://pangya.wiki/wiki/Tourney).
* **Short Game:** 9/18 holes, random starting position near cup except Par 3; initialize prior stroke count at 0/1/2 for Par 3/4/5. Yellow normal beam, blue Over-Par beam. Capacities 10/20/30; 9-hole time 15/20/25/30/35. The page omits 18-hole time choices. [Short Game](https://pangya.wiki/wiki/Short_Game).
* **Team Tournament (removed as of Fresh Up):** otherwise Tourney; equal Red/Blue teams, min 4 (2/team), no midgame join; lowest **sum of team scores** wins. Same standard capacities/orders/times. [Team Tournament](https://pangya.wiki/wiki/Team_Tournament).
* **Guild Battle:** exactly two guilds, equal initial rosters, >=3 per guild. Each player paired cross-guild; better hole score wins, and win/loss/draw points aggregate to guild total/Guild Points. Capacities 10/20/30; 9/18 holes; no Tiki scroll. Opponent disconnect immediately ends paired player’s game, who waits for others. Actual win/draw point values and pairing rule are missing. [Guild Battle](https://pangya.wiki/wiki/Guild_Battle).
* **GM Event:** Tourney variant, normally cap 100, occasionally 200; can start with unready players; no in-progress join, Treasure Points, trophies, or Tiki scroll. GM departure immediately kicks everyone. Event reward requires staying through end. Historical listed events are generally 30-minute 18-hole rooms. [GM Event](https://pangya.wiki/wiki/GM_Event).

### Special Shuffle Course (SSC)

One nonreturnable ticket creates fixed 30-min/cap-30/random-course/Shuffle room; cannot join in progress, change defaults after creation, use Artifact/Replay/Tiki, or continue room if creator leaves before start. Holes 1–17 are an unrevealed random sequence drawn across all courses; hole 18 is one of two special Par-4 variants. No Shuffle Bonus, records, or EXP; achievements still count; EXP boosters are not consumed. Club mastery per cleared hole = **1.5× normal**. Treasure Points invert (bad scores give more), without formula. [Special Shuffle Course](https://pangya.wiki/wiki/Special_Shuffle_Course).

Hole 18 fixed hole-out Pang: HIO 800, Eagle 600, Birdie 400, Par 200, Over Par 100 (same Natural/normal). A non-Give-Up hole-out also gives a random pouch. Listed normal values: 500, 1,500, 2,000, 6,000, 10,500, 11,000, 11,500. If initial players >20, jackpots become possible; listed values: 15,000, 100,000, 100,100, 100,500, 101,000, 105,000, 500,000, 500,100, 500,500, 501,000. Population-dependent probabilities/minimum and creator coin formula are absent.

## 7. Single-player and tutorial

* **Course Practice:** 9/18 solo; 9 all orders, 18 Front/Shuffle; time choices same as Tourney. Earn 1 EXP per cleared hole, no quit-rate penalty, no Treasure Points/reward, credit **1/3** ordinary Pang. Pang Coins/Spinning Cubes count fully and coin Pang is unreduced. [Course Practice](https://pangya.wiki/wiki/Course_Practice).
* **Hole Repeat:** repeat any non-SSC hole 9 or 18 times; same respective time choices; cup position fixed toggle; optional wind-change toggle. HIO forces wind to change next repetition regardless of toggle. Same EXP/quit/treasure rules, but credit **1/6** ordinary Pang. The page accidentally repeats the Course Practice 1/3 fact too. [Hole Repeat](https://pangya.wiki/wiki/Hole_Repeat).
* **Chip-In Practice Mode:** its short standalone article only identifies Grand Zodiac practice. Full Grand Zodiac article says entry consumes one nonreturnable ticket; 30 minutes; slope Apply/None, cup size x1–x9; optional island change after chip-in, while wind angle/power always change after chip-in; may leave any time and retain Pang. [Chip-In Practice Mode](https://pangya.wiki/wiki/Chip-In_Practice_Mode), [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac).
* **Tutorial:** scripted steps with per-step prizes; no Pang or EXP. [Tutorial](https://pangya.wiki/wiki/Tutorial).

## 8. Grand Zodiac / HIO mode

Rooms auto-create at event times, cap 100, cannot join in progress; reaching 100 changes countdown to 10 seconds. Player occupies one of 12 islands and retries identical position/wind until a chip-in. Miss -> reset to original shot conditions. Success -> mode-dependent condition transition. Every chip-in counts as HIO regardless of shot count. [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac).

**Per-HIO score is additive:** base success +1; first attempt +1; special shot +1; no arrow commands +1; non-Pangya +3. Thus maximum stated total appears **7** (1+1+1+1+3), although “special shot” normally requires arrow commands, making those two bonuses mutually exclusive under ordinary inputs; practical maximum likely 6 unless an item supplies a special shot. Ranking uses score only (not Pang), allows shared ranks. HIO-mode timer: originally 10 minutes, **7 minutes from Tomahawk Second Journey onward**.

Preset: maximum Power Gauge, Impact Zone +1 pixel, infinite Time Booster/Auto Caliper/Air Note. Records/HIO totals not saved; trophies/items/Pang won count. Active items and special Comets are consumed normally, with special Comet consumption **once per 4 shots**; Replay/cut-ins disabled. Only HIO, Comet, Mascot Pang bonuses; no Treasure Gauge.

C9→C1 levels alter cup size and HIO Pang. Exact Pang vector: **C9..C1 = 33,35,37,40,44,50,57,70,100**. All start C9; level rises/falls based on points over multiple games, but thresholds are absent.

Intermediate: flat tee, other shots visible, far distances only; after HIO return same island and preserve Spin/Curve, Power stance, last club. Advanced: sloped tee, other shots hidden, close/far allowed; after HIO move island and preserve Power stance only.

Gold beam: at **0:50**, opens a 30-sec qualifying window; each HIO adds a drawing entry. At **0:20**, choose Winner Takes All or Equal Distribution. Equal Distribution gives portions to everyone with >=1 qualifying HIO; jackpot size derives from starting room size, formula absent. Grand Zodiac Points depend on score and Intermediate/Advanced, formula absent.

## 9. EXP schedules

EXP depends on rank, holes, opponents and (for the documented eras) difficulty; boosters/events multiply or add by rules not supplied. No closed formula is asserted. Implement lookup tables versioned by season. [Experience Points](https://pangya.wiki/wiki/Experience_Points).

Notation below is exact corpus data: each tuple is `(3h,6h,9h,18h)`; entries within a player-count block are ranks 1..N.

**Season 4 Delight through Tomahawk Second Journey:** for star difficulty `s=1..5`, add `N×(s-1)` to every cell of the N-player 1-star table:

* N=2: rank1 `(4,9,14,29)`; rank2 `(2,5,9,21)`.
* N=3: rank1 `(6,14,22,46)`; rank2 `(4,9,15,33)`; rank3 `(2,7,12,28)`.
* N=4: rank1 `(8,19,30,63)`; rank2 `(6,15,25,56)`; rank3 `(4,13,22,49)`; rank4 `(2,10,18,42)`.

**After Tomahawk Second Journey:** exact tables by stars 1→5:

* N=2: S1 `[(7,14,20,40),(6,10,15,29)]`; S2 `[(8,14,21,41),(6,11,16,30)]`; S3 `[(8,15,22,43),(6,11,16,32)]`; S4 `[(8,16,23,45),(7,12,18,35)]`; S5 `[(9,17,25,49),(7,14,20,39)]`.
* N=3: S1 `[(9,17,25,50),(8,15,22,43),(7,13,18,36)]`; S2 `[(9,18,26,52),(8,16,23,45),(7,13,19,38)]`; S3 `[(10,19,27,54),(9,16,24,47),(7,14,20,40)]`; S4 `[(11,20,30,58),(9,18,26,51),(8,15,22,44)]`; S5 `[(12,22,33,64),(10,20,29,57),(9,17,25,50)]`.
* N=4: S1 `[(11,21,31,62),(10,19,29,56),(9,17,25,49),(8,15,22,42)]`; S2 `[(12,22,33,65),(11,20,30,59),(9,18,26,52),(8,16,23,45)]`; S3 `[(12,23,34,67),(11,21,31,62),(10,19,28,55),(9,16,24,47)]`; S4 `[(13,25,37,72),(12,23,34,67),(11,21,30,60),(10,18,27,53)]`; S5 `[(14,27,41,80),(13,26,38,75),(12,23,34,68),(11,21,31,61)]`.

Team Match exact `(4h..18h)`: winning team **6,8,10,...,34 = 2h-2**; losing team **3,4,5,...,17 = h-1**.

Course Practice/Hole Repeat: **1 EXP per completed hole**. Approach/Pang Battle/SSC/Tutorial explicitly give 0; Versus survivor after everyone leaves gains no further EXP for subsequent holes.

The pre-Second-Journey Tournament table is corrupt/incomplete in the source: player buckets 4–13,14–17,18–21,22–25,26–29,30; 1st row `6,7,8,9,10,1` (likely truncation/typo), Top 20% `4,5,6,7,8,9`, Middle 40% `2,3,4,5,6,7`, Lower `1,1,2,#,4,5`. Do not implement the `1` or `#` without another source.

## 10. Implementation ambiguities and residual risks

1. **BLOCKER — Experience Points source page:** Tournament EXP has literal corrupt cells (`1` for 30-player first, `#` for lower 22–25) and no current Tournament schedule. Requires official/server capture evidence.
2. **HIGH — scoring/hazards:** Give Up trigger timing, Move Comet accounting, O.B./hazard stroke sequencing, +5 exception, and physical strokes versus capped score are not defined precisely enough for a deterministic server.
3. **HIGH — physics:** no projectile equations, wind/elevation coefficients, impact-zone pixel timing, Accuracy/Control curves, terrain RNG selection, bounce/roll coefficients, cup collision, or special-shot ballistic constants. The corpus supplies qualitative behavior only.
4. **HIGH — economy:** missing ordinary hole-out, Nice Approach, long-chip, Clear/Dormie, treasure-box, Grand Zodiac Points/jackpot, SSC pouch probability/coin, and rounding formulas. “Many bonuses disabled Over Par” is not an allow/deny list.
5. **HIGH — mode ranking:** Approach reward population bands/ties and total-distance ties; Pang Battle stake initialization/fee rounding/tied Approach equality; Guild Battle point values/pairing; Tournament ranking/ties/trophies all absent.
6. **MEDIUM — versioning:** articles mix historical eras (removed Team Tournament, old/current EXP, event-only Grand Zodiac, removed Approach missions) without a single target build. Server rules must be feature/version gated.
7. **MEDIUM — internal inconsistencies:** Approach says “more than four” but explains exactly 4; Long Chip-In uses >=17 yd while Long Putt uses >17 yd; SSC source URL entry appears as `/wiki/Special_Shuffle_Course` but all other URL formats are consistent; Hole Repeat repeats the Course Practice 1/3 note alongside its own 1/6 rate; Grand Zodiac x1–x9 cup size says Pang follows a “Level” table that only maps C9–C1, without explicitly mapping x multipliers to C levels.
8. **MEDIUM — stat penalty:** class-to-Power threshold chart, effective-stat floor, item modifier order, and Assist System behavior beyond circle size are absent.
9. **LOW — source authority:** wiki prose contains grammatical errors and likely transcription mistakes. All constants should be validated against a chosen client/server revision before compatibility claims.

## Reimplementation guidance

Model rule data by **season/build and mode**, not globals. Separate physical stroke count, displayed capped score, hole terminal reason, Pang bonus ledger, and EXP/reward settlement. Use explicit state/event logs (`SHOT_STARTED`, `INPUT_RESOLVED`, `BALL_RESTORED`, `PENALTY_APPLIED`, `HOLE_OUT`, `GIVE_UP`) so ambiguous sequencing can be corrected after packet/client tests. Store EXP as lookup tables and mission predicates as data with active-version intervals. Avoid inventing physics from qualitative descriptions.
