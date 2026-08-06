# Characters, service history, seasons, and audio

## Scope and method

This synthesis covers all 31 articles classified as characters, hosting regions, or seasons, plus the two uncategorized audio articles and the general-history landing page (34 total). The 295 Global/Japan patch-note records are synthesized separately in [PATCH_HISTORY.md](PATCH_HISTORY.md); course lore remains in [COURSES.md](COURSES.md).

Wiki citations below use the live article URL, while facts were read from the supplied snapshots (including snapshot revision/timestamp). Treat the corpus as a community secondary source, not an official canon database.

## Canonical character facts

The character infoboxes provide the most implementation-ready identity data: regional display names, clothing/resource codes, birthday, age, and base stats. Slash-separated European names are alternatives across European localizations, not one long canonical name.

| Character | KR / JP / TH / EU name(s) | Code | Age; birthday | Base P/C/A/S/Cr | Canon/lore summary |
|---|---|---:|---|---|---|
| [Arin](https://pangya.wiki/wiki/Arin) | Arin / Arin / Arin / Marine, Arin, Alene, Ariana | `A` | 20; Nov 15 | 11/10/9/2/4 | Magician-family performer, top Wiz Wiz graduate; joined after falling for Max. |
| [Azer](https://pangya.wiki/wiki/Azer) | Azer / **Daisuke** / **Arthur** / Robert, Jürgen, Roger, Alfredo | `AZ` | 41; Dec 1 | 11/8/5/5/2 | Hot-tempered ex-cop invited by Lolo; strong situational analysis and woods specialty. |
| [Cecilia](https://pangya.wiki/wiki/Cecilia) | Cecilia / Cecilia / Cecilia / Cecilia, Cècilia | `C` | 27; Jun 23 | 10/9/6/2/6 | Captain of battle cruiser Silvia, mechanic/analyst, Silvia’s tournament representative. Has a Renewal variant (`CR`). |
| [Hana](https://pangya.wiki/wiki/Hana) | Hana / **Erika** / Hana / Anna, Ana | `H` | 14; Apr 9 | 9/11/6/2/2 | Invited by Quma for comet control; calm, consistent iron player. Has Renewal variant (`HR`). |
| [Kooh](https://pangya.wiki/wiki/Kooh) | Kooh / Kooh / Kooh / Kooh, Kua | `K` | 11; Feb 3 | 10/11/4/3/1 | Young captain of pirate ship Lunar Tomb; searches for her missing father and seeks to clear him of allegiance to the Demon King. |
| [Kaz](https://pangya.wiki/wiki/Kaz) | Kaz / Kaz / Kaz / Kaz | `KZ` | 18; Dec 31 | 12/11/8/3/3 | Rue Clan swordsman and intended successor to a sealed Evil Lord; amnesiac, accompanied by Karen’s spirit after her sacrifice. |
| [Lucia](https://pangya.wiki/wiki/Lucia) | Lucia / Lucia / Lucia / Lucia | `L` | 17; Jul 8 | 10/11/9/2/3 | International pop idol; introduced as the ninth playable character in Season 4. |
| [Max](https://pangya.wiki/wiki/Max) | Max / Max / Max / Max | `M` | 23; Sep 17 | 12/10/6/1/1 | British pro tennis player who dreams of piloting; saved from a crash by the Chronos Clan, then stays on Pangya Island and pursues Cecilia. |
| [Nell](https://pangya.wiki/wiki/Nell) | Nell / Nell / Nell / Nell | `NL` | unknown; Aug 16 | 12/11/7/3/2 | Born from the Demon King’s heart, raised by Cadie, unaware of that origin; introduced as the tenth character in Season 4.5. |
| [Nuri](https://pangya.wiki/wiki/Nuri) | Nuri / **Ken** / Nuri / Junior, Tim, Peke | `N` | 15; Oct 8 | 9/11/6/2/2 | Brought to Pangya by Pippin; controlled, low-power iron player with a signature hoverboard. Has Renewal variant (`NR`). |
| [Spika](https://pangya.wiki/wiki/Spika) | Spika / Spika / Spika / *(EU absent)* | `S` | 16 Earth years; May 20 | 12/11/9/2/3 | Off-world researcher collecting magic energy with device/companion Roi; linked to Shining Sand, Grand Zodiac, Cecilia, and Wingtross. |

`P/C/A/S/Cr` means power/control/accuracy/spin/curve. Every character infobox lists a 10,000 price, but the currency is not stated in these articles, so pangya-rs should not infer one. Career/specialty strings are flavor/localized copy rather than durable mechanics identifiers.

### Character modeling implications

- Separate a stable internal character ID from localized display names. Japanese `Daisuke`, `Erika`, and `Ken` map to Azer, Hana, and Nuri; European aliases are especially many-to-one.
- Keep Renewal forms as variants of the same identity, not new lore characters. The source explicitly documents Renewal only for Cecilia, Hana, and Nuri.
- Keep stats/version provenance. These pages present one base tuple but do not say which client build/season/region it belongs to.
- Preserve unknown/absent distinctly: Nell’s age is explicitly “Unknown”; Spika simply has no EU-name field.

## Hosting regions and service history

| Hosting service | Operation in article | Publishers / transitions | Source |
|---|---|---|---|
| Brazil | Oct 2005–Mar 2007 | Uno Games (Uno Net Work do Brasil subdivision) | [Brazil](https://pangya.wiki/wiki/Brazil) |
| China | May 2006–May 2008 | SINA Corporation | [China](https://pangya.wiki/wiki/China) |
| Europe | Mar 2007–31 Dec 2010 | GOA → Galaxy Games (Jul 2010) | [Europe](https://pangya.wiki/wiki/Europe) |
| Indonesia | Jul 2005–Apr 2008 | PT. BolehNet Indonesia | [Indonesia](https://pangya.wiki/wiki/Indonesia) |
| Korea | Nov 2004–29 Aug 2016 | Ntreev/Hanbitsoft arrangement → Ntreev (contract ended Mar 2009) → Smilegate Megaport after Ntreev’s Feb 2015 publishing retreat; Ntreev remained developer | [Korea](https://pangya.wiki/wiki/Korea) |
| Japan | Nov 2004 (closure conflict: region page says 9 Nov 2017; final patch says 10 Nov 2017 at 12:00) | Gamepot | [Japan](https://pangya.wiki/wiki/Japan); [JP.9.78.3](https://pangya.wiki/wiki/JP.9.78.3) |
| North America / US | Dec 2005–12 Dec 2016 | Gamefactory/OGPlanet (`Albatross18`, closed Mar 2009) → Ntreev US (Apr 2009) → Smilegate Interactive after Nov 2010 acquisition; GameRage portal from Sep 2011 | [North America](https://pangya.wiki/wiki/North_America) |
| Philippines | Jan 2006–25 Jan 2008 | netGames → Level Up! after 2006 roster merger | [Philippines](https://pangya.wiki/wiki/Philippines) |
| South East Asia (Malaysia/Singapore evidence) | 27 May 2006–Aug 2008 | Asiasoft; final update announced 8 May 2008 | [South East Asia](https://pangya.wiki/wiki/South_East_Asia) |
| Taiwan | Feb 2005–Apr 2009 | T2 Entertainment | [Taiwan](https://pangya.wiki/wiki/Taiwan) |
| Thailand | Feb 2005–30 Apr 2024 | Ini3 Digital; closure announced 30 Mar 2024 | [Thailand](https://pangya.wiki/wiki/Thailand) |

### Regional chronology and naming

- Earliest listed launches are Korea and Japan (Nov 2004), followed by Taiwan and Thailand (Feb 2005), Indonesia (Jul 2005), Brazil (Oct 2005), North America (Dec 2005), Philippines (Jan 2006), China and SEA (May 2006), and Europe (Mar 2007).
- Thailand was the last of these official PC services, closing in April 2024. Japan lasted to 2017, Korea and North America to 2016; Europe had already closed before the United-era synchronization.
- North America launched under **Albatross18**. Season 4: Delight (July 2009 in the page’s NA-oriented prose) was the first North American season to use official **Pangya** branding and also renamed unspecified in-game terms. Do not assume early US strings equal KR/JP/global strings.
- “North America,” “US,” and later “Global” are used inconsistently: the region article calls its scope North America and then “US”; United describes “Pangya Global and international” servers. Store publisher/service/locale as separate dimensions rather than using one overloaded region enum.
- Publisher transitions are not necessarily service closures. Europe’s GOA→Galaxy and Korea’s publisher handoffs were continuations; North America did have a one-month gap/relaunch between Albatross18 and Ntreev US according to the article.

## Seasons and cross-region release timeline

| Ordinal / title | KR | TH | JP | US | EU | Key facts |
|---|---:|---:|---:|---:|---:|---|
| 1 — [Pangya: Season 1](https://pangya.wiki/wiki/Pangya%3A_Season_1) | 2004 | 2005 | 2004 | — | — | Initial version had no formal season number/title; “Season 1” is retrospective usage. |
| 2 — [Pangya: Season 2](https://pangya.wiki/wiki/Pangya%3A_Season_2) | 2005 | 2006 | 2005 | — | — | Second season. |
| 3 — [Revolution](https://pangya.wiki/wiki/Pangya_Season_3%3A_Revolution) | 2007 | 2007 | 2007 | 2008 | 2008 | First season with a subtitle/name. |
| 4 — [Delight](https://pangya.wiki/wiki/Pangya_Season_4%3A_Delight) | 2008 | 2008 | 2008 | 2009 | 2009 | Pirate/treasure motif; Lucia, Lost Seaway, My Room expansion, Treasure Gauge, replay, 5-star course difficulty, Card Holic/Gacha and other systems. Extended to 4.5 in late Dec 2010; 4.5 added Nell. |
| 5 — [United](https://pangya.wiki/wiki/Pangya_United) | **2009 per infobox** | 2011 | 2011 | 2011 | — | First title to omit “Season”; intended to align timed updates across global/international servers. NA-oriented body says Apr 2011–Jun 2012. Added Special Shuffle Course, recycling/rental/basic systems and multiple system revisions. |
| 6 — [Tomahawk](https://pangya.wiki/wiki/Pangya_Tomahawk) | 2012 | 2012 | 2012 | 2012 | — | Sixth season. |
| 7 — [Challenges](https://pangya.wiki/wiki/Pangya_Challenges) | 2012 | 2013 | 2012 | 2013 | — | Seventh season. |
| 8 — [Grand Prix](https://pangya.wiki/wiki/Pangya_Grand_Prix) | 2014 | 2014 | 2014 | 2014 | — | Eighth season. |
| 9 — [Fresh Up](https://pangya.wiki/wiki/Pangya_Fresh_Up) | 2014 | 2014 | 2014 | 2014 | — | Ninth and final season. |

Important interpretation: year tables are region-specific and are more appropriate for data than the long Season 4/United prose, which appears North-America-oriented even when it speaks generically. “United” means broadly synchronized updates, not one shared account/game server; the page says servers were “connected” for similar timing, but supplies no protocol/account-merger evidence.

## Soundtrack/audio canon

[Pangya Online Original Soundtrack](https://pangya.wiki/wiki/Pangya_Online_Original_Soundtrack) inventories **55 entries**: 13 system BGM, 31 course BGM, 5 promotional themes, and 6 Japan-only VOCALOID collaboration remixes. Its explicit warning matters for pangya-rs: most system BGM had no proper published title; cleaned filenames or menu/area descriptions are labels, not authoritative track titles.

### System BGM (13)

- `Tea Time` — Helicon SoundWorks
- `Coffee Time` — ESTi; introduced with Grand Prix
- `Putting - Over Par`, `Putting - Under Par` — Hyunjun (Justy) Kim
- `Result - Over Par`, `Result - Under Par`, `Scoreboard`, `My Room` — Helicon SoundWorks
- `Grand Prix - Lobby`, `Grand Prix - Result 1`, `Grand Prix - Result 2` — STUDIO EIM
- `Grand Prix - Scoreboard` — 슈퍼꼬마 (supbaby)
- `Short Game Theme` — ESTi

### Course BGM mapping (31)

- Pink Wind / Wind Hill / Sepia Wind: `Breeze` (Justy Kim), `Spring` (Helicon)
- Blue Lagoon / Blue Water / Blue Moon: `Daydream` (Justy Kim), `Frog` (Helicon)
- Wiz Wiz / West Wiz: `Bunny Picnic` (Helicon), `A Shiny Day` (Justy Kim)
- White Wiz: `Snowscape`, `Winter Ride` (Helicon)
- Silvia Cannon: `Navy Blue Memory` (Helicon), `Rising Sun` (supbaby)
- Shining Sand: `Somewhere`, `Nowhere` (Helicon)
- Ice Cannon: `Crystal Waver`, `Happy Flight` (Helicon)
- Deep Inferno: `Dive Into Volcano`, `Vermilion Sunset` (ESTi)
- Ice Spa: `Crystal Lake` (Helicon), `Fade Into White` (supbaby)
- Lost Seaway: `The Mystery Of The Lost Seaway` (Nevis), `Voyage The Sky` (supbaby)
- Eastern Valley: `Eastern Valley`, `River` (supbaby)
- Wiz City: `Secret Wish`, `A Day In The WizCity` (supbaby)
- Ice Inferno: `Orbit of Darkness`, `Cyan Sunset` (supbaby)
- Grand Zodiac: `Grand Skyscraper` (supbaby)
- Abbot Mine: `Beautiful Ruins` (Zoolook: limgirl & supbaby), `Skyrider` (supbaby)
- Mystic Ruins: `Dear Memory`, `Oracle` (ヴァシュター / Akimasa Shibata)

### Promotional and region-specific tracks

Promotional: `Season 2 Theme Song` (credited Ntreev Soft), `Revive` (ESTi; Revolution), `ZERO FILL LOVE` (ESTi/Sanch; Delight and Lucia), `MUTE` (ESTi x TAK/Sanch; Challenges and Spika), and `Grand Skyscraper` (supbaby; Tomahawk and Grand Zodiac). Season 4’s article calls the trailer song “Zero Fill Love” sung by ESTi, whereas the soundtrack table credits `ZERO FILL LOVE` to ESTi/Sanch; preserve the soundtrack credit and flag the prose discrepancy rather than overwriting it.

Japan-only Pangya × VOCALOiD collaborations: `Bunny Picnic (Magical Lunchbox Mix)` (Gamepot feat. Hatsune Miku), `Crystal Lake (273k Mix)` (Gamepot feat. Kagamine Len), `Nowhere (Vocaloid Mix)` (ゆにめもP, awk feat. Megurine Luka), `Revive (Prayer For The Planet)` (Gamepot feat. Kagamine Rin), `A Shiny Day (The Sun, The Miracle Aztec)` (Gamepot feat. Hatsune Miku), and `Snowscape (yks Remix)` (yuukiss feat. MEIKO & KAITO). “Gamepot” is a placeholder attribution where the artist is unknown, per the article.

[Helicon SoundWorks](https://pangya.wiki/wiki/Helicon_SoundWorks) is described as the commissioned team that set Pangya’s overall sound/tone while working with Ntreev. Listed members are music director/main composer Hyunjun (Justy) Kim, moai, and MyNoX. moai and MyNoX reportedly had ties to Sonnori, the parent company from which Ntreev split. Model `Helicon SoundWorks` (team credit) separately from Justy Kim (individual credit); the source uses both.

## General historical synthesis useful to pangya-rs

1. **2004–06 expansion:** initial Korean/Japanese service and retrospectively named Season 1; rapid rollout through Thailand/Taiwan/Indonesia/Brazil/NA/Philippines/China/SEA. Early service naming and content were localized rather than globally uniform.
2. **2007–09 consolidation/rebranding:** Europe launches; Revolution is the first named season and first listed in US/EU; many smaller regional services close. Delight arrives by region in 2008–09, and North America changes from Albatross18 to Pangya after OGPlanet closure/Ntreev US relaunch.
3. **2010–12 synchronization:** Europe closes in 2010. Delight 4.5 introduces Nell, then United attempts coordinated international update timing. Tomahawk and Challenges release in KR/JP before TH/US for Challenges, showing synchronization was approximate.
4. **2014 final content era:** Grand Prix and Fresh Up are listed across KR/TH/JP/US in 2014; Fresh Up is the ninth/final season.
5. **2016–24 shutdown tail:** Korea and North America close in 2016, Japan in 2017, Thailand in 2024. “Final season” does not mean simultaneous end of service.

The [Main Page](https://pangya.wiki/wiki/Main_Page) supplies no additional game history; it describes pangya.wiki itself as a community-coordinated historical/technical/general wiki. That is useful provenance: claims are community-curated snapshots, not automatically official Ntreev records.

## Source caveats and review findings

- **High — Philippines:** prose says Level Up announced non-renewal “In January 2006,” causing suspension on 25 Jan 2008. This is internally suspect (same launch month, two years before shutdown) and likely a year typo. Do not encode the announcement year without external confirmation.
- **High — Japan closure date:** the region page says 9 Nov 2017, while final patch page JP.9.78.3 places service end at 12:00 on 10 Nov 2017. Preserve both claims until primary evidence resolves the one-day conflict.
- **High — United:** the infobox says KR 2009 while generic body text says United succeeded Delight in April 2011. This may reflect regional chronology versus NA-focused prose, but scope is unlabeled. Preserve both claims; do not flatten them to one global start date.
- **Medium — Delight:** the infobox gives regional 2008/2009 releases, while the body gives July 2009, late-Dec-2010 4.5, and Apr-2011 end without identifying those prose dates as US-specific. Context suggests NA, but the page does not say so explicitly.
- **Medium — Grand Prix:** its infobox `title` incorrectly says “Pangya Tomahawk,” while page title/body say Grand Prix. Use page title/body.
- **Medium — characters:** base stats, price, and biographies have no references or version/region qualifiers. They are wiki-attested defaults, not verified universal values.
- **Medium — audio:** both audio articles lack reference sections. Track names, credits, Helicon membership, and corporate history are community assertions; system labels are explicitly normalized filenames rather than proper titles.
- **Low — audio credit conflict:** Season 4 prose says ESTi sang `Zero Fill Love`, while the soundtrack article credits ESTi/Sanch. Keep raw credit fields/provenance until primary credits are checked.
- **Low — spelling/data hygiene:** “Semptember” (NA), “Feburary” (Thailand), `Ceclia` (Max), malformed Renewal clothing-code strings, and Grand Prix’s copied infobox title demonstrate that text should not be imported mechanically.
- **Low — naming:** European slash lists do not identify which language/country used which alias. Do not guess locale-to-alias mapping.

## All analyzed titles (34)

**Characters (11):** Arin; Azer; Cecilia; Hana; Kaz; Kooh; Lucia; Max; Nell; Nuri; Spika.

**Regions (11):** Brazil; China; Europe; Indonesia; Japan; Korea; North America; Philippines; South East Asia; Taiwan; Thailand.

**Seasons (9):** Pangya: Season 1; Pangya: Season 2; Pangya Season 3: Revolution; Pangya Season 4: Delight; Pangya United; Pangya Tomahawk; Pangya Challenges; Pangya Grand Prix; Pangya Fresh Up.

**Audio/general (3):** Helicon SoundWorks; Pangya Online Original Soundtrack; Main Page.

## Residual risks

- No primary-source verification was requested or performed; several region pages link archived/press sources, but most character, season, and audio claims are uncited.
- The thematic scope excludes 295 patch-note pages and course-specific lore. If “general history” is intended to include every chronological patch log, that is a separate large extraction and remains unreviewed here.
- The wiki snapshots were revised at different times (mostly 2024, with some 2025 season/audio edits), so cross-page wording and dates are not guaranteed to have been reconciled by editors.
