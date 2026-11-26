# Changelog

Versioning (semver, `MAJOR.MINOR.PATCH`): every change set bumps the version.
- PATCH (0.5.0 → 0.5.1): fixes, CI/docs-only tweaks.
- MINOR (0.5.x → 0.6.0): new features or settings.
- MAJOR: breaking changes (config/ban file formats). Stays 0 until a stable 1.0.
Bump it in 4 places: `Cargo.toml`, `Cargo.lock` (package `ghost-reforged`),
`packaging/windows/setup/Cargo.toml`, `packaging/windows/setup/Cargo.lock`.

## 0.9.1 — 2026-11-26

### Fixed
- **Play-as-host did not leave the results screen and rehost.** The match end is now detected from either signal the game sends: `GameClosed` (arrives just before the results) or `SetGlueScreen SCORE_SCREEN`, in any phase where the lobby is still ours, not only `InGame`. Before, a missed countdown/loading callback (or a reconnect) left the engine outside `InGame`, so the results screen was ignored. Counted once (`Engine::player_match_over`).
- At the end of the results delay the bot closes the results if they are open, even when they appeared after the match was counted (`score_open`), then waits for a menu screen before creating the next lobby (`menu_required`, same as the observer path).
- The Direct Strike map (`war3map.j`) ends every match with `CustomVictoryBJ(p, false, true)` / `EndGame(true)`, i.e. straight to the results screen, which is what the above relies on. Its `"score"` stats go to the game's stat channel, not the local UI socket, so the bot cannot read them.

### Added
- Activity shows `Game screen · <screen> · <bot state>` and `Game event · GameClosed …`, so a stuck end of match can be diagnosed from the Activity page.
- Tests: `a_player_match_that_ended_unnoticed_is_still_closed_and_rehosted`, `results_seen_during_cooldown_are_closed_when_it_ends`; updated `next_lobby_waits_for_score_screen_cooldown_and_fresh_provider` (it expected the results to close instantly, which 0.8.0 changed) and the 0.8.0 rehost test (now waits for a menu).

### Changed
- Version 0.9.0 → 0.9.1 (fix).

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Not tested in game. If it still sticks, the Activity lines show which screen/event the game sent last.

## 0.9.0 — 2026-11-26

### Added
- **Collapsible host settings** on the Hosting page. Profile, map, game name, next number, auto start and the live DS filter sit in one section.
  - Collapsed: map file name plus one summary line (e.g. "Mix Mode DS #5 · Auto start: Full · Filter off") and an **Edit ▾** link.
  - Expanded: the same fields as before, with a **Hide host settings ▴** link on the Profile label row (no extra height, so existing layout and UI tests keep their positions).
  - Collapses automatically when Host succeeds; reopens when hosting stops. Can be toggled any time; the DS filter stays live while expanded.
- Players / In this game / Recent kicks get the freed space (separated by a rule).
- `Message::ToggleHostSettings`, `App::settings_open`, `hosting_view::settings_summary`.
- Tests: `collapsed_settings_summarise_name_start_rule_and_filter`, `host_settings_collapse_on_toggle_and_reopen_when_hosting_stops`.

### Changed
- Version 0.8.0 → 0.9.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- `ui_tests` click positions were not re-measured; the expanded layout should be unchanged except for a rule above Players.

## 0.8.0 — 2026-11-26

### Added
- **Rehost after a match in Play-as-host mode.** When the host's match ends (`SCORE_SCREEN`), the game is counted and the lobby number advances (`Engine::finish_game`). The results stay open for the rehost delay, then the bot sends `ScoreScreenClose` + `InitializeNetProvider` (the observer host's proven return path) and creates the next lobby. Before, player mode ignored the score screen and never rehosted.
  - Clicking Back early skips the wait; no stale `ScoreScreenClose` is sent (`score_close_pending` is cleared on any menu screen).
  - With "Host the next lobby after each match" off, the game still counts and hosting stops; the results screen is left alone.
  - Activity shows "Match finished · next lobby in N s"; status shows the countdown.
- Options: in Play-as-host mode the rehost toggle reads "Host the next lobby after each match" and the delay is labelled "Results screen time (s)", with a hint.
- Tests: `a_player_host_sees_the_results_then_returns_and_hosts_the_next_lobby`, `a_player_host_who_clicks_back_early_is_not_sent_a_stale_close`, `a_player_host_without_rehost_keeps_the_results_and_stops`.

### Changed
- Version 0.7.0 → 0.8.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Not tested in game: that `ScoreScreenClose` closes the results for a player (it is used for the observer host today). If it does not, check `w3-protocol.log` around the match end.

## 0.7.0 — 2026-11-26

### Added
- **Live Direct Strike filter on the Hosting page.** Min level / min matches can be changed at any time, also while hosting; they are sent ~0.5 s after typing as part of `LiveSettings` (new fields `min_ds_level`, `min_ds_matches`). No restart needed.
  - A stricter filter re-checks players already in the lobby (`DsCheck::refilter`) and kicks those who no longer pass, with the usual reason. Relaxing it cancels kicks still queued.
  - Options page: the two minimum fields are no longer locked while hosting. Patch version still applies when hosting starts.
- **Players section** (Hosting page): everyone in the lobby with DS level, matches and status (Passed / Found / Below minimum / Not on W3C / Checking… / No BattleTag).
- **In this game / Last game section** (Hosting page): the players of the running game (lobby is gone once it starts) with level, matches and a **Ban** button each, plus an optional reason box. Bans go to `bans.json`; they take effect in later lobbies. The host is not listed.
- Tests: `a_stricter_live_filter_queues_players_already_in_the_lobby`, `player_rows_show_level_matches_and_filter_status`, `ds_minimums_are_live_settings_and_game_players_stay_listed_with_levels`; `live_settings_change_the_current_lobby_without_recreating_it` also checks the DS minimums.

### Changed
- Lobby rows no longer show the `[Lv. … | …m]` badge (it wrapped over three lines beside long names).
- Game start keeps the DS stats of the game's players instead of clearing them.
- Version 0.6.0 → 0.7.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Supersedes the earlier 0.7.0 zip built from a text rendering; this one was made from the real 0.6.0 files.

## 0.6.0 — 2026-11-26

### Added
- **Kick reasons.** Before any kick the bot posts `Removing <player>: <reason>` in lobby chat, and the launcher lists it under **Recent kicks** on the Hosting page (last 8, newest first) and in Activity.
  - Manual Kick: "Kicked by the host". Ban button: "Banned: <reason>". Ban list on rejoin: the saved reason, or "On the ban list".
  - Direct Strike filter: which minimum failed, e.g. "level 3 is below 10 and 40 matches is below 100", or "not found on W3Champions".
  - Vote kick: listed as "Vote kick passed" (chat already announces it).
- `BanList::matching`, `KickTarget::name`, `ds_report::reject_reason`, `hosting::Event::Kicked`.
- Tests: kick reason text, DS reject reasons, recent-kick list, observer setting.

### Fixed
- **Observer slots = 0 in player-host mode** now creates the game with observers off (`settingObservers` 0), so no observer team appears. Before, the game was created with "Full" observers and the bot closed each slot, leaving a column of Closed slots.
- The launcher's own-slot row no longer always says "Observer"; in player-host mode it shows "Host · slot N".

### Changed
- Version 0.5.0 → 0.6.0. `PendingKick` is now a named struct carrying the reason.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Assumed: observers "None" removes the observer team in Warcraft III 3.0 lobbies. Observer slots 1–12 still use "Full" + closing extras.

## 0.5.0 — 2026-11-26

### Added
- **Ban list on the Options page** (`src/launcher/ban_list.rs`).
  - Type any BattleTag (`Name#1234`, or a bare `Name` for every tag) plus an optional reason and click **Ban**. Works while hosting or idle.
  - **Last game's players**: when a game starts, the launcher remembers its human guests. Each can be banned after the match with one click (uses the reason box).
  - The launcher writes `bans.json` directly; a running host re-reads it within ~1 s and removes listed players from the lobby. No new worker messages.
- Test: `manual_and_last_game_bans_are_saved_with_reason_and_remembered`.

### Changed
- Version 0.4.2 → 0.5.0 (new feature).
- CI: moved GitHub Actions to Node 24 versions (`checkout@v6`, `cache@v5`, `upload-artifact@v6`) to clear the Node 20 deprecation warning.
- Ban file location is resolved once at launcher start (`App::ban_file`) and shared with hosting.

### Not verified
- Not compiled or run: the editing environment had no Rust toolchain. Run `cargo test` and check the Options page layout on Windows.
- "Last game's players" is in memory only; it is lost when the launcher closes.
