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

fn bans(lines: &str) -> (BanList, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(crate::bans::FILE_NAME);
    std::fs::write(&path, lines).unwrap();
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
