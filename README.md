# GHost Reforged

<img width="1710" height="1230" alt="image" src="https://github.com/user-attachments/assets/e330961d-d3ec-4e68-9d1c-1814ac907147" />

A Warcraft III auto-host for Windows, written in Rust.

Download [ghost-reforged.exe](https://github.com/Lilydales/w3-host-bot/releases/latest/download/ghost-reforged.exe) and run it. No installation required.

For a Start menu shortcut, run the [installer](https://github.com/Lilydales/w3-host-bot/releases/latest/download/GHost-Reforged-Setup-x64.exe), or paste this into PowerShell:

```powershell
irm https://github.com/Lilydales/w3-host-bot/releases/latest/download/install.ps1 | iex
```

Sign in to Battle.net, select a map from `Documents/Warcraft III/Maps` and click **Host**. Windows 10/11, 64-bit. No administrator rights required.

Auto start defaults to **Full**. Choose a minimum player count or use **Force start**. Players can use **!votestart** and **!votekick name**. Edit commands and the welcome message in **Options**, including while hosting. **Stop Hosting** closes the current lobby and stops new ones. Save presets in **Profiles**.

**!status** shows what the lobby is waiting for; **!yes** joins the active vote; **!from [name]** shows Battle.net regions; **!map** shows the map; **!color color** changes your color when the map permits it; **!stats [name]** shows games started here and first/last seen dates. Statistics stay on the hosting computer and survive restarts. Use **!help** for the enabled list or **!help command** for details.

**Banning players.** Click **Ban** next to a player in the lobby. They are removed at once, and anyone on the list is removed again whenever they rejoin. The list is a plain text file, `ban.txt`, saved next to the program (or in the data folder if that location is not writable). The path is shown in the activity log when hosting starts. One player per line: `Name#1234` bans that exact BattleTag, a bare `Name` bans every BattleTag with that name, and lines starting with `//` are comments. Edit or delete the file while hosting and the change applies within a second. Warcraft III cannot block a join in advance, so a banned player may appear in the lobby for a moment before being removed.

Closing the window keeps hosting in the system tray. Click the ghost to reopen it, or right-click and choose **Quit** to exit.

Early test build for Warcraft III **3.0.0.24268**. The game runs in the background and the bot uses an observer slot. The host appears with a blank name in the lobby, including for joining players.

Build from source: `powershell -File packaging/windows/build.ps1 -Setup` (Rust and the Visual Studio C++ build tools required).

For the people selling hosting solutions with closed source softwares: Welcome to the open source world.

## Changes from upstream

This fork of [shideneyu/ghost-reforged](https://github.com/shideneyu/ghost-reforged) (Apache 2.0) changes the chat command prefix from `?` to `!` and adds the persistent ban list (`src/bans.rs`, the **Ban** button, and automatic removal in `src/hosting/`).

## Temporary diagnostics: W3 protocol logging

Investigation only, **off by default** (enable with `W3_PROTOCOL_LOG=1`). When on, each text frame exchanged with Warcraft III's local UI WebSocket is written as one compact line to `w3-protocol.log` (repeats of the same frame skipped; capped at 5 MB, rotated once to `w3-protocol.previous.log`) (current working directory; override with `W3_PROTOCOL_LOG_PATH`). RECV frames are logged before deserialization, SEND frames are the exact serialized frame. Keys containing password/token/cookie/secret/authorization/ticket are redacted. 

Disable: set `W3_PROTOCOL_LOG=0`. Remove: delete `src/protocol_log.rs`, the `pub mod protocol_log;` line in `src/lib.rs`, and the four `protocol_log::` calls in `src/transport.rs` (marked `TEMP`).

## Native ban (experimental, v0.4.2)

`Warcraft_III.exe` lists a `BanPlayerFromGameLobby` command beside `KickPlayerFromGameLobby`. The **Ban** button now saves the player to `ban.txt` as before, then sends `BanPlayerFromGameLobby` with `{"slot": N}` instead of a plain kick. The payload is a guess based on Kick and is not verified. If the game ignores it, the player stays in the lobby and the existing ban-list enforcement removes them within seconds, so nothing is lost. Check `w3-protocol.log` for how the game replied. Set `W3_NATIVE_BAN=0` to send a plain kick again. Manual **Kick** and vote-kick are unchanged.
