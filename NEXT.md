# NEXT — ideas and open items

Things discussed but not done yet. Move an item to `CHANGELOG.md` when it ships.
Add new ideas at the bottom of the right section. Keep each item short: what, why, what is needed.

## Needs information first
- **Match results request (`GetGameScore`).** Exact match length, win/loss and scores after Quit. Need a `w3-protocol.log` covering a full match up to the results screen to see the request/reply format.
- **Final W3C scores in the replay.** In a W3C game the final `score`/`team` W3MMD values were missing (the game closed 2–3 s after the end). Check a `LastReplay.w3g` from a game *this bot* hosted; if the scores are there, show them on the Last game card.
- **Auto-quit tuning.** The end-panel detector uses one 640×360 screenshot. Collect Activity "End panel seen · match x / y" scores on SD/HD graphics and other window sizes; adjust `THRESHOLD` or add references if needed.
- **Who left early, live.** The UI socket never reports in-game leaves. The replay covers it after the match; nothing known for during the match.

## Features
- **Whisper banned players** the reason (`SendWhisperMessage`). Optional setting.
- **Advertise the lobby** in a Battle.net chat channel (`SendChatMessage` / `SendChatCommand`).
- **Invite friends** to the lobby (`FriendsSendInvitation`, `FriendsGetFriends`).
- **Report players** through Blizzard (`SendLobbyReport`).
- **Allow players not found on W3C** even when the DS filter is on (option), so a broken ds-report lookup cannot empty lobbies.
- **Ban list manager**: list, search and unban from the Options page (today: edit `bans.json`).
- **Keep "Last game" across launcher restarts** (currently in memory only).
- **Warn before quitting mid-match** ("You are in an active game") in the close dialog.
- **Close Battle.net and Warcraft III** together (W3Champions has this action), e.g. as an option on Exit W3.
- **Game session line** (from W3Champions' "Game Session: Lobby / Loading / Running / Ended"): one short state chip in the status bar instead of mixed status text.
- **Seat: check the DS colour layout.** `seat.rs` assumes `availableTeamColors` lists each team's colours. If "Your position" does not move you, send a `w3-protocol.log` from a DS lobby.
- **Background music** (looping track while idle/in lobby). Needs a mixer (PlaySound plays one sound at a time, so music would cut off cues).
- **Auto-login**: declined for now (plain-text password risk; Battle.net login is a web page). If revisited: Windows Credential Manager, and a protocol log of a real sign-in first.

## Maintenance
- **CI actions still `@v4`** in `.github/workflows/*.yml` (Node 20 warning). Move to `checkout@v6`, `cache@v5`, `upload-artifact@v6` (patch bump).
- **README is out of date**: says `ban.txt` (now `bans.json`), game build 3.0.0.24268 vs `Config::default()` 3.0.1.24371, no mention of Play-as-host, DS filter, replays.
- **Remove the temporary protocol logger** (`src/protocol_log.rs`, calls in `src/transport.rs`) when investigation is finished.
- **Long lobby names** may wrap in the Hosting status bar at 1000 px; shorten with "…".
