# Patch history synthesis

This document distills durable systems, content milestones, operational changes, and data-model implications from all 295 patch-note articles in the wiki corpus. Transient sales are omitted unless they reveal a reusable live-operations rule. The exhaustive page-level inventory is in [SOURCE_COVERAGE.md](SOURCE_COVERAGE.md).

> **Scope warning:** `Category:Global Patch Notes` contains 193 North-American/US pages, not a universal all-region ledger. The 102 Japanese pages have major chronological gaps and several stub-only entries. Treat both as region-specific, incomplete evidence.

## North America / “Global” patch notes

### Scope and method

All **193** `Category:Global Patch Notes` pages were read. Infobox publication dates, patch identifiers, regions, and bodies were normalized chronologically, then durable mechanics/content were separated from temporary sales and reward windows. Every source page is attested in [SOURCE_COVERAGE.md](SOURCE_COVERAGE.md).

**Coverage:** 2009-04-21 through 2016-04-27 (7 years, 6 days). Every page's infobox says `region=[[North America|US]]`. Thus “Global” means the North American/English service branding—not evidence that a change shipped in Korea, Japan, Thailand, or every region. The notes reference Ntreev USA, then SG Interactive/GameRage, use PST/PDT, enforce English in public channels, and occasionally discuss “Team Global” at international competition ([R3.433.01](https://pangya.wiki/wiki/GB.R3.433.01), [R4.553.01](https://pangya.wiki/wiki/GB.R4.553.01)).

### Release-family chronology

| Family | Pages | Date span | Durable release story |
|---|---:|---|---|
| R3 | 5 | 2009-04-21–2009-06-25 | Service stabilization and monetization foundation; Jump-In/Tiki Report; Scratchy arrives; Season 4 announced. |
| R4 | 59 | 2009-07-15–2011-04-20 | Season 4 feature build-out: creation, ghosts, cards, battle, Gacha, shops, guilds, cut-ins; Eastern Valley; Nell/4.5. |
| R5 | 57 | 2011-04-27–2013-05-08 | United through Tomahawk/Challenge: Tiki recycling, ranking, two courses, graphics, Grand Zodiac, practice tools, achievements/artifacts/quests, Spika. |
| R6/Q6 | 21 | 2013-05-29–2014-03-20 | Late Season 6: Roi dispatch, Abbot Mine, Club Workshop, attendance, richer crafting, item-effect ecosystem; Player Shops disabled. |
| R7 | 51 | 2014-04-03–2016-04-27 | Grand Prix + Natural Mode + Memorial economy; Fresh Up/remodels and UI; then event/Grand-Prix maintenance cadence, server consolidation, permanent Gacha-rate increase. |

#### R3 — service foundation (2009-04-21 to 2009-06-25)

- **2009-04-21:** initial corpus page is mostly crash/login/server fixes ([R3.432.01](https://pangya.wiki/wiki/GB.R3.432.01)).
- **2009-05-21 is the first high-impact economy baseline:** premium shop and in-client recharge opened; gifting premium items required Beginner E and friendship; decorations switched to Pang and were discounted. More dramatically, balances at/above 100,000 Pang were cut to 100,000 and Pang-item gifting was permanently disabled. Servers were segmented into rookie/open roles, with a rookie EXP bonus and increased capacity ([R3.433.01](https://pangya.wiki/wiki/GB.R3.433.01)). This is a migration/security event that an emulator or historical dataset should model explicitly, not as ordinary balance.
- **2009-06-11:** **Jump-In Mode** let players join a tournament within its first five minutes. **Tiki Report Scroll** let users leave a finished tournament and receive results later, supported by a Reports mailbox tab ([R3.434.01](https://pangya.wiki/wiki/GB.R3.434.01)).
- **2009-06-25:** **Scratchy Card** launched as a spend-derived draw (one card per 1,000 premium points spent), with rotating rare pools ([R3.435.00](https://pangya.wiki/wiki/GB.R3.435.00)). This becomes a recurring secondary loot system through 2016.

#### R4 — Season 4 systems and social play (2009-07-15 to 2011-04-20)

- **2009-07-30:** two major systems arrived together. **Self Design** uses editable premium blanks and non-editable Pang blanks that can receive copies; saved designs cannot be edited. **Ghost System** records an 18-hole run and permits profile-launched Ghost VS. The same patch changes Lost Seaway collision and reports server/messenger stability work ([R4.502.01](https://pangya.wiki/wiki/GB.R4.502.01)).
- **2009-08-13 to 09-10:** **Card Holic Vol. 1** (character/caddie cards applied from packs), **Approach/Pang Battle**, and then **Pangya Gacha** launched ([R4.503.01](https://pangya.wiki/wiki/GB.R4.503.01), [R4.504.01](https://pangya.wiki/wiki/GB.R4.504.01), [R4.505.01](https://pangya.wiki/wiki/GB.R4.505.01)). These establish the card-slot and premium rare economies used by later club patching and Memorial systems.
- **2010-01-13:** **Personal Shops** became available ([R4.521.01](https://pangya.wiki/wiki/GB.R4.521.01)); permanent card-ticket drops from Treasure Boxes followed on 2010-01-20 ([R4.522.01](https://pangya.wiki/wiki/GB.R4.522.01)).
- **2010-04-29:** **Eastern Valley** opened. The prior spring tour counted 15 courses, so this appears to be course 16 even though this page does not number it ([R4.531.01](https://pangya.wiki/wiki/GB.R4.531.01)).
- **2010-05-13:** nickname changes moved into Settings ([R4.532.01](https://pangya.wiki/wiki/GB.R4.532.01)). **2010-07-29:** the **Cut-In** illustration system arrived ([R4.539.01](https://pangya.wiki/wiki/GB.R4.539.01)).
- **2010-09-08 to 10-07:** attendance stamps, then the **Guild** system, guild creator/designer/editor, guild chat, and guild battles were progressively enabled ([R4.540.04](https://pangya.wiki/wiki/GB.R4.540.04), [R4.541.02](https://pangya.wiki/wiki/GB.R4.541.02), [R4.542.01](https://pangya.wiki/wiki/GB.R4.542.01), [R4.543.01](https://pangya.wiki/wiki/GB.R4.543.01)).
- **2010-12-08:** server lists became location-dependent ([R4.549.01](https://pangya.wiki/wiki/GB.R4.549.01)). **2010-12-21:** “Season 4.5” launched with **Nell**, bringing the character roster to ten by later notes ([R4.550.01](https://pangya.wiki/wiki/GB.R4.550.01)).
- **Operational durability:** GameGuard/launcher security was updated (2010-02-24); mail attachments were capped at 99 (2010-03-29); Dolfini Locker passcodes became support-recoverable (2010-11-09). These constraints should be represented separately from content ([R4.525.01](https://pangya.wiki/wiki/GB.R4.525.01), [R4.528.01](https://pangya.wiki/wiki/GB.R4.528.01), [R4.547.01](https://pangya.wiki/wiki/GB.R4.547.01)).

#### R5 — United, Tomahawk, and Challenge (2011-04-27 to 2013-05-08)

- **2011-04-27 Pangya United:** mailbox caddies were migrated to inventory; several course cards were disabled and Ghost Mode was unavailable ([R5.601.09](https://pangya.wiki/wiki/GB.R5.601.09)). The corpus never records Ghost Mode being restored, so its later state is unknown.
- **2011-06-08:** **Tiki’s Point Shop** opened, converting unwanted items into Tiki Points ([R5.604.01](https://pangya.wiki/wiki/GB.R5.604.01)). It later becomes an explicit recycling/mileage sink.
- **2011-08-25:** in-game rankings added ([R5.610.01](https://pangya.wiki/wiki/GB.R5.610.01)). **2011-10-19:** graphics engine upgrade improved transitions, shadows, water, characters, and transparency and raised the documented baseline to Pentium 4, 512 MB, GeForce FX, Windows XP/Vista/7, DirectX 9.0c ([R5.614.01](https://pangya.wiki/wiki/GB.R5.614.01)).
- **Course growth:** **Wiz City**, explicitly the 17th course, opened 2011-11-09 with a neutral Dolfini server ([R5.616.01](https://pangya.wiki/wiki/GB.R5.616.01)); **Ice Inferno**, explicitly the 18th, arrived 2012-02-15 ([R5.623.00](https://pangya.wiki/wiki/GB.R5.623.00)). Smart View gained Shift+right-click target zoom on 2012-02-01 ([R5.622.00](https://pangya.wiki/wiki/GB.R5.622.00)).
- **2012-06-07 Tomahawk launch:** **Grand Zodiac/Hole-In-One Mode**, free choice of starter character, improved item UI, `/action` commands, Papel Shop auto mode, Card Pack #3, **Air Note** previous-shot visualization, and **Chip-In Practice Tickets** arrived. Grand Zodiac is schedule-gated and point/reward driven ([R5.632.00](https://pangya.wiki/wiki/GB.R5.632.00)). Player-shop prices had just been capped at 10,000,000 ([R5.633.00](https://pangya.wiki/wiki/GB.R5.633.00)); Spinning Cube inventory rose from 50 to 100 ([R5.636.00](https://pangya.wiki/wiki/GB.R5.636.00)).
- **2012-09-26 “Second Journey”:** **Hole Repeat Mode**, **Club Card Patching**, Self Design shortcuts/long dresses, revised experience requirements, and revised level-up prizes arrived. Existing players could jump levels and higher ranks received migration rewards ([R5.641.00](https://pangya.wiki/wiki/GB.R5.641.00)).
- **2012-12-11 FastPass:** new and returning players could earn large rank/EXP boosts and equipment; claiming the boost suppresses intermediate level rewards and initially miscalculated some high-level accounts ([R5.647.00](https://pangya.wiki/wiki/GB.R5.647.00)). Treat as a progression migration, not a repeatable reward.
- **2013-01-09 to 01-24:** Spika was introduced by a hole-completion event, then formally supported by shop/Gacha. **Challenge** systems launched: achievements and achievement points, artifact room modifiers fueled by randomly dropped matching mana, daily quests with a ten-quest box, and Lolo’s Card Deck card recombination. Hit-bar positioning/size, inventory/shop search, and season-record reset also arrived ([R5.649.00](https://pangya.wiki/wiki/GB.R5.649.00), [R5.701.00](https://pangya.wiki/wiki/GB.R5.701.00)).
- **2013-03-27:** achievements were expanded and requirements retroactively reduced; decorative rewards and two artifacts were added: Dragon Orb (rain chance) and Frozen Flame (retain active items after use) ([R5.706.00](https://pangya.wiki/wiki/GB.R5.706.00)).

#### R6/Q6 — Club Workshop era (2013-05-29 to 2014-03-20)

The family’s identifier changes from `GB.R6.710–712` to `GB.Q6.713–724`, then back to `GB.R6.725–730`; no page explains `Q6`. It should be stored as the literal build family, while the product season remains Season 6.

- **2013-05-29:** Spika’s surveying event is the clearest description of **Roi dispatch**: send Rois to courses for drops; a missing Roi must be recovered by playing that course. Rewards include Vol. 3 Card Holic cards ([R6.710.00](https://pangya.wiki/wiki/GB.R6.710.00)).
- **2013-08-22 is the family’s key durable patch:** **Abbot Mines** was added as the apparent 19th course; **Club Workshop** added club Modify, Rank Up, and Maintain; attendance rewards were introduced. Supporting UCIM Chips, coating spray, modification/maintenance items, Abbot crystals, elemental shards, and Soren rewards form a club-upgrade economy ([Q6.716.00](https://pangya.wiki/wiki/GB.Q6.716.00)).
- **Club state management:** mastery can be doubled by events; UCIM Chips transfer mastery; Abbot Coating Spray restores recovery points; Titan Cleansing Powder resets upgraded clubs to defaults ([Q6.718.00](https://pangya.wiki/wiki/GB.Q6.718.00), [Q6.723.00](https://pangya.wiki/wiki/GB.Q6.723.00), [R7.812.00](https://pangya.wiki/wiki/GB.R7.812.00)). An implementation needs club rank/level/stats, mastery, recovery points, cards, and reset/transfer operations as distinct fields.
- **2013-09-25:** Spika gained Self Design blanks/dupes, completing feature parity ([Q6.718.00](https://pangya.wiki/wiki/GB.Q6.718.00)). **2013-10-10:** Panda Ninja Kun became a permanent mascot; item equipment could add power-shot cut-ins and lounge idle animations ([Q6.719.00](https://pangya.wiki/wiki/GB.Q6.719.00)).
- **2014-01-08 to 03-06:** high-impact premium items increasingly manipulate the simulation: wind direction, impact zone, item drop/Treasure rates, rain probability/duration, zero-wind chance, and power gauge ([R6.725.00](https://pangya.wiki/wiki/GB.R6.725.00), [R6.727.00](https://pangya.wiki/wiki/GB.R6.727.00), [R6.729.00](https://pangya.wiki/wiki/GB.R6.729.00)). Effects need typed triggers and stacking rules, not flavor-text blobs.
- **2014-03-20:** Player Shops were disabled “until further notice” ([R6.730.00](https://pangya.wiki/wiki/GB.R6.730.00)); they do not return until 2015-04-15 ([R7.825.00](https://pangya.wiki/wiki/GB.R7.825.00)).

#### R7 — Grand Prix, Fresh Up, consolidation (2014-04-03 to 2016-04-27)

- **2014-04-03 Season 7:** **Grand Prix** introduced scheduled tournaments against players and AI, Event/Novice/Class 1–3 categories, special rules, first-time top-three rewards, records/trophies, and ticket admission. Players receive three tickets daily, can earn more based on completed multiplayer hole count, and cap at 50. **Natural Mode** hides exact wind behind green/yellow/red ranges and allows slight direction changes ([R7.801.00](https://pangya.wiki/wiki/GB.R7.801.00)).
- **2014-04-17:** **Memorial Shop** opened. Normal coins use Achievement Point-derived Memorial Level to govern eligible historical Gacha pools and rates; premium coins bypass level and offer higher rare rates; special coins target Scratchy/character/event pools ([R7.802.00](https://pangya.wiki/wiki/GB.R7.802.00)). This closes the loop achievements → Memorial level → legacy rare acquisition.
- **2014-09-29 Fresh Up:** announced a new interface, modes, and features, with onboarding/loyalty/team seasons ([R7.813.00](https://pangya.wiki/wiki/GB.R7.813.00)). The page delegates details externally, so the corpus alone cannot enumerate the launch delta. Subsequent pages establish remodeled **Nuri R/Hana R**, expanded Tiki recycling, character mastery’s third Caddie Card slot, seasonal Grand Prix maps, and later **Cecilia R** ([R7.814.00](https://pangya.wiki/wiki/GB.R7.814.00), [R7.815.00](https://pangya.wiki/wiki/GB.R7.815.00), [R7.820.00](https://pangya.wiki/wiki/GB.R7.820.00)).
- **2014-12-10 UI bundle:** accept-all mailbox action, top-toolbar tooltips, Grand Prix potion equip, quest shortcuts, shopping-cart visibility changes, Assist-mode Auto Calipers support, and self-service Dolfini Locker password changes ([R7.818.00](https://pangya.wiki/wiki/GB.R7.818.00)).
- **2015 onward:** Grand Prix becomes the default delivery surface in almost every patch, usually with rotating Memorial coins, course/rule schedules, or weekend Pang/EXP rooms. This is durable framework but transient configuration; model tournaments and reward pools as data rather than hard-coded modes.
- **2015-10-21:** servers were merged to only **Lolo** and **Dolfini**, both open to all; Scratchy and Premium Memorial rates doubled ([R7.838.02](https://pangya.wiki/wiki/GB.R7.838.02)). **2015-11-04:** Gacha’s increased rate became permanent and the spend yield became four Scratchy Cards per 1,000 points ([R7.839.00](https://pangya.wiki/wiki/GB.R7.839.00)). These are major economy/live-ops state changes.
- **Final corpus page, 2016-04-27:** only a Grand Prix Bronze Card event and shop price fixes ([R7.851.00](https://pangya.wiki/wiki/GB.R7.851.00)). It is not evidence that development or service ended that day—only that this category snapshot ends there.

### Cross-family recurring systems and implementation implications

| System | First clear evidence | Last/late evidence | Durable model implication |
|---|---|---|---|
| Shop currencies/gifting | Premium shop, points, gifting rules, Pang reset: 2009-05-21 | Permanent shop additions: 2016-03-30 | Separate Pang/premium balances, giftability, rank gates, regional price catalog, migration ledger. |
| Cadie’s Cauldron crafting | Already “open again” 2009-10-29, proving pre-corpus origin | New cube recipes 2016-04-15 | Versioned recipes, availability windows, ownership limits, exchange/color transformations, compensation on removal. |
| Papel Shop | Update #1, 2009-05-21 | Cross Wings, 2016-02-17 | Separate draw inventory/pool/version; auto mode from 2012; coupons/boxes and rare rotations. |
| Scratchy | Launch 2009-06-25 | Pool update 2016-02-03; four-per-1,000 rule late 2015 | Spend-triggered ticket accrual plus versioned rare pools and rate multipliers. |
| Gacha | Launch 2009-09-10 | Obsidian Wings 2016-03-16 | Versioned pools, bundles, pity/bonus promotions, exchange variants, effective-date rate table. |
| Personal Shops/mail | Shops 2010-01-13; attachment cap 2010-03-29 | Disabled 2014-03-20; restored 2015-04-15 | Trade/mail eligibility per item, price caps, attachment caps, global service toggle. |
| Cards | Card Holic 2009-08-13 | Club cards, caddie slots, synthesis and ticket rewards throughout | Card volume/rarity/type, socket target, timers/override rules, synthesis odds, patch/removal. |
| Spinning Cubes | “Now appearing ... as well” 2011-12-22 (origin missing) | New rare/recipes 2016-04-15 | Course drop map, unopened/opened item distinction, capacity, rare pool and exchange recipes. |
| Attendance/daily missions | Attendance stamps 2010-09-08 | Login-and-play events 2016-03-02 | Calendar/day boundary in PST/PDT, per-day qualification, streak/milestone rewards. |
| Quit-rate remediation | Event by 2010-02-24 | Reduced for event participants 2016-01-06 | Quit count/rate, qualifying completed time, good-standing alternative reward, batch adjustment audit. |
| Courses/tours | 14-course tour 2009-05; 15 by 2010-03 | 19-course Grand Tour 2014-03 | Course catalog and release dates; events refer to snapshots of active catalog. |
| Club upgrades | Club Card Patching 2012-09 | Workshop rewards continue 2016 | Club base stats vs rank/level/modifiers, mastery, recovery, card sockets, transfer/reset operations. |
| Achievement/quest/artifact | 2013-01-24 | Achievement points drive Memorial Level from 2014 | Versioned achievement thresholds; retroactive migration; daily quest state; artifacts + mana + room modifier. |
| Grand Prix/Memorial | 2014-04-03/17 | Every late patch through 2016-04-27 | Schedule/rules/tickets/AI/class/trophy models; normal/premium/special coins, Memorial level and historical pools. |
| Character remodels/mastery | Fresh Up aftermath, 2014-09 onward | Classic/remodeled bundles continue 2016 | Treat classic and R forms as linked character variants with explicit item compatibility/bundling. |

#### Event framework patterns worth implementing generically

1. **Hole-completion counters:** per day, total window, exact course tours, and mode exclusions recur constantly.
2. **Hole drops:** every third/last/9th/18th hole, probability vs guarantee, course difficulty weighting, equipped-item multipliers, and inventory conversion on expiry.
3. **Login calendars:** one-time, daily, consecutive-day, N-distinct-days, plus already-owned substitution.
4. **Community counters and teams:** shared stage thresholds, individual ladders, red/blue seasonal assignments.
5. **Timed mode schedules:** Grand Zodiac and Grand Prix rooms have timezone-sensitive open periods and special rules.
6. **Craft/exchange pipelines:** raw drop → box/token → Cauldron recipe → random pool or deterministic item; recipes frequently expire separately from drops.
7. **Compensation/migration:** invalid cards replaced, missing/lapsed items converted to Pang/comets, EXP recalculated, rewards re-distributed. Preserve idempotent administrative grants and reason codes.

### Content milestones

- Playable roster: Lucia was teased for Season 4 in June 2009; Nell launched with 4.5 in December 2010; Spika launched January 2013. Later remodel variants Nuri R/Hana R/Cecilia R are not separate wholly independent content in item bundles.
- Course-count evidence is inconsistent: events count **14** courses in May 2009 and **15** in March 2010; the Delight season page calls Lost Seaway #16 and Eastern Valley #17, while the Global chronology implies Eastern Valley #16 and [GB.R5.616.01](https://pangya.wiki/wiki/GB.R5.616.01) also calls Wiz City #17. Later notes call Ice Inferno #18 and Abbot Mine #19. Do not derive stable course IDs from these ordinal claims.
- Modes: Tournament/Versus baseline → Jump-In, Ghost VS, Approach/Pang Battle, guild battle, Special Shuffle references, Grand Zodiac/HIO, Hole Repeat/Chip-In Practice, Grand Prix, Natural Mode.
- Social/UI: friends and messenger baseline, Personal Shops, guild/chat/battle, nickname changes, Dolfini Locker, search, adjustable hit bar, mailbox accept-all, lounge actions, character cut-ins.

### Contradictions, gaps, and data-quality findings

- **High — incomplete release sequence:** the category contains only its captured members, not a complete build ledger. Missing identifiers include R4.509, .511–.513, .515, .527; R5.631; and R7.836, plus many early R3 builds. Patch suffixes (`.00/.01/.02/.04/.09`) also vary. Do not infer “no change” for missing builds.
- **High — regional naming risk:** all 193 pages are US-region infoboxes. “Global” cannot safely seed other regional timelines; item availability, dates, rules, and naming may differ.
- **High — Fresh Up under-specified:** [R7.813.00](https://pangya.wiki/wiki/GB.R7.813.00) claims new interface/modes/features but redirects readers to an event page absent from the corpus. Remodel/mastery details must be reconstructed from later notes or another source.
- **Medium — course ordinals conflict:** Delight labels Lost Seaway #16 and Eastern Valley #17, but Global patch evidence implies Eastern Valley #16 and labels Wiz City #17. Treat course titles/content IDs as authoritative and ordinals as unresolved regional or editorial claims.
- **Medium — Ghost state gap:** Ghost launched [R4.502.01](https://pangya.wiki/wiki/GB.R4.502.01), was unavailable at United [R5.601.09](https://pangya.wiki/wiki/GB.R5.601.09), and has no restoration note here.
- **Medium — mode/shop origin gaps:** Cauldron was “open again” in October 2009 and Spinning Cubes were “now appearing ... as well” in December 2011; their true launch pages are absent.
- **Medium — explicit note defects:** [R6.728.00](https://pangya.wiki/wiki/GB.R6.728.00) and [R7.805.00](https://pangya.wiki/wiki/GB.R7.805.00) duplicate the 600–899 Play-to-Win tier; [Q6.723.00](https://pangya.wiki/wiki/GB.Q6.723.00) labels both Kaz title and mask as Day 3; [R7.842.00](https://pangya.wiki/wiki/GB.R7.842.00) says Kaz’s coin wins “Azer” rares; [R7.837.00](https://pangya.wiki/wiki/GB.R7.837.00) calls Nuri’s coin `(M)` once. Preserve source text but flag rather than encode literally.
- **Medium — impossible/stale window:** [R7.804.00](https://pangya.wiki/wiki/GB.R7.804.00), published 2014-05-21, lists Spika’s sale as 04/29–05/13 while its login event is 05/22–06/02.
- **Medium — questionable year range:** [R5.649.00](https://pangya.wiki/wiki/GB.R5.649.00) says logins between `1/05/2012` and `1/08/2013` qualified for a quit reset. This may be a typo, but the corpus cannot resolve it.
- **Low — duplicate/repeated launch wording:** [R4.504.01](https://pangya.wiki/wiki/GB.R4.504.01) repeats “Card Holic System has been implemented” from [R4.503.01](https://pangya.wiki/wiki/GB.R4.503.01); treat the earlier date as first evidence.
- **Low — publication order:** corpus array order starts in July 2013, jumps to 2009, and then resumes; infobox date, not array position or wiki revision timestamp (mostly 2024), is the release date.

### Highest-value pages for implementation

1. [GB.R3.433.01](https://pangya.wiki/wiki/GB.R3.433.01) — launch economy, permanent gifting rule, balance reset, server segmentation.
2. [GB.R4.502.01](https://pangya.wiki/wiki/GB.R4.502.01) — full Self Design and Ghost semantics.
3. [GB.R4.503.01](https://pangya.wiki/wiki/GB.R4.503.01), [R4.504.01](https://pangya.wiki/wiki/GB.R4.504.01), [R4.505.01](https://pangya.wiki/wiki/GB.R4.505.01) — Card Holic, battle, and Gacha foundations.
4. [GB.R5.614.01](https://pangya.wiki/wiki/GB.R5.614.01) — graphics-engine behavior and minimum requirements.
5. [GB.R5.632.00](https://pangya.wiki/wiki/GB.R5.632.00) — Tomahawk mode/UI/practice feature bundle.
6. [GB.R5.641.00](https://pangya.wiki/wiki/GB.R5.641.00) — Hole Repeat, club card patching, progression migration.
7. [GB.R5.701.00](https://pangya.wiki/wiki/GB.R5.701.00) — achievements, artifacts/mana, quests, synthesis, search/settings.
8. [GB.Q6.716.00](https://pangya.wiki/wiki/GB.Q6.716.00) — Abbot Mine + Club Workshop + attendance.
9. [GB.R7.801.00](https://pangya.wiki/wiki/GB.R7.801.00) — authoritative Grand Prix/Natural Mode/ticket rules.
10. [GB.R7.802.00](https://pangya.wiki/wiki/GB.R7.802.00) — Memorial Shop and achievement-linked level rules.
11. [GB.R7.818.00](https://pangya.wiki/wiki/GB.R7.818.00) — late UI/Assist/mailbox/locker feature set.
12. [GB.R7.838.02](https://pangya.wiki/wiki/GB.R7.838.02) and [R7.839.00](https://pangya.wiki/wiki/GB.R7.839.00) — server consolidation and permanent draw-economy changes.

---

## Japan patch notes

### Scope and method

All **102** `Category:Japan Patch Notes` pages were read, including repeated notices and defect-only stubs. The chronology intentionally excludes ordinary limited sales, routine birthday rewards, weekly CP-purchase bonuses, and repeated reminders unless they expose a durable mechanic, content milestone, balance rule, or Japanese operating model. Dates are infobox `publishDate`; when a note names a different effective date, both are retained. Each wiki page also embeds its archived `pangya.jp` source URL.

A crucial implementation rule is to model **note publication date**, **patch identifier**, and **feature effective date** separately. Several pages are retrospectives, previews, or corrections rather than the actual launch note.

### Implementation-oriented chronology

#### 3.x–4.x — early Japanese service (2005–2007; mostly archival stubs)

- **2005-12-29–2006-06-15, 3.08a–3.16b:** the four surviving pages only identify defects/client adjustment; their actual details remain behind archived links and are not present in the corpus. Do not infer mechanics from them. [JP.3.08a](https://pangya.wiki/wiki/JP.3.08a), [JP.3.08b](https://pangya.wiki/wiki/JP.3.08b), [JP.3.10b](https://pangya.wiki/wiki/JP.3.10b), [JP.3.16b](https://pangya.wiki/wiki/JP.3.16b)
- **2007-02-01, 4.00j:** server/channel names were changed—early evidence that Japanese server topology and display names are content/config data, not fixed client constants. [JP.4.00j](https://pangya.wiki/wiki/JP.4.00j)
- **2007-02-22, 4.01:** **Pink Wind** launched. This is the only durable course milestone recoverable from the 4.x stubs. [JP.4.01](https://pangya.wiki/wiki/JP.4.01)
- Remaining 4.x pages are defect or cosmetic/event headlines without body detail; preserve them as provenance, not implementation specifications. [JP.4.00l](https://pangya.wiki/wiki/JP.4.00l), [JP.4.04d](https://pangya.wiki/wiki/JP.4.04d), [JP.4.04f](https://pangya.wiki/wiki/JP.4.04f), [JP.4.06c](https://pangya.wiki/wiki/JP.4.06c), [JP.4.06d](https://pangya.wiki/wiki/JP.4.06d), [JP.4.07c](https://pangya.wiki/wiki/JP.4.07c), [JP.4.07d](https://pangya.wiki/wiki/JP.4.07d), [JP.4.07e](https://pangya.wiki/wiki/JP.4.07e)

#### 5.x — operational/social layer becomes visible (2009–2011)

- **2010-01 to 2010-03:** chat rooms included rate limiting and slash-command-driven cosmetic effects (`/quickmove`, `/bighead`, `/light`). User-created titles entered the game; the **Special Prize** system's rules changed (details linked but absent); retirement rate below 3% awarded a **Best Manner** icon. These are durable account/social-state concepts. [JP.5.55](https://pangya.wiki/wiki/JP.5.55), [JP.5.61](https://pangya.wiki/wiki/JP.5.61), [JP.5.63](https://pangya.wiki/wiki/JP.5.63)
- **2010-03-11, 5.64:** Japanese anti-cheat switched to **nProtect GameGuard**, subsequently receiving periodic updates. Treat anti-cheat as a versioned external subsystem with soft-failure reporting, not gameplay logic. [JP.5.64](https://pangya.wiki/wiki/JP.5.64), [JP.5.68](https://pangya.wiki/wiki/JP.5.68)
- **2010-03-25, 5.66:** Gacha result pages gained a “tweet” function, showing a web/social integration outside the game client. [JP.5.66](https://pangya.wiki/wiki/JP.5.66)
- **2010-06-03, 5.76:** server names and composition changed again; Best Manner awards continued. This reinforces configurable server layout and periodic account-derived badge assignment. [JP.5.76](https://pangya.wiki/wiki/JP.5.76)
- **2010-06-17, 5.78:** options gained a whole-screen processing setting. Client graphics/performance settings evolve independently from gameplay. [JP.5.78](https://pangya.wiki/wiki/JP.5.78)
- **2010-12-29–2011-02-24:** the inventory/card model is visible: Card Pack Vol.1, timed special cards installed into clothing, and effects that must cease/display correctly on expiry. **Ignition Booster** added +1 pixel to the impact zone. [JP.5.9924](https://pangya.wiki/wiki/JP.5.9924), [JP.5.9931](https://pangya.wiki/wiki/JP.5.9931)
- Defect pages show client hot patches could require restart, equipment slots prohibit/permit combinations, and display inventory can diverge from owned inventory. Model server-authoritative ownership separately from presentation and equip validation. [JP.5.71](https://pangya.wiki/wiki/JP.5.71), [JP.5.73](https://pangya.wiki/wiki/JP.5.73), [JP.5.74](https://pangya.wiki/wiki/JP.5.74)

#### 6.x / Tomahawk transition — spin cube and regional web economy (2011–2012)

- **2012-03-22, 6.49:** **Spin Cube** reward contents were rotatable live data; later pages repeatedly change courses and pools. Implement cube placement and loot tables as effective-dated configuration. [JP.6.49](https://pangya.wiki/wiki/JP.6.49)
- **2012-05-10, 6.56:** announced the 2012-05-31 major release-family transition **United → Tomahawk**. The actual launch note is missing from this category slice. [JP.6.56](https://pangya.wiki/wiki/JP.6.56)
- **2012-06-21, 7.04 (post-launch evidence):** Tomahawk-era **Hole-in-One Battle** could be globally suspended/re-enabled; modes named include Versus, Tournament, Special Shuffle, and HIO Battle. The page also fixes login-exit, random motion, and PP-gacha terminal-count bugs. [JP.7.04](https://pangya.wiki/wiki/JP.7.04)

#### 7.x / Challenges — progression, quests, artifacts, crafting (2012–2014)

- **2012-12-27, 7.30 — central milestone:** **Pangya Challenges** added character **Spika**, achievement-style **Yarikomi Pangya**, three daily quests refreshed at 00:00 (box every 10 completions), room **Artifacts** powered by mana dropped probabilistically on cup-in, and **Lolo's Card Synthesis** (input type/rarity affects output rarity). It also added item search, high-resolution shot-bar/wind scaling, level-based default course selection, and assist consumables for special-shot commands and trajectory hints. Dolph Safe navigation moved under My Room. This page defines multiple entities and relationships suitable for separate services/tables: mission progress, quest rotation, artifact ownership/effects/mana, and card recipes. [JP.7.30](https://pangya.wiki/wiki/JP.7.30)
- **2013-02:** single-play exposes **Course Practice** and **Repeat One Hole**. A routing bug selected Blue Moon instead of Ice Inferno; another patch restored shot-bar size at specific resolutions. Course IDs must not be keyed by display order/name, and UI scaling requires regression tests. [JP.7.36](https://pangya.wiki/wiki/JP.7.36), [JP.7.37](https://pangya.wiki/wiki/JP.7.37), [JP.7.38](https://pangya.wiki/wiki/JP.7.38)
- **2013-03-22, 7.41:** UCC Web Shop could be disabled independently and resumed after repair—evidence for a region-specific external user-content commerce service and feature flag. [JP.7.41](https://pangya.wiki/wiki/JP.7.41)
- **2013-08-01, 7.60 — central milestone:** new course **Abbot Mine** (source transliterates エボートマイン), **Club Set Workshop** using Evote/Abbot energy to strengthen clubs and raise stats, plus Card Pack Vol.4. The same release **removed Ghost Mode** and stopped Fortune Key login rewards. Treat club base stats, enhancement state/resources, and mastery as distinct; removal flags matter for legacy data. [JP.7.60](https://pangya.wiki/wiki/JP.7.60)
- **2013-08-22–29:** Spika gained motions; when a character has more than 14, a random subset is displayed. A looping rhythm motion was added. This is a concrete client UI limit/selection rule. [JP.7.63](https://pangya.wiki/wiki/JP.7.63), [JP.7.64](https://pangya.wiki/wiki/JP.7.64)
- Equipment increasingly carries conditional gameplay effects: separate left-hand rings can coexist with old rings; wind-sensitive ear cuffs alter impact success and may rotate wind near-vertical; birthday hats boost XP; Wing Force Gloves' wind display timing was changed because it could affect play. Effects require trigger, stacking group, probability, and mode applicability metadata—not only flat stats. [JP.7.50](https://pangya.wiki/wiki/JP.7.50), [JP.7.63](https://pangya.wiki/wiki/JP.7.63), [JP.7.70](https://pangya.wiki/wiki/JP.7.70), [JP.7.85](https://pangya.wiki/wiki/JP.7.85)

#### 8.x / Grand Prix and Fresh Up-era evidence (2014–2016)

- **Effective 2014-03-19; page 2014-03-27, 8.01:** **Grand Prix Mode** existed with event tournaments; normal-play sorting and club-ranking determination needed fixes. The launch note itself is absent. [JP.8.01](https://pangya.wiki/wiki/JP.8.01)
- Japanese competitive hierarchy is now explicit: Challenge Cup records exclude Single Play and Grand Prix; top 199 qualify for **Master Cup**; rewards use **EP** (sometimes CP). Event Grand Prix tournaments coexist with PJC/season cups. Implement record eligibility per mode/event rather than a universal leaderboard. [JP.8.13](https://pangya.wiki/wiki/JP.8.13), [JP.8.17](https://pangya.wiki/wiki/JP.8.17), [JP.8.26](https://pangya.wiki/wiki/JP.8.26)
- **2014-07-31, 8.19:** a temporary Visual Effects options tab tested a planned graphics upgrade. [JP.8.19](https://pangya.wiki/wiki/JP.8.19)
- **2014-09-18, 8.26:** announced a major update for 2014-09-25, but the implementation page is absent. By **8.31**, **Ken(R)** and **Erika(R)** have separate motion-change items and receive rules distinct from classic characters; later pages also name **Cecilia(R)**. This likely brackets the Fresh Up/remaster-character transition, but the corpus does not establish its complete feature set. [JP.8.26](https://pangya.wiki/wiki/JP.8.26), [JP.8.31](https://pangya.wiki/wiki/JP.8.31), [JP.9.60](https://pangya.wiki/wiki/JP.9.60)
- Later 8.x pages mostly show mature live-ops atop existing systems. Notable durable data points: equipment stats could be corrected after release (including club curve slots and clothing control slots); Spin Cube accidentally returned 1 XP/1 PP; and mascots/caddies and club mastery receive multipliers. [JP.8.46](https://pangya.wiki/wiki/JP.8.46), [JP.8.53](https://pangya.wiki/wiki/JP.8.53), [JP.8.63](https://pangya.wiki/wiki/JP.8.63), [JP.8.87](https://pangya.wiki/wiki/JP.8.87)

#### 9.x — final content, mature effects, shutdown (2016–2017)

- **Effective 2016-07-21; page 2016-07-28, 9.21:** **Mystic Ruins**, the first course in three years, launched with course gimmicks; daily quests and Grand Prix content followed by 07-28, and it immediately entered Master Cup rotation. [JP.9.21](https://pangya.wiki/wiki/JP.9.21)
- **2016-08-10, 9.23:** Spin Cube placement/pool rotation is explicit: old courses Ice Inferno/Eastern Valley/Lost Seaway/Shining Sand → Mystic Ruins/Wind Hill/White Wiz/Ice Cannon; event overrides can place cubes on every course. [JP.9.23](https://pangya.wiki/wiki/JP.9.23)
- Equipment effects now form combinable state machines: paired rings activate terrain-100%, Safety, Miracle Sign, drive-distance, or combo-gauge effects; mascots can gain reinforcement; mascot+glove combinations add special effects. A 9.23 correction says Earth Ring Safety misbehaved in Versus, confirming **mode-scoped effect execution**. [JP.8.99](https://pangya.wiki/wiki/JP.8.99), [JP.9.00](https://pangya.wiki/wiki/JP.9.00), [JP.9.23](https://pangya.wiki/wiki/JP.9.23), [JP.9.32](https://pangya.wiki/wiki/JP.9.32), [JP.9.38](https://pangya.wiki/wiki/JP.9.38), [JP.9.60](https://pangya.wiki/wiki/JP.9.60)
- **2017-04-27 onward:** previously missing PP-gacha variants were added for Spika and the R characters; later magic recipes and EP exchange inventory were expanded for them. Character applicability should be explicit and migration-tested. [JP.9.60](https://pangya.wiki/wiki/JP.9.60), [JP.9.63](https://pangya.wiki/wiki/JP.9.63)
- **Effective 2017-08-31 / 09-07; documented 2017-09-21 and 09-28:** monetization shutdown removed CP consumables, priced most other CP goods at 1 CP, altered PP-gacha rare rates, and ran long-form final events. [JP.9.76.3](https://pangya.wiki/wiki/JP.9.76.3), [JP.9.77](https://pangya.wiki/wiki/JP.9.77)
- **2017-11-09–10, 9.78.3:** mass re-release, “Strong and New Game” (new accounts start Amateur E with 999,999 PP and a Mithril Sword set), final GM tournaments, and explicit service end at **2017-11-10 12:00**. This is terminal-state configuration, not a normal economy baseline. [JP.9.78.3](https://pangya.wiki/wiki/JP.9.78.3)

### System evolution distilled for implementation

1. **Release ledger:** store `patch_id`, `published_at`, `effective_at`, `family`, `region`, `source_url`, and relation (`preview`, `launch`, `follow-up`, `defect`, `shutdown`). Never sort solely by numeric-looking patch IDs.
2. **Modes and eligibility:** Versus, Tournament, Special Shuffle, HIO Battle, Ghost (removed), Single Course Practice/Repeat Hole, Grand Prix, Challenge Cup and Master Cup require separate availability and leaderboard eligibility rules.
3. **Content:** effective-dated courses, characters/classic-vs-R variants, cards, card packs, artifacts, titles/decorations, caddies/mascots, equipment slots, motions, and external/UCC catalog entries.
4. **Progression/economy:** Yarikomi missions, daily quest rotations, EP, PP/CP, room artifacts+mana, card synthesis, Magic Box recipes, Club Workshop enhancement/mastery, Spin Cube placement/loot pools, and Gacha/attendance pools are independently rotated systems.
5. **Effects engine:** represent flat stats, slots, probabilistic triggers, trigger timing, stacking groups, pair/set prerequisites, character applicability, mode applicability, temporary duration, and presentation effects separately. The patch defects repeatedly demonstrate failures when those dimensions are conflated.
6. **Live operations:** feature flags are required for mode suspension, UCC shop outage, event-wide cube placement, server layouts, reward-pool swaps, anti-cheat, and shutdown economy overrides.

### Japan-specific operating features

- **PangyaJapanCup/PJC**, Try/Challenge/Master Cups, EP distribution, top-199 qualification, GM tournaments, internet-café finals, and Nico Nico broadcasts form a distinctly Japanese tournament/live-broadcast layer. [JP.5.63](https://pangya.wiki/wiki/JP.5.63), [JP.6.49](https://pangya.wiki/wiki/JP.6.49), [JP.7.64](https://pangya.wiki/wiki/JP.7.64), [JP.8.13](https://pangya.wiki/wiki/JP.8.13)
- **Self Design Shop #1, Web Self Design Shop #2/UCC Web Shop, Web Gacha, official-site event pages, Twitter integration**, and serial/offline rewards mean the JP service cannot be reconstructed as a client/server binary alone. [JP.5.58](https://pangya.wiki/wiki/JP.5.58), [JP.5.66](https://pangya.wiki/wiki/JP.5.66), [JP.7.41](https://pangya.wiki/wiki/JP.7.41)
- The corpus documents a large Japan-specific licensed-content surface (Code Geass, Monogatari, Hello Kitty, Madoka Magica, Railgun S, Fate, Vividred, Squid Girl, Trickster, etc.). Most is transient catalog content, but voice clubs, special motions/logos, cut-ins, mascots, cards, and item-set effects require generic content hooks. Examples: [JP.5.78](https://pangya.wiki/wiki/JP.5.78), [JP.6.55](https://pangya.wiki/wiki/JP.6.55), [JP.7.73](https://pangya.wiki/wiki/JP.7.73), [JP.8.46](https://pangya.wiki/wiki/JP.8.46), [JP.9.21](https://pangya.wiki/wiki/JP.9.21)
- Best Manner badge, forced-termination count compensation, Japanese server naming/composition, and nProtect are region operations/account-policy features. [JP.5.61](https://pangya.wiki/wiki/JP.5.61), [JP.5.63](https://pangya.wiki/wiki/JP.5.63), [JP.5.64](https://pangya.wiki/wiki/JP.5.64), [JP.5.76](https://pangya.wiki/wiki/JP.5.76)

### Contradictions, gaps, and cautions

- **High — source completeness:** 14 early pages (3.x–4.x) are headline/link stubs; numerous version/date ranges are absent. The corpus jumps 4.07e→5.54, 5.80→5.9924, 6.56→7.04, 7.81→7.84, and many later patch numbers. “All 102 read” does **not** mean the Japanese patch history is complete.
- **High — missing major-launch records:** Tomahawk is previewed at 6.56 but its launch note is absent; Grand Prix is only referenced after its 2014-03-19 implementation; the 2014-09-25 major update is previewed but its launch note is absent. Do not synthesize launch contents beyond follow-up evidence. [JP.6.56](https://pangya.wiki/wiki/JP.6.56), [JP.8.01](https://pangya.wiki/wiki/JP.8.01), [JP.8.26](https://pangya.wiki/wiki/JP.8.26)
- **Medium — date/order anomaly:** `JP.5.73` is published 2010-05-21 but precedes `JP.5.74` published 2010-05-20 in corpus/version order. Sort by dates while retaining original patch ID and corpus order. [JP.5.73](https://pangya.wiki/wiki/JP.5.73), [JP.5.74](https://pangya.wiki/wiki/JP.5.74)
- **Medium — effective vs publication:** Mystic Ruins is repeatedly called a 2016-07-21 launch on pages published 07-28, 08-10 and 08-18; 9.76.3 (09-21) reports economy changes effective 08-31/09-07. Repetition is corroboration, not multiple launches. [JP.9.21](https://pangya.wiki/wiki/JP.9.21), [JP.9.23](https://pangya.wiki/wiki/JP.9.23), [JP.9.24](https://pangya.wiki/wiki/JP.9.24), [JP.9.76.3](https://pangya.wiki/wiki/JP.9.76.3)
- **Medium — naming/transliteration:** JP.7.60's Japanese headline says エボートマイン while the English wiki/course name is **Abbot Mine**. Preserve source label plus canonical content ID/alias rather than fuzzy matching names. [JP.7.60](https://pangya.wiki/wiki/JP.7.60)
- **Low — source notice wording:** 9.32/9.35 render sale/maintenance intervals with “maintenance start” before “maintenance end,” likely extraction/source wording errors. Do not derive interval validation rules from those lines.
- **Low — unresolved defect pages:** several notices promise a future correction without the correction page in this slice (e.g. 5.55, 5.71, 7.32, 7.42, 7.86). Defect reports prove intended constraints but not always final effective values.

### Highest-value pages

1. **[JP.7.30](https://pangya.wiki/wiki/JP.7.30)** — richest single specification: Challenges, Spika, achievements, daily quests, artifacts/mana, card synthesis, assists and UI changes.
2. **[JP.7.60](https://pangya.wiki/wiki/JP.7.60)** — Abbot Mine, Club Set Workshop, Card Vol.4, Ghost removal.
3. **[JP.9.21](https://pangya.wiki/wiki/JP.9.21)** — Mystic Ruins effective launch plus daily/Grand Prix integration.
4. **[JP.9.23](https://pangya.wiki/wiki/JP.9.23)** — explicit Spin Cube configuration migration and mode-scoped effect fixes.
5. **[JP.9.76.3](https://pangya.wiki/wiki/JP.9.76.3)** and **[JP.9.78.3](https://pangya.wiki/wiki/JP.9.78.3)** — shutdown economy and terminal service state.
6. **[JP.8.01](https://pangya.wiki/wiki/JP.8.01)** — best surviving evidence for Grand Prix timing.
7. **[JP.5.64](https://pangya.wiki/wiki/JP.5.64)**, **[JP.5.76](https://pangya.wiki/wiki/JP.5.76)**, **[JP.7.41](https://pangya.wiki/wiki/JP.7.41)** — Japanese anti-cheat, server topology, and UCC service boundaries.
