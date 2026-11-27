# Changelog

Versioning (semver, `MAJOR.MINOR.PATCH`): every change set bumps the version.
- PATCH (0.5.0 → 0.5.1): fixes, CI/docs-only tweaks.
- MINOR (0.5.x → 0.6.0): new features or settings.
- MAJOR: breaking changes (config/ban file formats). Stays 0 until a stable 1.0.
Bump it in 4 places: `Cargo.toml`, `Cargo.lock` (package `ghost-reforged`),
`packaging/windows/setup/Cargo.toml`, `packaging/windows/setup/Cargo.lock`.

## 0.22.0 - 2026-11-28

### Added
- **Bring GHost back to the front after Battle.net starts the game** (Play as host, Options; on by default). After Host, and when hosting restarts a closed game, a watcher brings the GHost Reforged window back whenever Battle.net or the new Warcraft III window takes the foreground. It stops 25 s after the game process appears, after 6 refocuses, or after 2 minutes; does nothing if the game was already running, if the launcher is minimised or in the tray, or if you switch to any other program. Activity: "Game launched · GHost Reforged brought back to the front". (`src/game_control.rs`, `reclaim_focus_after_launch`; profile field `refocus_after_launch`.) The W3Champions launcher does the same (refocuses its window after launch actions).
- **Exit W3 ▸** link in the status bar (under Game view), shown while Warcraft III runs. Asks the game to close (WM_CLOSE), waits 6 s, then ends the process. If hosting is running it stops hosting first (otherwise the bot would start the game again). During a match it needs a second click within 5 s ("Click again to exit ▸"). `game_control::exit_game`, `Message::ExitGame`.
- **Leave game** button: during a match the Force start button becomes Leave game (click twice within 5 s: "Confirm leave"). Sends `LeaveGame`; the normal end-of-match handling then counts the match, reads the replay and hosts the next lobby (if "Host the next lobby after each match" is on). Leaving early may count as a leave on W3C/FLO. `Control::LeaveGame`.
- **"Stop also exits W3"** live switch in the status bar. When on, Stop Hosting closes Warcraft III once hosting has stopped. Saved with the profile immediately (`exit_game_on_stop`, off by default).

### Changed
- Status bar layout, borrowing from the W3Champions launcher: one contextual main button (Force start in the lobby → Leave game in a match), game tools grouped under Game view.
- Tests: `focus_is_reclaimed_only_from_battlenet_or_the_new_game_window`, `reclaiming_stops_after_the_launch_settles_or_the_limit`, `a_confirmation_click_expires`, `exit_is_offered_only_while_the_game_runs`.
- Bumped the bot and Windows installer to 0.22.0. Not compiled in the editing environment.

## 0.21.0 - 2026-11-27

### Added
- **Battle.net sign-in check** (`src/battlenet.rs`, new), like the W3Champions launcher: reads Battle.net's own client log (`%LOCALAPPDATA%\Battle.net\Logs\battle.net-*.log`, newest file, last 4 MB). The last of `Logged into Battle.net successfully` / `Account manager state has changed to: Ready` (signed in) or `Disconnected from Battle.net` / `... Incomplete` (signed out) wins. Checked against the user's real log. No credentials or tokens are read.
- When hosting starts and Battle.net is running but signed out, the bot opens Battle.net and says "Battle.net is not signed in. Sign in to Battle.net; hosting continues automatically." instead of launching the game into a login screen; the normal retry picks it up once signed in.
- Hosting status bar: "Battle.net is not signed in." with an **Open Battle.net ▸** link. The header status light's tooltip adds the Battle.net state. Activity logs each change. Polled every 3 s with the game process (`Message::BattleNetObserved`, `App::battlenet`).

### Fixed
- **Battle.net / Warcraft III installed on another drive.** The launcher path was hard-coded to `%ProgramFiles(x86)%\Battle.net` (C:), so with `D:\Program Files (x86)\Battle.net` the bot could not start the game ("Start Warcraft III before hosting on this installation"). It now uses the running Battle.net.exe, else searches `Program Files (x86)`, `Program Files` and the root of drives C–Z (`battlenet::find_launcher`, `find_game`).
- Tests: `the_last_sign_in_signal_in_the_log_wins`, `install_folders_on_other_drives_are_searched`.
- Bumped the bot and Windows installer to 0.21.0. Not compiled in the editing environment.

## 0.20.0 - 2026-11-27

### Added
- **Bring Warcraft III to the front when a game starts** (Play as host, Options; on by default). When the match starts loading, the bot restores the game window if minimised and brings it forward (a tap of Alt lets a background process take the foreground). Retries for up to 5 s while the window appears; Activity logs the result. `end_panel::focus`, `Config::focus_on_start`, profile field `focus_on_start`.

### Fixed
- **Replay folder `Replays`.** Auto-detection now checks both `Replay` and `Replays`, in `Documents/Warcraft III` and every `BattleNet/<n>` account (`replays::candidates`). The user's replays are in `BattleNet\0\Replays`.
- **"Checking…" forever on the Last game card.** Lookups still running when the game started were discarded, and the next lobby cleared the last game's levels. Running lookups now finish after the game starts (`DsCheck::game_started`, nothing is kicked or announced), and the last game's levels are kept when the next lobby opens.
- **Last game card layout.** Each player is one name line plus one detail line ("Lv 804 · 107 matches · Passed") with the Ban button on the right, so names no longer collide with the numbers in the narrow panel. Replay leavers use the same two-line layout. The no-replay fallback lists one "Left early" button per player instead of a sideways strip. Section order: Last game → leavers / replay prompt → Players → ban reason.
- **Brand ghost cut off.** The header brand has 34 px side padding so the ghost sits inside the frame's ornate ends.
- Tests: `both_replay_and_replays_folders_are_searched_in_every_account`, `lookups_running_when_the_game_starts_still_report_levels`, `game_rows_read_as_one_short_detail_line`.
- Bumped the bot and Windows installer to 0.20.0. Not compiled in the editing environment.

## 0.19.0 - 2026-11-27

### Changed
- **Auto-exit instead of clicking Quit.** When the Direct Strike end panel (VICTORY/DEFEAT) appears, the bot now leaves the game (`LeaveGame`) after the delay, instead of clicking the map's Quit button, which did not always end the match. If the game is still running 15 s later it tries once more. Leaving skips the map's Quit handler, so W3C/FLO may record it as a leave. Options: "Exit the game when the Direct Strike end panel appears" + delay; the old "If Quit cannot be pressed, leave the game" toggle is removed (its profile field is kept so saved profiles still load).
- Leaving a running player-host match straight to a menu (no results screen) now counts the match, reads its replay and rehosts, the same as a results screen.
- **Direct Strike only.** These features now apply only when the map file name contains "Direct Strike" (`config::is_direct_strike_map`, ignoring spaces, `_` and case): the end-panel auto-exit, ds-report lookups / level filter / stats announcements (including your own), and automatic early-leave bans. For other maps the welcome message works as before, early leavers are still listed with a Ban button, and the folded Host settings show "Map type: Other (DS features off)".
- Tests: `the_game_is_left_after_two_sightings_and_the_delay_then_retried_once`, `only_direct_strike_maps_are_watched`, `leaving_a_player_match_straight_to_a_menu_counts_it_and_rehosts`, `other_maps_only_list_early_leavers`; settings-grid and replay tests updated.
- Bumped the bot and Windows installer to 0.19.0. Built on 0.18.4; not compiled in the editing environment.

## 0.18.4 - 2026-11-27

### Fixed
- **Play-as-host got stuck after a match** ("Waiting for Battle.net…", "Waiting for a fresh Battle.net session", repeating) until Host was stopped and started again. After closing the results, the bot now reuses the signed-in session the same way a fresh Host click does: `ScoreScreenClose` + `GetGameList`, then creates the lobby from an online listing or a provider confirmation. It no longer waits for a menu callback or re-initializes the provider. A reconnect while waiting also skips re-initialization for a player host (`Engine::recover`); the observer host is unchanged.
- **Header tabs**: labels were drawn at the top of the tab and clipped by its frame; they are centred now (padding 14 / 12).
- Tests updated for the new return path; new `a_player_host_rehosts_from_an_online_listing_without_reinitializing`.
- Bumped the bot and Windows installer to 0.18.4.

## 0.18.3 - 2026-11-27

### Fixed
- UI test `rendered_menu_opens_selects_and_closes_without_clicking_underneath` still could not reach the Auto start menu. Since 0.17.4 the expanded host settings shared the Auto Host panel 3 : 2 with Recent activity, which pushed the Auto start row below the visible part of its scroll area. While editing, the settings form now uses the whole panel; Recent activity shows again when the settings are folded. Real app fix: the Auto start menu was hard to reach by hand too.
- Bumped the bot and Windows installer to 0.18.3.

## 0.18.2 - 2026-11-27

### Fixed
- UI test `rendered_menu_opens_selects_and_closes_without_clicking_underneath` could not find the Auto start menu. Since 0.18.0 the test's click search also scans the header, where hovering the Warcraft III status light (a tooltip) marks the layout outdated; later clicks on the stale layout were ignored. `Harness::find` now rebuilds the interface whenever an update reports it outdated. Test-only change; no app behaviour changed.
- Bumped the bot and Windows installer to 0.18.2.

## 0.18.1 - 2026-11-27

### Fixed
- Build error in 0.18.0: the status-bar text helper took the font size as `u16`, which `iced::Pixels` does not accept. It now takes `u32`. No behaviour change.
- Bumped the bot and Windows installer to 0.18.1. The push-triggered Windows workflow compiles and tests it.

## 0.18.0 - 2026-11-27

### Changed
- **Navigation moved from the left sidebar to the header.** Hosting, Profiles, Options and Activity are tabs next to the brand; pages now use the full window width. The header is 92 px tall (was 166), giving every page about 74 px more height.
- The Warcraft III status (Online / Offline / Checking) is a coloured light next to the brand, with the details in its tooltip.
- **Hosting status bar no longer crops text.** The status and notice lines stay on one line, are shortened with "…" past 90 characters (full text in a tooltip and on the Activity page), and the notice is hidden when it only repeats the status.
- Dragging the window works on the brand and on the empty header space.
- Backdrop strips follow the new layout (`backdrop_view::LEFT`, `TAB_HEIGHT`, `TAB_TOP`; `SIDEBAR` removed). UI test search now scans from the top of the window.
- Test: `status_text_is_one_short_line`; backdrop test points updated for the full-width layout.
- Bumped the bot and Windows installer to 0.18.0. No hosting behaviour changed. Not compiled in the editing environment; the push-triggered Windows workflow runs the tests.

## 0.17.4 - 2026-11-27

### Changed
- Hosting page: Recent activity has its own scroll area and fills the rest of the Auto Host panel, newest first (up to 60 lines; the full log stays on the Activity page). Host settings, Last game and Recent kicks scroll separately above it. Folded, they take only the height they need (at most 300 px); while editing, they share the panel 3 : 2 with Recent activity.
- Bumped the bot and Windows installer to 0.17.4. No hosting behaviour changed. Not compiled in the editing environment; the push-triggered Windows workflow runs the tests.

## 0.17.3 - 2026-11-27

### Fixed
- Windows release CI excludes the two native audio integration tests that require an active playback endpoint. All other tests remain in the release gate. Run the audio tests separately on a Windows machine with an active playback endpoint.
- The backdrop test tolerates subpixel rounding when comparing clipped rectangles, without relaxing its overlap or coverage checks. The replay observer test now supplies the host identity so it tests observer exclusion without treating the host as an eligible candidate.
- Bumped the bot and Windows installer to 0.17.3. No runtime behaviour changed. Windows tests have not been rerun in the editing environment.

## 0.17.2 - 2026-11-27

### Fixed
- Windows release CI filters only the native mute integration proof, which requires an active playback endpoint. Its failure while holding the presentation mutex also caused the window tests to fail with a poisoned lock. Window tests remain part of the CI release gate. The audio test remains available on a Windows machine with an active audio endpoint.
- Updated two stale assertions: removal chat is already prefixed by `Removing`, and results for an owned player-host lobby close after the configured rehost delay, unlike results encountered at startup. No bot runtime changes.
- Bumped the bot and Windows installer to 0.17.2. The Windows suite was not run in the editing environment.

## 0.17.1 - 2026-11-27

### Changed
- Version bumps committed to `main` can now create their Windows release automatically. Commit the workflow and all four Cargo version files together. The Windows workflow checks both manifests, runs locked tests, and builds the EXE and installer before pushing a new `vMAJOR.MINOR.PATCH` tag for the exact built commit and publishing the assets.
- Existing tags are not moved or republished by the automatic build. Manually pushed `v*` tags still use the tag build. A manual run on a branch builds artifacts without publishing; if the automatic run pushed a tag but failed to publish the release, run the workflow manually on that tag to attach the assets.
- Bumped the bot and Windows installer to 0.17.1. No hosting or updater behavior changed.

## 0.17.0 — 2026-11-26

### Added
- **End buffer for early leavers.** Leaving in the last *N* minutes of a match (default **2**) no longer counts as leaving early, since the result is already decided. Leaving before the **limit** (default **8 min**) always counts, even when the buffer would cover it (short matches). `replays::Window { limit_ms, buffer_ms }`, `Window::is_early`, `early()`, `near_end()`.
- Options → Early leavers: **Limit** stepper (1–30 min) and **End buffer** stepper (0–10 min). Profile fields `early_leave_minutes` (default 8) and `end_buffer_minutes` (default 2); old profiles load with the defaults.
- Last game card lists end-buffer leavers in grey as "in the last N min, not counted"; Activity marks them "(end buffer, not counted)".
- Test: `the_end_buffer_forgives_late_leavers_but_never_leaves_before_the_limit`.

### Changed
- Rule *Before 7 min* is now **Before the limit** and uses the limit setting (serialised name unchanged, so 0.16.0 profiles still load).
- The "match ended early" marker on the Last game card still uses the map's own 7-minute rule.
- Version 0.16.0 → 0.17.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.

## 0.16.0 — 2026-11-26

### Added
- **Early leavers from the replay.** After each Play-as-host match the launcher reads `LastReplay.w3g` (`src/replay.rs`, new) and lists who left and when. Checked against a real W3C replay: players, match length (28:01) and the one early leave (23:56) all read correctly; observers (FLO) are skipped; players recorded leaving within 3 s of the end left with everyone and do not count.
  - Parser: 28-byte header, zlib blocks inflated without a stream end (each block is flushed, not finished), player records, 0x39 metadata, slot table (observers = team 24), leave records (0x17) and time slots (0x1E/0x1F). Unknown blocks stop the scan without failing. Dependency: `flate2` (already in Cargo.lock; added to `ghost-reforged`'s list).
- **Automatic early-leave bans** (Options → Early leavers): *Before 7 min* (default), *Any early leave*, or *Off (list only)*. Reason saved: `Left the game early (m:ss)`. Never the host, never a name without a BattleTag. Banned players get the native ban when they join a later lobby (0.10.0).
- **Last game card:** match length from the replay; each early leaver with time and "before the end", marked *Banned automatically* or with a **Ban** button. The old manual "Left early" buttons remain only when no replay could be read.
- **Replay folder:** found automatically (newest `Documents/Warcraft III/BattleNet/<n>/Replay/LastReplay.w3g`, also `Documents/Warcraft III/Replay`). If not found, the Last game card shows **Choose Replay folder** / **Find automatically**; the same controls are in Options. The choice is saved in `replay-folder.txt` in the data folder. Folder picker: `ghost_pick_folder` in `src/native/map_dialog.cpp`.
- The replay must be newer than the match start (wall clock), so an old replay is never used; the launcher waits up to ~22 s for it.
- **NEXT.md**: list of ideas and open items not done yet.
- Tests: `players_leaves_and_length_are_read_and_observers_skipped`, `other_files_are_rejected_and_a_cut_replay_keeps_what_was_read`, `the_rule_picks_who_to_ban_and_never_the_host_or_end_of_match_leavers`, `reading_a_replay_bans_by_rule_and_a_missing_one_asks_for_the_folder`, `the_chosen_folder_is_saved_and_cleared`.

### Changed
- Profile field `early_leave_rule` (default *Before 7 min*; old profiles load unchanged).
- `maps::documents_directory` is shared with the replay module.
- Version 0.15.0 → 0.16.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- The parser logic was run (as a Python mirror) against one real replay only; the Rust tests use a synthetic replay in the same layout.
- The folder picker C++ was not compiled here.

## 0.15.0 — 2026-11-26

### Changed (Hosting page redesign, step 2)
- **Status bar across the top** (`hosting_view::status_bar`, height `STATUS_HEIGHT` 96): lobby name (large), players X / Y or "In game" / "Next game", the coloured status line, the notice line, a **Game view** toggle, and Host / Force start / Stop Hosting. The left panel no longer has a footer.
- **Wider lobby:** columns are now settings 2 : lobby 3 (`SETTINGS_PORTION` / `LOBBY_PORTION`), previously 3 : 2.
- **Game View is optional** (`App::show_preview`, `Message::TogglePreview`, off by default). Off: the Lobby / Current Game panel uses the full height. On: Game View appears under it as before.
- Backdrop strips follow the new layout (`Coverage::Hosting(preview)`: gap under the status bar, gap between the columns, gap above Game View only when shown).
- **UI tests no longer use fixed click coordinates** for moving controls: `Harness::find` clicks a grid on one built frame and returns the first point that produces the wanted message (Auto start menu, its "Full" item, Host, Kick, Slot menu and "Add computer"). Window controls and the update modal keep their fixed points.
- Test: `the_game_view_is_optional_and_the_status_bar_toggles_it`; backdrop tests cover `Hosting(false)` and `Hosting(true)`.
- Version 0.14.0 → 0.15.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Not visually checked. At 1000 px wide a long lobby name may wrap in the status bar.

## 0.14.0 — 2026-11-26

### Changed (Hosting page redesign, step 1)
- **One player list.** The Lobby panel is now the only place players are listed: each occupied row shows `Lv 804 · 107` (coloured by the live DS filter: red below minimum / not on W3C, gold lookup failed, muted otherwise) between the name and the Slot/Kick/Ban controls. Your own row at the bottom shows your level too. The duplicate **Players** table on the left is gone.
- **Left panel = control and feedback:** Host settings → Last game (after a match) → Recent kicks → **Recent activity** (last 10 Activity lines with an **Open log ▸** link), so the empty space now shows what the bot is doing.
- Control positions unchanged (Slot/Kick/Ban stay right-aligned in the lobby row; Host/Force start/Stop stay in the footer), so existing UI click tests keep their coordinates.
- `hosting_view::level_badge`, `recent_activity`; `players` kept but unused.
- Version 0.13.1 → 0.14.0.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Not visually checked.

## 0.13.1 — 2026-11-26

### Fixed
- **ds-report lookups failing ("Lookup failed").** "Failed" means ds-report answered with something other than stats or 404/500 (most likely a bot-filter 403/429 page) or the connection failed; the player name sent was correct (`L0v31yC0rn#1238`, confirmed in `w3-protocol.log`). Requests now send a browser User-Agent and Accept/Accept-Language headers like the site's own page; ureq's default `ureq/2.x` agent is commonly blocked.
- Activity now includes the start of ds-report's reply on failure, e.g. `HTTP 403: <html> <title>Just a moment...`, so the exact cause is visible (`ds_report::snippet`).
- Test: `reply_snippets_are_short_single_lines`.

### Changed
- Version 0.13.0 → 0.13.1.

### Not verified
- Not compiled or run here (no Rust toolchain). Run `cargo test`.
- Could not reach ds-report from the editing environment, so the header fix is untested. `w3-protocol.log` only records the game's UI socket, not ds-report traffic.

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
