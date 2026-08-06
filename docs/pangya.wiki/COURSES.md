# Courses and walkthrough data

## Scope and source convention
- Analyzed **all 21 `Category:Courses` pages and all 26 `Category:Course Walkthroughs` pages** in the captured corpus (47 articles total). `Course Practice` and `Special Shuffle Course` are synthesized in the gameplay document because their categories are game mode/tournament, not course or course-walkthrough.
- Source URLs below are canonical title-derived pangya.wiki URLs. Facts are taken from the stored wikitext, not from a live-page refresh.
- Overview and walkthrough evidence are deliberately separated. Overview pages define location, star difficulty, and the authoritative 18-hole par row. Walkthrough pages define three pin distances and elevations per hole plus tactics. **Distance is yards (`y` in source, normalized to `yd`); elevation is metres (`m`).** `?`, blank (`—`), `N/A`, and apparent source errors are retained rather than invented.

## Course overview catalogue

| Course source | Location | Difficulty | Par / shape | Theme, hazards, and overview mechanics |
|---|---|---:|---|---|
| [Abbot Mine](https://pangya.wiki/wiki/Abbot_Mine) | Oriens | 2★ | 73; H1–18: 4/3/4/4/5/3/4/4/5/3/4/5/4/5/5/3/4/4 | magical underground mine/excavation; walls, cliffs, water and elevated land. Reflection Pads reflect shots (H4; chainable); horizontal fans lift/redirect; branching slides keep rolling until stopped. |
| [Blue Lagoon](https://pangya.wiki/wiki/Blue_Lagoon) | Libera | 1★ | 72; H1–18: 4/3/4/4/5/3/5/4/4/3/4/5/4/4/5/3/4/4 | sunny Libera beach/ocean; explicitly beginner-friendly. No overview-specific mechanic. |
| [Blue Moon](https://pangya.wiki/wiki/Blue_Moon) | Libera | 3★ | 72; H1–18: 4/3/4/4/5/3/5/4/4/3/4/5/4/4/5/3/4/4 | rare two-moon night variant of Libera; darkness is the stated challenge, with luminous night plants. No overview-specific mechanic. |
| [Blue Water](https://pangya.wiki/wiki/Blue_Water) | Libera | 3★ | 72; H1–18: 4/3/4/4/5/3/5/4/4/3/4/5/4/4/5/3/4/4 | tropical Libera waterfront; palm trees and huts constrain placement. No overview-specific mechanic. |
| [Deep Inferno](https://pangya.wiki/wiki/Deep_Inferno) | Narakas | 5★ | 72; H1–18: 4/3/4/4/5/3/4/4/5/4/4/3/4/5/4/3/4/5 | subterranean cursed volcanic/dragon land in Narakas. No overview-specific mechanic documented; no walkthrough articles in corpus. |
| [Eastern Valley](https://pangya.wiki/wiki/Eastern_Valley) | Oriens | 2★ | 71; H1–18: 4/5/3/4/4/4/5/3/4/3/4/5/4/4/3/4/4/4 | hidden enchanted Oriens valley protected by Chronos Fairy spells. No overview-specific mechanic documented; no walkthrough articles in corpus. |
| [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac) | not applicable | special: Intermediate/Advanced plus player level C9–C1 | special HIO arena; no par/length | 12 floating islands around one central green; HIO/chip-in arena, not a normal 18-hole course. Timed up-to-100-player HIO mode and ticketed solo practice; details below. |
| [Ice Cannon](https://pangya.wiki/wiki/Ice_Cannon) | Maga Valley - Glacial Region | 2★ | 72; H1–18: 4/3/4/5/4/4/3/5/4/4/4/4/3/5/4/3/4/5 | midnight-sun glacial battlefield with frozen Silvia ships, ice walls/flows and water gaps. Ice produces long rolls/Over Drive; frozen cannons do not alter wind. |
| [Ice Inferno](https://pangya.wiki/wiki/Ice_Inferno) | Narakas - Coldest Depths | 5★ | 72; H1–18: 4/3/4/4/5/3/4/4/5/4/4/3/4/5/4/3/4/5 | frozen Narakas mountains/depths; evil-creature/Demon King setting. No overview-specific mechanic documented; no walkthrough articles in corpus. |
| [Ice Spa](https://pangya.wiki/wiki/Ice_Spa) | Maga Valley - Glacial Region | 1★ | 72; H1–18: 4/3/5/4/4/3/4/4/5/3/4/5/4/4/3/4/4/5 | snowy glacial-region hot-spring resort with penguins. No overview-specific mechanic documented. |
| [Lost Seaway](https://pangya.wiki/wiki/Lost_Seaway) | Oriens | 1★ | 72; H1–18: 4/3/5/4/4/5/4/4/4/3/4/5/4/4/4/4/3/4 | sky/ocean seaway, wrecked ships, waves and islands. Giant Booster Gates propel shots; walkthrough pages provide no tactics. |
| [Mystic Ruins](https://pangya.wiki/wiki/Mystic_Ruins) | Ventus | 3★ | 72; H1–18: 4/3/4/4/5/3/5/4/4/3/4/4/5/4/4/3/4/5 | Ventus ruins (overview explicitly says no known course description). Japan-only final/20th course; no walkthrough pages are present in corpus despite overview links. |
| [Pink Wind](https://pangya.wiki/wiki/Pink_Wind) | Ventus | 1★ | 72; H1–18: 4/3/4/5/4/3/5/4/4/4/4/3/5/4/4/3/4/5 | spring cherry-blossom Ventus Forest with Titans. No overview-specific mechanic. |
| [Sepia Wind](https://pangya.wiki/wiki/Sepia_Wind) | Ventus | 3★ | 72; H1–18: 4/3/4/4/5/4/3/4/5/4/5/4/3/4/4/3/5/4 | industrial Ventus village; windmills, trees and club factories. Risk/reward shots through windmills and trees; no walkthrough tactics supplied. |
| [Shining Sand](https://pangya.wiki/wiki/Shining_Sand) | Oriens | 2★ | 72; H1–18: 4/3/4/4/5/3/5/4/4/3/4/4/5/4/4/3/4/5 | hot, glittering Oriens desert and legendary battle site. Booster Gates/portals add distance and can chain routes, but can hurt score. |
| [Silvia Cannon](https://pangya.wiki/wiki/Silvia_Cannon) | Libera Eastern Valley | 4★ | 72; H1–18: 4/3/4/5/4/4/5/4/3/4/4/3/4/5/4/3/4/5 | Libera east-valley naval cruiser; fans, aircraft and cannons. Fans lift/redirect; aircraft downdraft/collision and some vertical motion; cannon used to force 6–9 m wind but has stayed off since Tomahawk; no instant replays. |
| [West Wiz](https://pangya.wiki/wiki/West_Wiz) | Western Entrance of Maga Valley | 1★ | 72; H1–18: 4/3/4/5/4/3/5/4/4/4/4/4/5/3/4/3/4/5 | violet-sky western Maga Valley entrance with shooting stars and cool breezes. Introductory Maga Valley course; no overview-specific mechanic. |
| [White Wiz](https://pangya.wiki/wiki/White_Wiz) | Maga Forest | 3★ | 72; H1–18: 4/3/4/5/4/5/4/4/3/5/4/3/4/5/4/3/4/4 | snow-covered magic elementary school in Maga Forest. Icy surface can boost a landed Comet; walkthrough has measurements but no hints. |
| [Wind Hill](https://pangya.wiki/wiki/Wind_Hill) | Ventus | 5★ | 72; H1–18: 4/3/4/4/5/4/3/4/5/4/5/4/3/4/4/3/5/4 | high Ventus hills with strong gales, windmills and trees. Wind tunnels temporarily alter direction/power (`?` display); wider bounce after normal/Cobra; unusually strong slope effects on putting. |
| [Wiz City](https://pangya.wiki/wiki/Wiz_City) | Maga Valley | 2★ | 72; H1–18: 4/3/4/4/5/3/5/4/4/3/4/5/4/4/5/3/4/4 | southern Maga Valley magical city/academy. Magic Carpet (95% strength; repeated bounce for normal/Cobra, inert for Tomahawk/Spike); chainable purple Booster Gates; no Over Drive; collectible Coins/Cubes vanish once shot count reaches par. |
| [Wiz Wiz](https://pangya.wiki/wiki/Wiz_Wiz) | Maga Valley | 4★ | 72; H1–18: 4/5/4/3/4/5/4/4/3/5/4/4/3/5/4/3/4/4 | foggy Maga Valley magic-school tournament among repeated cliffs. Planning-heavy cliff course; no overview-specific mechanic documented. |

### Grand Zodiac implementation facts (overview/game-mode data)
- No Front/Back 9 exists. Twelve tee islands, each with different distance/elevation, surround one green. On a miss, the player retries from identical position/wind until an HIO; every chip-in counts as HIO regardless of shot count. After success, conditions change by mode.
- HIO score: successful HIO +1; first attempt +1; special shot +1; no arrow command +1; no Pangya +3. Standard timer was 10 min originally, then 7 min; rooms auto-create during events, accept up to 100 players, and cannot be joined in progress.
- Presets: always-full Power Gauge, Impact Zone +1 px, infinite Time Booster/Auto Caliper/Air Note. Records/HIO records are not saved (trophies are); Replay Tape/cut-ins/Treasure Gauge unavailable; active items and special Comets are consumed; only HIO, Comet, Mascot Pang bonuses apply.
- Player levels C9→C1 shrink cup and increase HIO Pang: `33/35/37/40/44/50/57/70/100`. Intermediate: flat tee, other shots visible, far distances only, same island after HIO, retains spin/curve + Power Shot stance + club. Advanced: changing slope, others hidden, close/far distances, changes island, retains only Power Shot stance.
- Gold beam runs from 0:50 to 0:20; HIOs are jackpot entries, followed by Winner Takes All or Equal Distribution. Luck awards can grant 5 Silent Nerve Stabilizers or 3 Safe Silents.
- Solo Chip-In Practice costs one ticket/game (3/10/20 tickets cost 900/2,500/4,000 Points), lasts 30 min, offers tee slope on/off and cup x1–x9, changes wind after chip-in, optional tee-island change, and adds a top-view ruler. Source: [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac).

## Front 9 / Back 9 walkthrough data

Each bullet is `hole: par — three pin distances; three elevations — implementation/tactical facts`. Where a page has no Hints section, only its map metadata is reportable. Par discrepancies against overview are flagged.

### Abbot Mine
#### [Abbot Mine Front 9](https://pangya.wiki/wiki/Abbot_Mine_Front_9)
- **H1: P4** — 479 yd/489 yd/490 yd; elev -28.6 m/-28.1 m/-28.0 m — Short drivers should aim for the closer fairway on the left. Long drivers should aim for the larger fairway on the right for an easier approach. Both fairway areas are relatively flat, making this hole a good opportunity to chip in.
- **H2: P3** — 245 yd/248 yd/N/A; elev -13.0 m/-12.9 m/N/A — There are only two pins for this hole. A simple Par 3 hole.
- **H3: P4** — 430 yd/444 yd/449 yd; elev -28.5 m/-28.2 m/-28.1 m — Long drivers may green in one this hole by using a Spin Mastery and applying a frontal Power Spin with its Impact Point moved all the way to the top of the Comet. However, strong headwinds will make this difficult to do.
- **H4: P4** — 413 yd/415 yd/415 yd; elev -18.4 m/-18.5 m/-18.5 m — Hit the Reflection Pads to reach the green for an Eagle opportunity, as well as a possible HIO. You can choose to hit either one of the three reflection pads (depending on your club distance) and it will steer the Comet towards the green. Remember to apply Backspin to make sure the Comet does not roll off the green.
- **H5: P5** — 546 yd/562 yd/563 yd; elev 23.0 m/23.0 m/23.0 m — Shoot to the rough area on the left. The fan will blow the Comet upwards and place the Comet safely on the land above. Low drivers should use a Spike or Tomahawk shot on the approach shot.
- **H6: P3** — 230 yd/238 yd/N/A; elev 7.26 m/7.76 m/N/A — Take note of the elevation for this hole.
- **H7: P5** — 245 yd/247 yd/N/A; elev -54.3 m/-54.6 m/N/A — Roll down the slide to gain Over Drive. However, incorrectly rolling the Comet on the slide may cause it to roll to the further divider and end up about 300y away from the green. Using a 3I club with the Impact Point moved all the way back, hit a Power Tomahawk shot towards the green to pass through the cliff and land closer to the green. If you have items which increase drive distance, green in one is possible. If green in one, use this chance to get an Albatross. Take note that headwinds will make it difficult to green in one. It is possible to HIO this hole by applying top Power Spin with a Spin Mastery under sufficient drive and wind conditions. **Source discrepancy: walkthrough says par 5, overview says par 4.**
- **H8: P4** — 327 yd/333 yd/333 yd; elev -36.0 m/-36.3 m/-36.3 m — In this hole, it would be wise to just do a Spike shot. Players with longer club distances can also attempt for a HIO.
- **H9: P4** — 486 yd/499 yd/501 yd; elev -43.9 m/-43.8 m/-43.8 m — Because of the high height of the slide entrance, you would have to Tomahawk to get in. Otherwise, you could just go to the fairway and try for a long chip-in. **Source discrepancy: walkthrough says par 4, overview says par 5.**

#### [Abbot Mine Back 9](https://pangya.wiki/wiki/Abbot_Mine_Back_9)
- **H10: P3** — 228 yd/237 yd/224 yd; elev -5.37 m/-5.19 m/-5.32 m — Simple Par 3 hole.
- **H11: P4** — 477 yd/478 yd/481 yd; elev -6.82 m/-6.81 m/-6.81 m — Most players would use an iron club to enter the slide. This will bring your comet all the way to the green for an easy eagle putt.
- **H12: P5** — 518 yd/520 yd/523 yd; elev 23.7 m/23.7 m/23.8 m — Use the fan's power to blow your comet upwards to land safely on the rough. This is the most ideal spot.
- **H13: P4** — 336 yd/347 yd/-; elev -3.31 m/-3.64 m/- — Use front spin to jump the water and get your comet to reach the green in one shot.
- **H14: P4** — 374 yd/379 yd/-; elev -19.3 m/-19.6 m/- — Shoot to the fairway and then shoot to the green. It is possible to reach the green in one shot but you will need a drive that is longer than 290y without double power. The green is not putt friendly. **Source discrepancy: walkthrough says par 4, overview says par 5.**
- **H15: P5** — 295 yd/303 yd/-; elev -19.3 m/-19.3 m/- — Use a Spike shot to reach the green. Players who have a long drive can Tomahawk onto the green.
- **H16: P3** — 185 yd/212 yd/212 yd; elev 6.93 m/6.76 m/6.79 m — Easy Par 3 hole with relatively flat green.
- **H17: P4** — 370 yd/374 yd/384 yd; elev -3.78 m/-3.73 m/-3.74 m — You can choose to go either sides of the fairway.
- **H18: P4** — 437 yd/451 yd/456 yd; elev -1.84 m/-1.84 m/-1.84 m — Use an iron club and shoot through the arch to land safely on the fairway island.

### Blue Lagoon
#### [Blue Lagoon Front 9](https://pangya.wiki/wiki/Blue_Lagoon_Front_9)
- **H1: P4** — 416 yd/417 yd/425 yd; elev -0.70 m/-0.56 m/-0.98 m — There is a flat spot in the rough before the first bunker
- **H2: P3** — 224 yd/228 yd/231 yd; elev -3.41 m/-3.65 m/-3.10 m — Easy par 3, even for players with low power. The green is slightly sloped.
- **H3: P4** — 368 yd/372 yd/381 yd; elev 0.15 m/0.33 m/0.27 m — Shoot to the right of the house for a flatter fairway, over the trees for some small overdrive. The green is at the top of a hill, but slopes away from the tee. Make sure to land on, but don't roll off
- **H4: P4** — 369 yd/371 yd/375 yd; elev -3.04 m/-3.25 m/-3.46 m — The rough is flat between the first bunker and the path, the green sits on a sloped rough, which leads to awkward bounces
- **H5: P5** — 486 yd/490 yd/507 yd; elev -5.91 m/-7.55 m/-6.49 m — If you have low power, you can use a Spin Mastery to make it to the green in 2 strokes. Players with medium or strong power can hit to the center of the island, and tomahawk to the green.
- **H6: P3** — 215 yd/216 yd/226 yd; elev -2.20 m/-2.64 m/-2.50 m — It is very easy to overshoot the green on this hole! But if you hit too light, you might end up in the water
- **H7: P5** — 485 yd/488 yd/490 yd; elev -3.18 m/-3.17 m/-3.08 m — Since the fairway is uphill, you might get more distance on the drive if you apply backspin. Using a spin mastery with top powerspin is also effective
- **H8: P4** — 416 yd/426 yd/430 yd; elev -0.48 m/-0.52 m/-0.27 m — Shooting for the rough island surrounded by sand is one way to work, but shooting for the fairway is usually easier and makes for a better approach
- **H9: P4** — 383 yd/383 yd/388 yd; elev 0.30 m/0.35 m/0.24 m — If you use a full drive, the fairway is not even. It gets flatter the closer you are to the tee island.

#### [Blue Lagoon Back 9](https://pangya.wiki/wiki/Blue_Lagoon_Back_9)
- **H10: P3** — 283 yd/289 yd/291 yd; elev -2.97 m/-2.67 m/-2.48 m — To get to the green, tomahawk onto the opposite side of the mountain and let it bounce. Players with high drive might be able to make it with a spin mastery and a full backspin tomahawk (if you powerspin, it will roll back off)
- **H11: P4** — 356 yd/357 yd/264 yd; elev -6.27 m/-6.12 m/-5.94 m — There are a couple of spots on the fairway island that are flat, but finding them can be a challenge.
- **H12: P5** — 543 yd/544 yd/554 yd; elev -12.6 m/-13.2 m/-13.1 m — A spike off the tee and a tomahawk on the 2nd stroke will get the ball to the green with most powers.
- **H13: P4** — 428 yd/442 yd/443 yd; elev -0.38 m/-0.82 m/-1.07 m — This green breaks away from the approach, so putting on too much power, or missing pangya, will likely cause an overshoot into the rough
- **H14: P4** — 425 yd/441 yd/441 yd; elev -6.63 m/-6.11 m/-6.23 m — There is a small hill at the far edge of the fairway, so it might be better to hit it into the flatter rough
- **H15: P5** — 261 yd/270 yd/279 yd; elev -0.05 m/-0.05 m/-0.04 m — Tomahawk or spike for an easy Albatross
- **H16: P3** — 220 yd/231 yd/236 yd; elev -6.25 m/-5.52 m/-5.41 m — Pretty simple hole, the green is below the tee so don't use as much power.
- **H17: P4** — 416 yd/417 yd/439 yd; elev -24.2 m/-24.4 m/-24.8 m — There is a very uneven fairway, but there are some parts of the rough that are flat. A long drive might get to the flat part of the fairway as well.
- **H18: P4** — 409 yd/415 yd/425 yd; elev -4.82 m/-5.02 m/-5.21 m — Shooting to the bridge is tempting, but risks OB. Stay on the fairway, it is an easy approach at any power

### Blue Moon
#### [Blue Moon Front 9](https://pangya.wiki/wiki/Blue_Moon_Front_9)
- **H1: P4** — 385 yd/377 yd/401 yd; elev -1.11 m/-1.14 m/-1.35 m — Short course, so if you chip with woods, aim close! Some pins are very close to the edge, so you might want to use the short irons anyway
- **H2: P3** — 228 yd/231 yd/242 yd; elev 3.01 m/3.49 m/3.32 m — Some of the pins are behind the lighthouse, so you need to use curve. The 242 is also too far to powerbackspin, so make sure to remember a potion
- **H3: P4** — 375 yd/386 yd/394 yd; elev 4.57 m/4.68 m/4.73 m — It is much easier to hit to the middle island, but if you can make it to the fairway, it is flatter.
- **H4: P4** — 417 yd/434 yd/438 yd; elev 5.98 m/6.13 m/5.60 m — If you have short drive, stop before the bridge. If you have good drive, shoot right over the river. Do not try to land on the bridge, it's too risky!
- **H5: P5** — 497 yd/507 yd/515 yd; elev 4.12 m/4.28 m/4.22 m — A long course that is hilly. The slope should allow good overdrive, but the green is higher than the fairway
- **H6: P3** — 207 yd/210 yd/221 yd; elev 5.47 m/5.80 m/6.09 m — The green island is above the fairway. Tomahawk or you might bounce off the side into the water!
- **H7: P5** — 537 yd/542 yd/556 yd; elev 12.1 m/12.8 m/12.6 m — Unless you have 270+ normal drive, an albatross is impossible. If you do, a double tomahawk to the corner of the upper section will put you in range
- **H8: P4** — 412 yd/422 yd/437 yd; elev 26.7 m/27.5 m/27.2 m — The fairway has a nasty angle. Landing in the rough is a lot flatter, tomahawk to the green for the approach.
- **H9: P4** — 414 yd/427 yd/432 yd; elev -0.79 m/-0.30 m/-0.21 m — Easy hole, but be careful, too far left and you might hit the mushrooms!

#### [Blue Moon Back 9](https://pangya.wiki/wiki/Blue_Moon_Back_9)
- **H10: P3** — 221 yd/221 yd/236 yd; elev 5.24 m/5.82 m/5.80 m — Simple approach, but the green is above the tee. Use extra power, or you might stop in the rough
- **H11: P4** — 440 yd/441 yd/455 yd; elev 2.09 m/1.72 m/1.47 m — Try a bit of curve, careful of the mountain. The green is a bit above the fairway, but it also slopes downward. Careful not to hit the mushrooms, or undershoot the green.
- **H12: P5** — 268 yd/269 yd/280 yd; elev 0.06 m/0.05 m/0.04 m — TOMAHAWK or spike to the green for an easy albatross, or if you're lucky, a Hole in One!
- **H13: P4** — 385 yd/395 yd/396 yd; elev -1.89 m/-1.84 m/-2.16 m — You can either shoot to the end of the fairway, or try to go in the rough past the trees. The rough is a bit closer to the green and flatter, but the penalty might negate the advantage
- **H14: P4** — 370 yd/384 yd/395 yd; elev 3.83 m/4.51 m/4.69 m — Another short hole. Between the bunker and the path is pretty flat, but going to the fairway is a safer approach.
- **H15: P5** — 505 yd/507 yd/512 yd; elev 6.29 m/6.69 m/6.61 m — There is a bunker in the fairway, with hills leading into it. Aim near the top of the hill on the left, then tomahawk to the green. The green is above and surrounded by a bunker, so be careful!
- **H16: P3** — 265 yd/268 yd/285 yd; elev 2.21 m/2.38 m/2.71 m — This is a very long par 3. You can sometimes double-tomahawk to the green, or sometimes bounce off the mountain.
- **H17: P4** — 424 yd/427 yd/435 yd; elev 1.26 m/1.25 m/1.48 m — If you have about 25 strength, you can get to the fairway. If not, hit near the lighthouse for an easier approach.
- **H18: P4** — 455 yd/461 yd/476 yd; elev 8.10 m/8.11 m/8.39 m — If you can tomahawk, shoot left of the lighthouse to the far fairway. If you have enough power, shoot to the right of the lighthouse to the far fairway (250y +, be careful of the trees on the approach). If you don't have either, just shoot down the fairway and go to the far fairway on your next shot

### Blue Water
#### [Blue Water Front 9](https://pangya.wiki/wiki/Blue_Water_Front_9)
- **H1: P4** — 409 yd/438 yd/446 yd; elev -0.42 m/-1.17 m/-1.55 m — No hint text in source.
- **H2: P3** — 213 yd/228 yd/236 yd; elev -3.48 m/-2.99 m/-2.96 m — No hint text in source.
- **H3: P4** — 379 yd/392 yd/398 yd; elev -0.28 m/-0.06 m/-0.32 m — No hint text in source.
- **H4: P4** — 376 yd/379 yd/392 yd; elev -2.94 m/-2.84 m/-2.32 m — No hint text in source.
- **H5: P5** — 507 yd/507 yd/509 yd; elev -6.23 m/-6.33 m/-6.46 m — No hint text in source.
- **H6: P3** — 235 yd/239 yd/243 yd; elev -3.16 m/-3.53 m/-3.28 m — No hint text in source.
- **H7: P5** — 506 yd/507 yd/512 yd; elev -3.21 m/-3.25 m/-3.05 m — No hint text in source.
- **H8: P4** — 434 yd/—/445 yd; elev -0.31 m/-0.35 m/-0.17 m — No hint text in source.
- **H9: P4** — 383 yd/395 yd/401 yd; elev 0.33 m/0.27 m/0.05 m — No hint text in source.

#### [Blue Water Back 9](https://pangya.wiki/wiki/Blue_Water_Back_9)
- **H10: P3** — 292 yd/307 yd/310 yd; elev ?/?/? — No hint text in source.
- **H11: P4** — 364 yd/371 yd/390 yd; elev ?/?/? — No hint text in source.
- **H12: P5** — 533 yd/544 yd/558 yd; elev ?/?/? — No hint text in source.
- **H13: P4** — 424 yd/439 yd/441 yd; elev ?/?/? — No hint text in source.
- **H14: P4** — 457 yd/462 yd/472 yd; elev ?/?/? — No hint text in source.
- **H15: P5** — 272 yd/283 yd/286 yd; elev ?/?/? — No hint text in source.
- **H16: P3** — 227 yd/236 yd/246 yd; elev ?/?/? — No hint text in source.
- **H17: P4** — 403 yd/442 yd/453 yd; elev ?/?/? — No hint text in source.
- **H18: P18** — 405 yd/434 yd/444 yd; elev ?/?/? — No hint text in source. **Source discrepancy: walkthrough says par 18, overview says par 4.**

### Ice Spa
#### [Ice Spa Front 9](https://pangya.wiki/wiki/Ice_Spa_Front_9)
- **H1: P4** — ?/?/?; elev ?/?/? — No hint text in source.
- **H2: P3** — 232 yd/236 yd/239 yd; elev ?/?/? — No hint text in source.
- **H3: P5** — 422 yd/423 yd/?; elev ?/?/? — No hint text in source.
- **H4: P4** — ?/?/?; elev ?/?/? — No hint text in source.
- **H5: P4** — ?/?/?; elev ?/?/? — No hint text in source.
- **H6: P3** — 233 yd/239 yd/240 yd; elev ?/?/? — No hint text in source.
- **H7: P4** — 379 yd/384 yd/?; elev ?/?/? — No hint text in source.
- **H8: P4** — ?/?/?; elev ?/?/? — No hint text in source.
- **H9: P5** — 264 yd/268 yd/270 yd; elev ?/?/? — No hint text in source.

#### [Ice Spa Back 9](https://pangya.wiki/wiki/Ice_Spa_Back_9)
- **H10: P3** — 232 yd/234 yd/234 yd; elev ?/?/? — No hint text in source.
- **H11: P4** — 303 yd/310 yd/?; elev ?/?/? — No hint text in source.
- **H12: P5** — ?/?/?; elev ?/?/? — No hint text in source.
- **H13: P4** — 385 yd/390 yd/?; elev ?/?/? — No hint text in source.
- **H14: P4** — 230 yd/238 yd/241 yd; elev ?/?/? — A 2w Tomahawk will get you over the ice hill and onto the green. Watch the power though, as the green's on lower ground.
- **H15: P3** — 233 yd/237 yd/239 yd; elev ?/?/? — No hint text in source.
- **H16: P4** — 349 yd/352 yd/354 yd; elev ?/?/? — No hint text in source.
- **H17: P4** — 318 yd/226 yd/?; elev ?/?/? — No hint text in source.
- **H18: P5** — ?/?/?; elev ?/?/? — No hint text in source.

### Ice Cannon
#### [Ice Cannon Front 9](https://pangya.wiki/wiki/Ice_Cannon_Front_9)
- **H1: P4** — 477 yd/455 yd/468 yd; elev ?/?/? — Shoot left with some slight curve to get in the longer shoot and very close to the green. If you have enough power (and perhaps some tailwind), the right ice flow offers more overdrive. Some players have mastered a tomahawk with an iron for the super pangya multiplier on this hole.
- **H2: P3** — 181 yd/198 yd/202 yd; elev ?/?/? — This green is very nasty, a power backspin near the hole is the best bet. Too short, and you will bounce on the ice and past the green. Too long and you overshoot the green anyway!
- **H3: P4** — 347 yd/357 yd/370 yd; elev ?/?/? — Players with low power will want to land somewhere between the 3 ice ponds and hope their 3w makes it over the ship. A safe bet is to just use an iron to get on the top, bouncing off the side is never a good thing!
- **H4: P5** — 523 yd/537 yd/545 yd; elev ?/?/? — Players with low drive will need to either tomahawk or spike to make it into the canyon. Make it all the way to the other side for an easy approach to the green!
- **H5: P4** — 431 yd/442 yd/455 yd; elev ?/?/? — Use a 2w to get over the aircraft and onto the ice for big overdrive, or shoot down the ship for a flat fairway for the approach
- **H6: P4** — 364 yd/377 yd/377 yd; elev ?/?/? — Ignore the grassy fairway and shoot onto the ship on your first stroke. The green is far below the ship, so be careful not to overshoot.
- **H7: P3** — 236 yd/250 yd/259 yd; elev ?/?/? — Not sure why there is a ship on this course, just tomahawk from the tee to the green. Spike also works, but since the green is so far down, your spike will cover some serious distance!
- **H8: P5** — 378 yd/384 yd/387 yd; elev ?/?/? — If you are confident, shoot to the right of the first wall and land in the snow for an easy approach (J1 on the map). You can also try to curve onto the ice pond and tomahawk to the green, or go all the way around.
- **H9: P4** — 475 yd/483 yd/485 yd; elev ?/?/? — Shoot to the middle of the ship, and then either spike or tomahawk down to the green (green is much lower, so spike will go much farther)

#### [Ice Cannon Back 9](https://pangya.wiki/wiki/Ice_Cannon_Back_9)
- **H10: P4** — 406 yd/420 yd/428 yd; elev ?/?/? — With some practice, it is easy to land on the ice flow for big overdrive. Players with low power should try a tomahawk with powerspin
- **H11: P4** — 368 yd/386 yd/388 yd; elev ?/?/? — Shoot down the middle of the ship on the ice for overdrive, or shoot to the lower platform on your left for a flat, closer-to-level, approach
- **H12: P4** — 495 yd/504 yd/507 yd; elev ?/?/? — The fairway is nice and downward sloped, so you can get plenty of overdrive without dealing with ice at all. Good thing too, since the holes in the ice cause lots of water hazards
- **H13: P3** — 169 yd/182 yd/191 yd; elev ?/?/? — Deceptively simple, a shot with a 3w or even the long irons will hit the giant flower. A tomahawk with an iron, or perhaps a powerbackspin with a high wood, will get above or below the flower.
- **H14: P5** — 541 yd/554 yd/558 yd; elev ?/?/? — A good hit onto the ice will give good overdrive and good approach. A tomahawk with an iron onto the cliff side by the yellow line gives lots of overdrive, with a super pangya multiplier!
- **H15: P4** — 391 yd/412 yd/416 yd; elev ?/?/? — Simple hole, hit from the tee onto the ship, then to the green. The green is level with the ship, so your placement would depend on which pin you get.
- **H16: P3** — ?/?/?; elev ?/?/? — You can tomahawk with an iron over the wall
- **H17: P4** — 421 yd/431 yd/444 yd; elev ?/?/? — Either spike into the canyon for lots of overdrive, or hit it straight down the ship for a level fairway (but a green that is much lower)
- **H18: P5** — —/—/—; elev ?/?/? — Tomahawk or Spike into the chute to get to the green in one stroke

### Lost Seaway
#### [Lost Seaway Front 9](https://pangya.wiki/wiki/Lost_Seaway_Front_9)
- **H1: P4** — 360 yd/361 yd/369 yd; elev ?/?/? — No hint text in source.
- **H2: P3** — ?/?/?; elev ?/?/? — No hint text in source.
- **H3: P5** — 503 yd/?/—; elev ?/?/? — No hint text in source.
- **H4: P4** — 403 yd/404 yd/411 yd; elev ?/?/? — No hint text in source.
- **H5: P4** — 324 yd/333 yd/339 yd; elev ?/?/? — No hint text in source.
- **H6: P5** — ?/?/?; elev ?/?/? — No hint text in source.
- **H7: P4** — 385 yd/390 yd/397 yd; elev ?/?/? — No hint text in source.
- **H8: P4** — 302 yd/?/?; elev ?/?/? — No hint text in source.
- **H9: P4** — 388 yd/398 yd/403 yd; elev ?/?/? — No hint text in source.

#### [Lost Seaway Back 9](https://pangya.wiki/wiki/Lost_Seaway_Back_9)
- **H10: P3** — 230 yd/239 yd/247 yd; elev ?/?/? — No hint text in source.
- **H11: P4** — 349 yd/356 yd/363 yd; elev ?/?/? — No hint text in source.
- **H12: P5** — 487 yd/494 yd/495 yd; elev ?/?/? — No hint text in source.
- **H13: P4** — ?/?/?; elev ?/?/? — No hint text in source.
- **H14: P4** — ?/?/?; elev ?/?/? — No hint text in source.
- **H15: P4** — 495 yd/?/?; elev ?/?/? — No hint text in source.
- **H16: P4** — 372 yd/374 yd/389 yd; elev ?/?/? — No hint text in source.
- **H17: P3** — ?/?/?; elev ?/?/? — No hint text in source.
- **H18: P4** — 455 yd/456 yd/467 yd; elev ?/?/? — No hint text in source.

### Pink Wind
#### [Pink Wind Front 9](https://pangya.wiki/wiki/Pink_Wind_Front_9)
- **H1: P4** — 352 yd/357 yd/338 yd; elev ?/?/? — The fairway is flat before the lake, this hole is short enough so that the longer approach does not make a difference.
- **H2: P3** — 221 yd/223 yd/231 yd; elev ?/?/? — Easy par 3, but the green is below the tee. Be careful not to overpower.
- **H3: P4** — 365 yd/?/?; elev ?/?/? — You can bounce on the path for bigger overdrive, or shoot short for a flat part of the path/rough to approach. The origin of the line on the map is a 3rd route, a flat rough level with the green, but is a very risky shot.
- **H4: P5** — ?/?/?; elev ?/?/? — Players with medium power can try to spike with spin mastery (full backspin) to get to the green in one stroke. Or try to land on the path (or even sometimes the fairway) and shoot over the peninsula onto the green for an eagle.
- **H5: P4** — 377 yd/376 yd/?; elev ?/?/? — The small rough protruding into the pond is flatter than the fairway
- **H6: P3** — 230 yd/?/?; elev ?/?/? — The green is above the tee, so make sure to add power (even with tomahawk)
- **H7: P5** — ?/?/?; elev ?/?/? — The left branch is farther from the green, but a clear shot. The right is closer, but you might have to curve around trees
- **H8: P4** — 387 yd/396 yd/399 yd; elev ?/?/? — This hole is so short that players who rely on woods for chipping will want to avoid their 1w.
- **H9: P4** — 439 yd/445 yd/?; elev ?/?/? — The left branch is closer to the green, but you will most likely be blocked by the high hill for the approach. The right is not flat, but it is a clear shot. Make sure to use plenty of power, or you will end up in the bunker

#### [Pink Wind Back 9](https://pangya.wiki/wiki/Pink_Wind_Back_9)
- **H10: P4** — 379 yd/389 yd/395 yd; elev ?/?/? — The rough between the river and the cliff (J1 on the map) is both closer to the green than the fairway, and level.
- **H11: P4** — 379 yd/388 yd/395 yd; elev ?/?/? — The rough behind the billboard is flat. Don't worry, you can shoot through it without resistance.
- **H12: P3** — 235 yd/241 yd/?; elev ?/?/? — Before Season 4, a missed pangya for tomahawk would send the ball into the water. But now, just don't use spike!
- **H13: P5** — ?/?/?; elev ?/?/? — This is another uphill battle, either use backspin on the fairway to arch higher, or top powerspin with a spin mastery. I have also seen people land on the path, but this is not for beginners!
- **H14: P4** — 404 yd/412 yd/417 yd; elev ?/?/? — From the tee, shoot left over the small hill to land on a fairly flat fairway for an easy approach. If your drive is high enough, shoot straight and hope you clear the cliff for your approach.
- **H15: P4** — 361 yd/363 yd/375 yd; elev ?/?/? — The rough between the trees and the cliff (J1 on the map) is flatter than the fairway
- **H16: P3** — ?/?/?; elev ?/?/? — This green is much higher than the tee, so add plenty of power to your shot.
- **H17: P4** — 396 yd/401 yd/402 yd; elev ?/?/? — The fairway island to the right is closer to the green, but the stretch of fairway straight ahead is flatter
- **H18: P5** — ?/?/?; elev ?/?/? — The best approach is to tomahawk onto the path, and tomahawk again to the green. Shooting on the fairway will often lead you to the bunker, and it is much lower than the green

### Sepia Wind
#### [Sepia Wind Front 9](https://pangya.wiki/wiki/Sepia_Wind_Front_9)
- **H1: P4** — 407 yd/415 yd/415 yd; elev ?/?/? — No hint text in source.
- **H2: P3** — 292 yd/299 yd/299 yd; elev ?/?/? — No hint text in source.
- **H3: P4** — 414 yd/419 yd/434 yd; elev ?/?/? — No hint text in source.
- **H4: P4** — 451 yd/464 yd/466 yd; elev ?/?/? — No hint text in source.
- **H5: P5** — 473 yd/479 yd/485 yd; elev ?/?/? — No hint text in source.
- **H6: P4** — 415 yd/429 yd/439 yd; elev ?/?/? — No hint text in source.
- **H7: P3** — 231 yd/240 yd/245 yd; elev ?/?/? — No hint text in source.
- **H8: P4** — 410 yd/421 yd/440 yd; elev ?/?/? — No hint text in source.
- **H9: P5** — 487 yd/492 yd/499 yd; elev ?/?/? — No hint text in source.

#### [Sepia Wind Back 9](https://pangya.wiki/wiki/Sepia_Wind_Back_9)
- **H10: P4** — 384 yd/387 yd/398 yd; elev ?/?/? — No hint text in source.
- **H11: P5** — 584 yd/588 yd/591 yd; elev ?/?/? — No hint text in source.
- **H12: P4** — 443 yd/447 yd/452 yd; elev ?/?/? — No hint text in source.
- **H13: P3** — 224 yd/227 yd/236 yd; elev ?/?/? — No hint text in source.
- **H14: P4** — 354 yd/372 yd/384 yd; elev ?/?/? — No hint text in source.
- **H15: P4** — 413 yd/417 yd/424 yd; elev ?/?/? — No hint text in source.
- **H16: P3** — 248 yd/255 yd/262 yd; elev ?/?/? — No hint text in source.
- **H17: P5** — 474 yd/482 yd/484 yd; elev ?/?/? — No hint text in source.
- **H18: P4** — 385 yd/395 yd/402 yd; elev ?/?/? — No hint text in source.

### Shining Sand
#### [Shining Sand Front 9](https://pangya.wiki/wiki/Shining_Sand_Front_9)
- **H1: P4** — 401 yd/401 yd/411 yd; elev ?/?/? — If you underpower your approach, you will end up in the bunker
- **H2: P3** — 231 yd/239 yd/248 yd; elev ?/?/? — A tomahawk will hit the UFO. Try a cobra instead! The green is far below the tee
- **H3: P4** — 271 yd/279 yd/284 yd; elev ?/?/? — If you use too much power, you will hit over the portal. A safe way is to spike right to the green, if you have enough power
- **H4: P4** — 407 yd/414 yd/425 yd; elev ?/?/? — Simple approach, but the fairway is uphill. You might want to apply backspin
- **H5: P5** — 431 yd/433 yd/438 yd; elev ?/?/? — Curve around the UFO into the patch of rough for a good approach, it will take practice to not get blocked by the obelisks
- **H6: P3** — 228 yd/231 yd/238 yd; elev ?/?/? — Ignore the portal on the ground and go right for the green. Don't overpower it, the green is far below the tee
- **H7: P5** — 486 yd/492 yd/498 yd; elev ?/?/? — If you hit it right into the portal in the river, it will hit another and get to the green.
- **H8: P4** — 412 yd/413 yd/419 yd; elev ?/?/? — The fairway is steeply downhill, so some curve with a 2w or 3w can give lots of overdrive
- **H9: P4** — 274 yd/284 yd/284 yd; elev ?/?/? — A properly-aimed powercurve tomahawk will get to the green in one shot

#### [Shining Sand Back 9](https://pangya.wiki/wiki/Shining_Sand_Back_9)
- **H10: P3** — 226 yd/230 yd/233 yd; elev ?/?/? — Careful, even a tomahawk can hit the wall.
- **H11: P4** — 438 yd/444 yd/445 yd; elev ?/?/? — A tomahawk or spike into the portal might get to the green in one shot
- **H12: P4** — 378 yd/387 yd/391 yd; elev ?/?/? — 3 ways to approach: shoot left onto the fairway, shoot straight and bounce on the UFO (use 2w or lower) or try to land on the stone pathway (high risk of sand)
- **H13: P5** — 473 yd/482 yd/485 yd; elev ?/?/? — Shoot far onto the patch of rough, and with a bit of curve you can tomahawk onto the green. Too much curve will hit the UFO, too little will hit the pyramid
- **H14: P4** — 414 yd/417 yd/423 yd; elev ?/?/? — Hit properly into the portal, and you might get to the green
- **H15: P4** — 354 yd/358 yd/364 yd; elev ?/?/? — Hit onto the portal with back powerspin. If you don't, it will roll off the green.
- **H16: P3** — 241 yd/242 yd/269 yd; elev ?/?/? — Straight forward par 3, remember the green is below the tee
- **H17: P4** — 358 yd/364 yd/366 yd; elev ?/?/? — The fairway is pretty flat if you shoot left, but you can also try to hit between the obelisks for a closer approach
- **H18: P5** — ?/?/?; elev ?/?/? — Hit left along the rough (right in the picture) for a closer approach (two tomahawks will work). You can also shoot straight, and hit into the portal to get on the green

### Silvia Cannon
#### [Silvia Cannon Front 9](https://pangya.wiki/wiki/Silvia_Cannon_Front_9)
- **H1: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H2: P3** — —/—/—; elev ?/?/? — No hint text in source.
- **H3: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H4: P5** — —/—/—; elev ?/?/? — No hint text in source.
- **H5: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H6: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H7: P5** — —/—/—; elev ?/?/? — No hint text in source.
- **H8: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H9: P3** — —/—/—; elev ?/?/? — No hint text in source.

#### [Silvia Cannon Back 9](https://pangya.wiki/wiki/Silvia_Cannon_Back_9)
- **H10: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H11: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H12: P3** — —/—/—; elev ?/?/? — No hint text in source.
- **H13: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H14: P5** — —/—/—; elev ?/?/? — No hint text in source.
- **H15: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H16: P3** — —/—/—; elev ?/?/? — No hint text in source.
- **H17: P4** — —/—/—; elev ?/?/? — No hint text in source.
- **H18: P5** — —/—/—; elev ?/?/? — No hint text in source.

### West Wiz
#### [West Wiz Front 9](https://pangya.wiki/wiki/West_Wiz_Front_9)
- **H1: P4** — 369 yd/371 yd/381 yd; elev ?/?/? — Shoot straight then over the rough onto the green, very easy hole. Not so easy if you're looking for a flat spot!
- **H2: P3** — 207 yd/211 yd/221 yd; elev ?/?/? — Very simple par 3, the green is even pretty level with the tee!
- **H3: P4** — 407 yd/427 yd/431 yd; elev ?/?/? — If you have enough drive, shoot with a 2w to the right, with a bit of curve, for more overdrive
- **H4: P5** — 496 yd/509 yd/510 yd; elev ?/?/? — The fairway has an upward slope, so getting overdrive is not easy. Tomahawk or Spike to the green for an eagle.
- **H5: P4** — 415 yd/422 yd/429 yd; elev ?/?/? — For more overdrive, hit up above on the left hill. For a flatter, easier approach, hit right onto the level fairway
- **H6: P3** — 212 yd/215 yd/242 yd; elev ?/?/? — The green is lower than the tee, so be careful not to overshoot.
- **H7: P5** — 520 yd/524 yd/526 yd; elev ?/?/? — Shoot straight onto the rough (pictured), then tomahawk or spike to the green for an eagle. The green is far below the tee, so a spike works well for the farther pins.
- **H8: P4** — 360 yd/380 yd/390 yd; elev ?/?/? — Try to stay on the fairway, but not too far away from the green on your first shot. The bunker and rough means that if you undershoot, you have a difficult approach chip.
- **H9: P4** — 422 yd/435 yd/447 yd; elev ?/?/? — The fairway slopes far downward, so use a smaller club for bigger overdrive (just make sure you get to the fairway!

#### [West Wiz Back 9](https://pangya.wiki/wiki/West_Wiz_Back_9)
- **H10: P4** — ?/?/?; elev ?/?/? — If you have a long drive, go around for an easier approach. If you have low power, hit to the rough patch between the trees to be closer to the green.
- **H11: P4** — 320 yd/324 yd/345 yd; elev ?/?/? — You can try to curve around the tree to go on the left path in the rough (closer to the green), or shoot straight onto the fairway for a flatter approach (but low powered players might be blocked by the castle)
- **H12: P4** — 432 yd/433 yd/443 yd; elev ?/?/? — Try hitting straight ahead, then over the rough and past the tower on the green.
- **H13: P5** — 475 yd/482 yd/492 yd; elev ?/?/? — Another sloped fairway, a tomahawk or spike onto the fairway, then another one onto the green, is the best bet for eagle
- **H14: P4** — 199 yd/200 yd/206 yd; elev ?/?/? — The green is far above the tee, so make sure to use extra power (but not so much as you pass the green altogether!) **Source discrepancy: walkthrough says par 4, overview says par 3.**
- **H15: P4** — 452 yd/462 yd/468 yd; elev ?/?/? — The fairway is not flat, but the best and easiest approach
- **H16: P3** — 209 yd/219 yd/235 yd; elev ?/?/? — If you tomahawk, you will likely hit the top of the arch. A 1w with a backpowerspin will go under easily, so will a cobra with a lower club
- **H17: P4** — 438 yd/451 yd/?; elev ?/?/? — Try to shoot far to get all the way around the windmill. If you have low power, just make sure to time your approach shot carefully
- **H18: P5** — 332 yd/341 yd/356 yd; elev ?/?/? — Either hit onto the rough directly ahead, then tomahawk down the canyon, or tomahawk onto the rough island, then tomahawk again to the green. Players with enough drive might not need to use a tomahawk to get to the island.

### White Wiz
#### [White Wiz Front 9](https://pangya.wiki/wiki/White_Wiz_Front_9)
- **H1: P4** — 396 yd/406 yd/419 yd; elev ?/?/? — No hint text in source.
- **H2: P3** — 168 yd/168 yd/181 yd; elev ?/?/? — No hint text in source.
- **H3: P4** — 398 yd/403 yd/410 yd; elev ?/?/? — No hint text in source.
- **H4: P5** — 470 yd/473 yd/481 yd; elev ?/?/? — No hint text in source.
- **H5: P4** — 414 yd/422 yd/427 yd; elev ?/?/? — No hint text in source.
- **H6: P5** — 497 yd/499 yd/503 yd; elev ?/?/? — No hint text in source.
- **H7: P4** — 420 yd/424 yd/441 yd; elev ?/?/? — No hint text in source.
- **H8: P4** — 398 yd/403 yd/415 yd; elev ?/?/? — No hint text in source.
- **H9: P3** — 231 yd/237 yd/237 yd; elev ?/?/? — No hint text in source.

#### [White Wiz Back 9](https://pangya.wiki/wiki/White_Wiz_Back_9)
- **H10: P5** — 482 yd/488 yd/494 yd; elev ?/?/? — No hint text in source.
- **H11: P4** — 383 yd/392 yd/396 yd; elev ?/?/? — No hint text in source.
- **H12: P3** — 191 yd/198 yd/201 yd; elev ?/?/? — No hint text in source.
- **H13: P4** — 387 yd/387 yd/393 yd; elev ?/?/? — No hint text in source.
- **H14: P5** — 584 yd/592 yd/600 yd; elev ?/?/? — No hint text in source.
- **H15: P4** — 446 yd/451 yd/465 yd; elev ?/?/? — No hint text in source.
- **H16: P3** — 165 yd/169 yd/178 yd; elev ?/?/? — No hint text in source.
- **H17: P4** — 452 yd/453 yd/466 yd; elev ?/?/? — No hint text in source.
- **H18: P4** — 357 yd/366 yd/389 yd; elev ?/?/? — No hint text in source.

## Courses without walkthrough articles in this corpus
No Front/Back walkthrough data can be recovered for: [Deep Inferno](https://pangya.wiki/wiki/Deep_Inferno), [Eastern Valley](https://pangya.wiki/wiki/Eastern_Valley), [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac), [Ice Inferno](https://pangya.wiki/wiki/Ice_Inferno), [Mystic Ruins](https://pangya.wiki/wiki/Mystic_Ruins), [Wind Hill](https://pangya.wiki/wiki/Wind_Hill), [Wiz City](https://pangya.wiki/wiki/Wiz_City), [Wiz Wiz](https://pangya.wiki/wiki/Wiz_Wiz).

## All analyzed titles (47)
- Overview: [Abbot Mine](https://pangya.wiki/wiki/Abbot_Mine)
- Overview: [Blue Lagoon](https://pangya.wiki/wiki/Blue_Lagoon)
- Overview: [Blue Moon](https://pangya.wiki/wiki/Blue_Moon)
- Overview: [Blue Water](https://pangya.wiki/wiki/Blue_Water)
- Overview: [Deep Inferno](https://pangya.wiki/wiki/Deep_Inferno)
- Overview: [Eastern Valley](https://pangya.wiki/wiki/Eastern_Valley)
- Overview: [Grand Zodiac](https://pangya.wiki/wiki/Grand_Zodiac)
- Overview: [Ice Cannon](https://pangya.wiki/wiki/Ice_Cannon)
- Overview: [Ice Inferno](https://pangya.wiki/wiki/Ice_Inferno)
- Overview: [Ice Spa](https://pangya.wiki/wiki/Ice_Spa)
- Overview: [Lost Seaway](https://pangya.wiki/wiki/Lost_Seaway)
- Overview: [Mystic Ruins](https://pangya.wiki/wiki/Mystic_Ruins)
- Overview: [Pink Wind](https://pangya.wiki/wiki/Pink_Wind)
- Overview: [Sepia Wind](https://pangya.wiki/wiki/Sepia_Wind)
- Overview: [Shining Sand](https://pangya.wiki/wiki/Shining_Sand)
- Overview: [Silvia Cannon](https://pangya.wiki/wiki/Silvia_Cannon)
- Overview: [West Wiz](https://pangya.wiki/wiki/West_Wiz)
- Overview: [White Wiz](https://pangya.wiki/wiki/White_Wiz)
- Overview: [Wind Hill](https://pangya.wiki/wiki/Wind_Hill)
- Overview: [Wiz City](https://pangya.wiki/wiki/Wiz_City)
- Overview: [Wiz Wiz](https://pangya.wiki/wiki/Wiz_Wiz)
- Walkthrough: [Abbot Mine Front 9](https://pangya.wiki/wiki/Abbot_Mine_Front_9)
- Walkthrough: [Abbot Mine Back 9](https://pangya.wiki/wiki/Abbot_Mine_Back_9)
- Walkthrough: [Blue Lagoon Back 9](https://pangya.wiki/wiki/Blue_Lagoon_Back_9)
- Walkthrough: [Blue Lagoon Front 9](https://pangya.wiki/wiki/Blue_Lagoon_Front_9)
- Walkthrough: [Blue Moon Front 9](https://pangya.wiki/wiki/Blue_Moon_Front_9)
- Walkthrough: [Blue Moon Back 9](https://pangya.wiki/wiki/Blue_Moon_Back_9)
- Walkthrough: [Blue Water Front 9](https://pangya.wiki/wiki/Blue_Water_Front_9)
- Walkthrough: [Blue Water Back 9](https://pangya.wiki/wiki/Blue_Water_Back_9)
- Walkthrough: [Ice Spa Back 9](https://pangya.wiki/wiki/Ice_Spa_Back_9)
- Walkthrough: [Ice Cannon Front 9](https://pangya.wiki/wiki/Ice_Cannon_Front_9)
- Walkthrough: [Ice Cannon Back 9](https://pangya.wiki/wiki/Ice_Cannon_Back_9)
- Walkthrough: [Ice Spa Front 9](https://pangya.wiki/wiki/Ice_Spa_Front_9)
- Walkthrough: [Lost Seaway Front 9](https://pangya.wiki/wiki/Lost_Seaway_Front_9)
- Walkthrough: [Lost Seaway Back 9](https://pangya.wiki/wiki/Lost_Seaway_Back_9)
- Walkthrough: [Pink Wind Front 9](https://pangya.wiki/wiki/Pink_Wind_Front_9)
- Walkthrough: [Pink Wind Back 9](https://pangya.wiki/wiki/Pink_Wind_Back_9)
- Walkthrough: [Sepia Wind Front 9](https://pangya.wiki/wiki/Sepia_Wind_Front_9)
- Walkthrough: [Sepia Wind Back 9](https://pangya.wiki/wiki/Sepia_Wind_Back_9)
- Walkthrough: [Shining Sand Front 9](https://pangya.wiki/wiki/Shining_Sand_Front_9)
- Walkthrough: [Shining Sand Back 9](https://pangya.wiki/wiki/Shining_Sand_Back_9)
- Walkthrough: [Silvia Cannon Front 9](https://pangya.wiki/wiki/Silvia_Cannon_Front_9)
- Walkthrough: [Silvia Cannon Back 9](https://pangya.wiki/wiki/Silvia_Cannon_Back_9)
- Walkthrough: [West Wiz Back 9](https://pangya.wiki/wiki/West_Wiz_Back_9)
- Walkthrough: [West Wiz Front 9](https://pangya.wiki/wiki/West_Wiz_Front_9)
- Walkthrough: [White Wiz Front 9](https://pangya.wiki/wiki/White_Wiz_Front_9)
- Walkthrough: [White Wiz Back 9](https://pangya.wiki/wiki/White_Wiz_Back_9)

## Review findings and residual risks
- **warning — par conflicts:** walkthrough vs overview conflicts occur at Abbot Mine H7 (P5 vs P4), H9 (P4 vs P5), H14 (P4 vs P5); Blue Water H18 (P18 vs P4, almost certainly a typo); and West Wiz H14 (P4 vs P3). Each is flagged inline. Treat the overview par matrix as the catalogue authority unless intentionally reproducing walkthrough data.
- **warning — Eastern Valley:** its overview par row totals 71, unlike most normal courses’ 72 (Abbot Mine totals 73). This is reported as stored, not normalized.
- **warning — corpus walkthrough coverage:** many tables contain `?`, empty yard values, `N/A`, or missing third pins; most elevations outside Abbot Mine/Blue Lagoon/Blue Moon are unknown. Do not coerce these to zero.
- **warning — overview links:** Mystic Ruins advertises Front/Back walkthrough links, but neither article exists in the AllPages corpus. Seven other normal courses also lack walkthrough-category articles.
- **info — source freshness:** title URLs are provided for citation, but this analysis attests to corpus revision content only; live pangya.wiki content may have changed.
