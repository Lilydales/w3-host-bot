# Changelog

## Unreleased

### Added
- **Ban list on the Options page** (`src/launcher/ban_list.rs`).
  - Type any BattleTag (`Name#1234`, or a bare `Name` for every tag) plus an optional reason and click **Ban**. Works while hosting or idle.
  - **Last game's players**: when a game starts, the launcher remembers its human guests. Each can be banned after the match with one click (uses the reason box).
  - The launcher writes `bans.json` directly; a running host re-reads it within ~1 s and removes listed players from the lobby. No new worker messages.
- Test: `manual_and_last_game_bans_are_saved_with_reason_and_remembered`.

### Changed
- Ban file location is resolved once at launcher start (`App::ban_file`) and shared with hosting.

### Not verified
- Not compiled or run: the editing environment had no Rust toolchain. Run `cargo test` and check the Options page layout on Windows.
- "Last game's players" is in memory only; it is lost when the launcher closes.
