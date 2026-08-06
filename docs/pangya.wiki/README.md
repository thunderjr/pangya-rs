# pangya.wiki research synthesis

This folder is an implementation-oriented synthesis of **every main-namespace page returned by** [Special:AllPages](https://pangya.wiki/wiki/Special:AllPages) on 2026-08-05.

## Coverage

- AllPages listing pages followed: **2** (`+4` through `JP.8.99`, then `JP.9.00` through `Wiz Wiz`)
- Unique article pages: **437**
- Successfully read: **437**
- Missing/unreadable: **0**
- Source wikitext reviewed: **1,095,928 characters**
- Global/US patch-note pages: **193**
- Japan patch-note pages: **102**
- Other gameplay, course, history, character, and technical pages: **142**
- Redirects: **1** (`Experience points` → `Experience Points`)
- Source revision timestamps range from 2024-03-28 to 2026-05-24.

The page-by-page audit, including source URL, category, revision ID, timestamp, and destination document, is in [SOURCE_COVERAGE.md](SOURCE_COVERAGE.md).

## Documents

| Document | Contents |
|---|---|
| [GAMEPLAY_AND_MODES.md](GAMEPLAY_AND_MODES.md) | Shot state, stats, terrain, scoring, Pang/EXP rules, special shots, room settings, multiplayer, tournament, solo, and Grand Zodiac behavior |
| [COURSES.md](COURSES.md) | All 21 course pages and 26 walkthrough pages: locations, difficulty, par matrices, hazards, special mechanics, pin distances/elevations, tactics, and source contradictions |
| [CHARACTERS_HISTORY_AND_AUDIO.md](CHARACTERS_HISTORY_AND_AUDIO.md) | All characters and regional names/stats, hosting services, season chronology, soundtrack mapping, and historical caveats |
| [CLIENT_TECHNOLOGY.md](CLIENT_TECHNOLOGY.md) | Fresh XML UI, DAT localization, QuickPatch pipeline, internal-tool evidence, hit-bar state, and client/server clues |
| [PATCH_HISTORY.md](PATCH_HISTORY.md) | Durable milestones and implementation implications synthesized from all 295 Global/US and Japanese patch-note pages |
| [SOURCE_COVERAGE.md](SOURCE_COVERAGE.md) | Exhaustive 437-page provenance and coverage ledger |

## Highest-value conclusions for pangya-rs

1. **Version every rule and content table.** Pages mix Season 4, United, Tomahawk, Challenges, Grand Prix, Fresh Up, region-specific variants, removed modes, and late shutdown configurations.
2. **Separate accounting concerns.** Physical strokes, displayed/capped score, penalty events, Pang bonuses, EXP, treasure, and live-event rewards need independent ledgers.
3. **Use data-driven modes and live operations.** Course/order/time settings, Grand Prix schedules, loot pools, attendance, hole drops, quests, crafting recipes, migrations, and feature outages changed repeatedly.
4. **Model item effects as typed rules.** Patch history proves triggers, probabilities, stacking groups, set prerequisites, character/mode applicability, timing, and durations—not just flat stats.
5. **Do not invent missing physics or protocol constants.** The wiki gives many visible rules but not projectile equations, packet layouts, impact timing curves, several reward formulas, or complete patch manifests.
6. **Keep source provenance.** pangya.wiki is community-authored and contains typos, copied infoboxes, contradictory par data, incomplete tables, missing launch notes, and inferred technical terminology.

## Research method

The browser session was restricted to `pangya.wiki` and driven with `agent-browser`:

1. Opened `https://pangya.wiki/wiki/Special:AllPages`.
2. Followed the `Next page (JP.9.00)` navigation link.
3. Extracted and deduplicated the 437 article titles/URLs from both listing pages.
4. Read the current revision content and metadata for every listed title through same-origin MediaWiki API requests executed inside the browser session.
5. Checked that requested pages = returned pages = 437, with zero missing pages.
6. Synthesized the corpus by topic and audited every source page into the coverage ledger.

This folder intentionally contains synthesis and provenance, not a verbatim 1.1 MB mirror of the wiki.
