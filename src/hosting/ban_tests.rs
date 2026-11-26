use super::*;
use serde_json::json;

fn lobby(host: bool, players: &[(i64, &str, u8, u8, bool)]) -> Lobby {
    serde_json::from_value(json!({
        "isHost": host, "lobbyName": "Test #1", "mapData": {"mapPath": "map.w3x"},
        "players": players.iter().enumerate().map(|(slot, (id, name, status, kind, own))| json!({
            "id": id, "name": name, "slot": slot, "slotStatus": status,
            "slotType": kind, "isSelf": own, "team": 0, "isObserver": false, "teamChangeEnabled": true,
        })).collect::<Vec<_>>(),
    }))
    .unwrap()
}

/// Builds a ban list from names, one per line (`//` lines are ignored), saved as JSON.
fn bans(lines: &str) -> (BanList, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(crate::bans::FILE_NAME);
    let entries: Vec<_> = lines
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|player| json!({"player": player, "reason": "test", "banned_at": 1}))
        .collect();
    std::fs::write(&path, serde_json::to_string(&entries).unwrap()).unwrap();
    (BanList::open(Some(&path)), directory)
}

#[test]
fn only_a_real_banned_guest_is_targeted() {
    let (list, _guard) = bans("Cheater#1\nBot#2\nBlank#3\nHost#4\n");
    let room = lobby(
        true,
        &[
            (0, "Host#4", 2, 0, true),        // the host is never a target
            (1, "Friend#9", 2, 0, false),     // not banned
            (2, "Bot#2", 2, 1, false),        // computer slot
            (3, "Blank#3", 0, 0, false),      // empty slot
            (4, "", 2, 0, false),             // name not resolved yet
            (5, "Cheater#1", 2, 0, false),    // the one to remove
        ],
    );
    let target = BanEnforcer::default().next(&room, &list, 10).unwrap();
    assert_eq!(target.name(), "Cheater#1");
    assert_eq!(target.slot, 5);
}

#[test]
fn nothing_happens_when_this_bot_is_not_the_lobby_host() {
    let (list, _guard) = bans("Cheater#1\n");
    let room = lobby(false, &[(1, "Cheater#1", 2, 0, false)]);
    assert!(BanEnforcer::default().next(&room, &list, 10).is_none());
}

#[test]
fn an_empty_ban_list_never_removes_anyone() {
    let (list, _guard) = bans("// nobody\n");
    let room = lobby(true, &[(1, "Anyone#1", 2, 0, false)]);
    assert!(BanEnforcer::default().next(&room, &list, 10).is_none());
}

#[test]
fn the_same_player_is_retried_only_after_a_pause_and_others_go_first() {
    let (list, _guard) = bans("First#1\nSecond#2\n");
    let room = lobby(
        true,
        &[(1, "First#1", 2, 0, false), (2, "Second#2", 2, 0, false)],
    );
    let mut enforcer = BanEnforcer::default();
    assert_eq!(enforcer.next(&room, &list, 10).unwrap().name(), "First#1");
    assert_eq!(enforcer.next(&room, &list, 11).unwrap().name(), "Second#2");
    assert!(enforcer.next(&room, &list, 12).is_none());
    assert_eq!(enforcer.next(&room, &list, 14).unwrap().name(), "First#1");
}

#[test]
fn a_bare_name_ban_catches_any_tag_and_the_request_matches_the_live_lobby() {
    let (list, _guard) = bans("Griefer\n");
    let room = lobby(
        true,
        &[(0, "Me#1", 2, 0, true), (3, "griefer#8888", 2, 0, false)],
    );
    let target = BanEnforcer::default().next(&room, &list, 1).unwrap();
    assert!(target.matches(&room));
    let moved = lobby(
        true,
        &[(0, "Me#1", 2, 0, true), (4, "griefer#8888", 2, 0, false)],
    );
    assert!(!target.matches(&moved));
}

#[test]
fn a_confirmed_ban_with_a_reason_is_stored_and_then_enforced() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(crate::bans::FILE_NAME);
    let mut list = BanList::open(Some(&path));
    let room = lobby(
        true,
        &[(0, "Me#1", 2, 0, true), (1, "Rude#5", 2, 0, false)],
    );
    assert!(BanEnforcer::default().next(&room, &list, 1).is_none());
    assert_eq!(list.add("Rude#5", "insulted players"), Ok(true));
    let target = BanEnforcer::default().next(&room, &list, 1).unwrap();
    assert_eq!(target.name(), "Rude#5");
    assert_eq!(list.get("Rude#5").unwrap().reason, "insulted players");
    assert_eq!(BanList::open(Some(&path)).get("rude#5").unwrap().reason, "insulted players");
}

#[test]
fn a_ban_without_a_reason_still_removes_the_player() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(crate::bans::FILE_NAME);
    let mut list = BanList::open(Some(&path));
    let room = lobby(true, &[(1, "Quiet#2", 2, 0, false)]);
    assert_eq!(list.add("Quiet#2", ""), Ok(true));
    assert!(list.get("Quiet#2").unwrap().reason.is_empty());
    assert!(BanEnforcer::default().next(&room, &list, 1).is_some());
}

#[test]
fn kick_reasons_are_readable_in_chat_and_never_start_a_command() {
    assert_eq!(ban_reason(Some("  spam ")), "Banned: spam");
    assert_eq!(ban_reason(Some("")), "On the ban list");
    assert_eq!(ban_reason(None), "On the ban list");
    let line = reason_chat("/x#1", "Banned: a\nb");
    assert_eq!(line.message, "SendGameChatMessage");
    assert_eq!(line.payload["content"], " Removing /x#1: Banned: ab");
}
