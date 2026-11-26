# Changelog

Versioning (semver, `MAJOR.MINOR.PATCH`): every change set bumps the version.
- PATCH (0.5.0 → 0.5.1): fixes, CI/docs-only tweaks.
- MINOR (0.5.x → 0.6.0): new features or settings.
- MAJOR: breaking changes (config/ban file formats). Stays 0 until a stable 1.0.
Bump it in 4 places: `Cargo.toml`, `Cargo.lock` (package `ghost-reforged`),
`packaging/windows/setup/Cargo.toml`, `packaging/windows/setup/Cargo.lock`.

## 0.13.0 — 2026-11-26

### Changed (Hosting page layout)
- **Folded host settings are a labelled grid** instead of one grey line: Map and Next game on full rows, then Auto start · DS filter, Rehost · Auto-quit (Auto-quit only in Play-as-host). `hosting_view::settings_rows` / `settings_grid`. The expanded form is unchanged, so existing UI test coordinates still apply.
- **Status beside Host / Force start / Stop**, with a coloured dot: grey idle/stopped, blue connecting, green lobby open, gold in game. Uses the existing `hosting_status`.
- **Players (n)** and **Recent kicks (n)** show counts. Recent kicks shows the latest 3 with a **Show all ▾ / Show less ▴** link (`Message::ToggleKicks`, `App::kicks_expanded`), so it no longer pushes Players down.
- **Right column:** Game View is shorter (`PREVIEW_HEIGHT` 286 → 244, picture 184 → 144 px), giving the Lobby / Current Game panel more rows without scrolling.
- Theme, frames, fonts, colours, sidebar and all buttons/behaviour unchanged.
- Test: `folded_settings_grid_lists_every_setting_and_auto_quit_only_for_player_hosts`.
- Version 0.12.2 → 0.13.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`; UI tests render the pages at 1180×830 and 1000×720 and will catch layout panics.
- Not visually checked. Send a screenshot and I will adjust spacing.

## 0.12.2 — 2026-11-26

### Fixed
- **Play-as-host stopped after one match because it shared the observer bot's rehost switch.** The profile's `rehost` ("Create next lobby after countdown") was designed for the observer host; when it was off, a player host also stopped after its first match. Play-as-host now has its own switch, `player_rehost` ("Host the next lobby after each match"), **on by default**, including for existing profiles (serde default true). `Profile::rehosts()` / `set_rehost()` pick the switch for the mode in use; the Options toggle, live settings, `max_games` and the Hosting summary all use it. Turning Play-as-host on or off never inherits the other mode's choice.
- Test: `player_and_observer_hosts_keep_separate_rehost_switches`.

### Changed
- Version 0.12.1 → 0.12.2.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.

## 0.12.1 — 2026-11-26

### Fixed
- **Unexplained stop after a match.** The only path in the code that ends hosting cleanly after a match (no error, no Stop click) is the game limit, which applies when "Host the next lobby after each match" is off (`max_games` = 1). The bot now says so in the status line, Activity and the notice, instead of just "Auto Host stopped.". Other failures already retry automatically and never stop hosting.
- The collapsed host settings line shows `Rehost: on` or `Rehost: OFF (stops after 1 game)`.
- Live settings no longer switch on the observer-only "leave at load" / "restart client" flags for a player host (`Engine::update_rehost`). They were ignored in player mode, so this is a tidy-up, not the cause.
- Tests: `live_rehost_settings_never_turn_on_observer_only_leaving_for_a_player_host`; summary test covers the rehost text.

### Changed
- Version 0.12.0 → 0.12.1.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Not confirmed that rehost is off in the user's profile; the new message will show it.

## 0.12.0 — 2026-11-26

### Added
- **Auto-quit for Direct Strike (Play-as-host).** The map's VICTORY/DEFEAT panel sends nothing to the bot, so the bot watches the game window instead (`src/end_panel.rs`, `src/hosting/end_watch.rs`).
  - Detection: every 2 s while a match runs, a window capture is compared with two reference patches (the panel's Quit button and its discord link box) using normalised cross-correlation, searched ±3 px. Both must score ≥ 0.75, on two captures in a row. Positions come from war3map.j (Quit button 0.08×0.04 at (0.675, 0.21) on the 0.8×0.6 frame grid, 4:3-centred), so any window size works. The Extra Round button (0.56, 0.21) is never clicked.
  - Action, after the configured delay: a posted click on Quit (mouse untouched); 4 s later, if the panel is still there, a real click (game brought forward, pointer moved and put back); 15 s later, only if enabled, `LeaveGame` as a last resort.
  - Clicking Quit runs the map's own win/loss code, so W3C/FLO results are recorded normally. The results screen then follows and the existing rehost path takes over.
  - Activity logs "End panel seen · match 0.92 / 0.95", each click, and capture failures.
- Options (Play as host): "Press Quit on the Direct Strike end panel" (on by default), a 0–60 s delay (default 5 s), and "If Quit cannot be pressed, leave the game" (off by default). Applied when hosting starts.
- Config: `auto_quit`, `auto_quit_delay_seconds`, `leave_if_quit_fails`. Profile fields with defaults, so old profiles load unchanged.
- Tests: `the_end_panel_is_recognised_at_any_window_shape_and_noise_is_not`, `the_quit_button_is_found_on_the_map_frame_grid`, `quit_is_clicked_after_two_sightings_and_the_delay_then_retried_then_left_only_if_enabled`.

### Changed
- Version 0.11.1 → 0.12.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Reference patches come from one 640×360 screenshot. Checked against that image (both ≈ 1.0) and random scenes in it (≤ 0.66 / 0.45). Different graphics settings (SD/HD) may score lower; the Activity line shows the scores for tuning.
- Not known whether Warcraft III accepts the posted click; the real click is the fallback.

## 0.11.1 — 2026-11-26

### Fixed
- **"Checking…" forever when a ds-report lookup fails.** Failed lookups (blocked, network error, unreadable answer) now send `Event::DsFailed`; the row shows **Lookup failed** instead of Checking…, for the host and for guests.
- Activity explains every lookup that did not find stats: `DS lookup · Name · failed · HTTP 403` / `not found · HTTP 404` / `network: …` (`ds_report::lookup_explained`). The host line reads `DS lookup for you · Name · <result> · <reason>`.

### Added
- `extras/war3map-autoquit.j` is delivered separately (not in the repo): Direct Strike map change that ends the match for everyone 20 s after the end panel, as if each player pressed Quit. The bot then sees the results screen and rehosts as before.

### Changed
- Version 0.11.0 → 0.11.1.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.

## 0.11.0 — 2026-11-26

### Added
- **Your own stats.** In Play-as-host mode the host's BattleTag is looked up on ds-report once per lobby (`DsCheck::lookup_self`) and shown first in Players and Current Game as "Name (you)". Display only: never filtered, kicked or announced. Activity logs `DS lookup for you · … · <result>` ("not found (HTTP 404/500)" or "lookup failed"), so a broken lookup is obvious.
- `App::own_name`; own stats are kept across new lobbies and game start.
- Tests: `the_hosts_own_lookup_is_shown_but_never_kicked_or_announced`, `the_hosts_own_stats_survive_a_new_lobby_and_the_game_start`, `the_finished_matchs_roster_after_quit_does_not_stop_a_player_host`.

### Fixed
- **Hosting stopped after clicking Quit (likely cause).** After a player-host match the bot releases the lobby, but Warcraft III can still report the finished game's roster. The engine treated it as "an existing lobby is active" and stopped; on reconnect the same roster raised "This lobby belongs to another session", so hosting never resumed. The finished lobby's name is now remembered (`Config::finished_lobby`, not saved) and its roster is ignored by both checks. Any other foreign lobby still stops hosting.

### Changed
- Version 0.10.0 → 0.11.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- The Quit fix matches the symptoms but was not confirmed against a log. If hosting still stops, the last Activity lines show why.
- Direct Strike's in-game VICTORY panel waits for Quit; the bot cannot press it.

## 0.10.0 — 2026-11-26

### Added
- **Current Game panel.** During a match the right-hand Lobby panel becomes "Current Game": every other player with DS level, matches, status and a **Ban** button. The left side no longer duplicates it.
- **Early-leave bans.** The launcher times each match (`Event::MatchEnded`, sent once by `Engine::take_match_ended`). A match shorter than 7 minutes is marked "ended early" (Direct Strike ends the game when someone leaves before 7 minutes). The Last game section then shows a red notice and a **Left early** button per player that bans with the reason "Left the game early".
  - Who left cannot be read from the game's local UI socket, so the host picks the player. If a future protocol log shows an in-game leave message, this can be automated.
- Tests: `the_end_of_a_player_match_is_reported_once`, `a_short_match_is_marked_as_ended_early_and_leavers_get_the_early_reason`, `settings_start_folded_only_when_a_real_map_is_ready`.

### Changed
- **Banned players who rejoin get Warcraft III's native ban** (`BanPlayerFromGameLobby`, `request_ban`) instead of a plain kick. Bans are always saved locally first (`bans.json`); the native ban still falls back to the list if the game ignores it. `W3_NATIVE_BAN=0` restores plain kicks.
- **Host settings start folded** when the profile is ready to host (map file exists, all fields valid), and fold again when hosting stops if still complete. Otherwise they open so you can fix them.
- Version 0.9.1 → 0.10.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Not tested in game. The 7-minute rule is a fixed constant (`EARLY_END_SECONDS`).

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
