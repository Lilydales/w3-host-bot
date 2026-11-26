# Changelog

Versioning (semver, `MAJOR.MINOR.PATCH`): every change set bumps the version.
- PATCH (0.5.0 → 0.5.1): fixes, CI/docs-only tweaks.
- MINOR (0.5.x → 0.6.0): new features or settings.
- MAJOR: breaking changes (config/ban file formats). Stays 0 until a stable 1.0.
Bump it in 4 places: `Cargo.toml`, `Cargo.lock` (package `ghost-reforged`),
`packaging/windows/setup/Cargo.toml`, `packaging/windows/setup/Cargo.lock`.

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
