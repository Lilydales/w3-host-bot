use super::*;
use std::fs;

fn list_in(directory: &tempfile::TempDir) -> (BanList, PathBuf) {
    let path = directory.path().join(FILE_NAME);
    (BanList::open(Some(&path)), path)
}

fn names(path: &Path) -> Vec<BanEntry> {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_missing_file_is_created_as_an_empty_json_list_and_bans_nobody() {
    let directory = tempfile::tempdir().unwrap();
    let (list, path) = list_in(&directory);
    assert!(list.is_empty());
    assert!(names(&path).is_empty());
    assert!(!list.is_banned("Anyone#1234"));
}

#[test]
fn entries_are_read_with_reason_and_timestamp_and_case_is_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(
        &path,
        "\u{feff}[{\"player\":\"  Cheater#1234 \",\"reason\":\"maphack\",\"banned_at\":1700000000},\
          {\"player\":\"SPAMMER\"},{\"player\":\"\"}]",
    )
    .unwrap();
    let list = BanList::open(Some(&path));
    assert_eq!(list.len(), 2);
    assert!(list.is_banned("cheater#1234"));
    assert!(list.is_banned("CHEATER#1234"));
    let entry = list.get("cheater#1234").unwrap();
    assert_eq!(entry.player, "Cheater#1234");
    assert_eq!(entry.reason, "maphack");
    assert_eq!(entry.banned_at, 1_700_000_000);
    let bare = list.get("spammer").unwrap();
    assert_eq!((bare.reason.as_str(), bare.banned_at), ("", 0));
}

#[test]
fn a_full_battletag_is_exact_and_a_bare_name_covers_every_tag() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(&path, r#"[{"player":"Exact#1111"},{"player":"AnyTag"}]"#).unwrap();
    let list = BanList::open(Some(&path));
    assert!(list.is_banned("Exact#1111"));
    assert!(!list.is_banned("Exact#2222"));
    assert!(!list.is_banned("Exact"));
    assert!(list.is_banned("AnyTag#1"));
    assert!(list.is_banned("anytag#9999"));
    assert!(list.is_banned("AnyTag"));
    assert!(!list.is_banned("AnyTagger#1"));
    assert!(!list.is_banned(""));
    assert!(!list.is_banned("   "));
}

#[test]
fn adding_saves_reason_and_time_survives_a_restart_and_ignores_duplicates() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, path) = list_in(&directory);
    assert_eq!(list.add("Troll#4242", "  spamming\nchat "), Ok(true));
    assert_eq!(list.add("troll#4242", "other"), Ok(false));
    assert!(list.is_banned("Troll#4242"));
    let saved = names(&path);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].player, "Troll#4242");
    assert_eq!(saved[0].reason, "spamming chat");
    assert!(saved[0].banned_at > 1_600_000_000);
    let reopened = BanList::open(Some(&path));
    assert!(reopened.is_banned("TROLL#4242"));
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened.get("troll#4242").unwrap().reason, "spamming chat");
}

#[test]
fn a_reason_is_optional_and_capped() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, path) = list_in(&directory);
    assert_eq!(list.add("Quiet#1", ""), Ok(true));
    assert_eq!(list.add("Wordy#2", &"x".repeat(500)), Ok(true));
    let saved = names(&path);
    let quiet = saved.iter().find(|e| e.player == "Quiet#1").unwrap();
    let wordy = saved.iter().find(|e| e.player == "Wordy#2").unwrap();
    assert_eq!(quiet.reason, "");
    assert_eq!(wordy.reason.len(), MAX_REASON);
}

#[test]
fn adding_keeps_entries_that_were_hand_edited_in_since_the_last_load() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, path) = list_in(&directory);
    fs::write(&path, r#"[{"player":"First#1","reason":"manual"}]"#).unwrap();
    assert_eq!(list.add("Second#2", "bot"), Ok(true));
    let reopened = BanList::open(Some(&path));
    assert_eq!(reopened.get("First#1").unwrap().reason, "manual");
    assert!(reopened.is_banned("Second#2"));
}

#[test]
fn hand_edits_apply_without_restarting_and_deleting_the_file_clears_the_list() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, path) = list_in(&directory);
    assert!(!list.is_banned("Late#1"));
    fs::write(&path, r#"[{"player":"Late#1"}]"#).unwrap();
    list.reload();
    assert!(list.is_banned("Late#1"));
    fs::write(&path, "[]").unwrap();
    list.reload();
    assert!(!list.is_banned("Late#1"));
    fs::write(&path, r#"[{"player":"Again#1","reason":""}]"#).unwrap();
    list.reload();
    assert!(list.is_banned("Again#1"));
    fs::remove_file(&path).unwrap();
    list.reload();
    assert!(list.is_empty());
}

#[test]
fn a_broken_file_keeps_the_previous_list_warns_and_is_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(&path, r#"[{"player":"Kept#1"}]"#).unwrap();
    let mut list = BanList::open(Some(&path));
    fs::write(&path, "[{ oops").unwrap();
    list.reload();
    assert!(list.is_banned("Kept#1"));
    assert!(list.warning().unwrap().contains("not valid JSON"));
    assert!(list.add("New#2", "").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "[{ oops");
}

#[test]
fn an_old_plain_text_list_is_imported_once_and_kept_as_a_backup() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join(LEGACY_FILE_NAME);
    fs::write(
        &legacy,
        "\u{feff}// header\n\n  Cheater#1234  \r\nSPAMMER\n// Skipped#1\n",
    )
    .unwrap();
    let (list, path) = list_in(&directory);
    assert_eq!(list.len(), 2);
    assert!(list.is_banned("cheater#1234"));
    assert!(list.is_banned("Spammer#7"));
    assert!(!list.is_banned("Skipped#1"));
    let saved = names(&path);
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(|e| e.reason == "Imported from ban.txt" && e.banned_at > 0));
    assert!(!legacy.exists());
    assert!(directory.path().join("ban.txt.migrated").exists());
    // A later start must not import anything again, even if a new ban.txt shows up.
    fs::write(&legacy, "Sneaky#9\n").unwrap();
    let reopened = BanList::open(Some(&path));
    assert_eq!(reopened.len(), 2);
    assert!(!reopened.is_banned("Sneaky#9"));
}

#[test]
fn an_empty_json_file_made_by_locate_still_imports_the_old_list() {
    let program = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::write(program.path().join(LEGACY_FILE_NAME), "Old#1\n").unwrap();
    let path = BanList::locate(Some(program.path()), data.path());
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
    let list = BanList::open(Some(&path));
    assert!(list.is_banned("Old#1"));
}

#[test]
fn unusable_names_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, _) = list_in(&directory);
    assert!(list.add("", "").is_err());
    assert!(list.add("   ", "").is_err());
    assert!(list.add("two\nlines", "").is_err());
    assert!(list.add("// comment", "").is_err());
    assert!(list.add(&"x".repeat(200), "").is_err());
    assert!(list.is_empty());
}

#[test]
fn without_a_location_nothing_is_saved_but_the_error_is_clear() {
    let mut list = BanList::open(None);
    assert!(list.add("Someone#1", "").is_err());
    assert!(!list.is_banned("Someone#1"));
}

#[test]
fn the_program_folder_is_preferred_and_the_data_folder_is_the_fallback() {
    let program = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    assert_eq!(
        BanList::locate(Some(program.path()), data.path()),
        program.path().join(FILE_NAME)
    );
    let missing = program.path().join("does-not-exist");
    assert_eq!(
        BanList::locate(Some(&missing), data.path()),
        data.path().join(FILE_NAME)
    );
    assert_eq!(
        BanList::locate(None, data.path()),
        data.path().join(FILE_NAME)
    );
}
