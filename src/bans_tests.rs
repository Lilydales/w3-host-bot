use super::*;
use std::fs;

fn list_in(directory: &tempfile::TempDir) -> (BanList, PathBuf) {
    let path = directory.path().join(FILE_NAME);
    (BanList::open(Some(&path)), path)
}

#[test]
fn a_missing_file_is_created_with_instructions_and_bans_nobody() {
    let directory = tempfile::tempdir().unwrap();
    let (list, path) = list_in(&directory);
    assert!(list.is_empty());
    assert!(fs::read_to_string(path).unwrap().starts_with("//"));
    assert!(!list.is_banned("Anyone#1234"));
}

#[test]
fn comments_blank_lines_case_and_byte_order_marks_are_handled() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(
        &path,
        "\u{feff}// header\n\n  Cheater#1234  \r\nSPAMMER\n// Skipped#1\n",
    )
    .unwrap();
    let list = BanList::open(Some(&path));
    assert_eq!(list.len(), 2);
    assert!(list.is_banned("cheater#1234"));
    assert!(list.is_banned("CHEATER#1234"));
    assert!(!list.is_banned("Skipped#1"));
}

#[test]
fn a_full_battletag_is_exact_and_a_bare_name_covers_every_tag() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(&path, "Exact#1111\nAnyTag\n").unwrap();
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
fn adding_saves_to_disk_survives_a_restart_and_ignores_duplicates() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, path) = list_in(&directory);
    assert_eq!(list.add("Troll#4242"), Ok(true));
    assert_eq!(list.add("troll#4242"), Ok(false));
    assert!(list.is_banned("Troll#4242"));
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("Troll#4242").count(), 1);
    let reopened = BanList::open(Some(&path));
    assert!(reopened.is_banned("TROLL#4242"));
    assert_eq!(reopened.len(), 1);
}

#[test]
fn adding_after_a_hand_edit_without_a_final_newline_keeps_lines_separate() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(&path, "First#1").unwrap();
    let mut list = BanList::open(Some(&path));
    assert_eq!(list.add("Second#2"), Ok(true));
    let reopened = BanList::open(Some(&path));
    assert!(reopened.is_banned("First#1"));
    assert!(reopened.is_banned("Second#2"));
}

#[test]
fn hand_edits_apply_without_restarting_and_deleting_the_file_clears_the_list() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, path) = list_in(&directory);
    assert!(!list.is_banned("Late#1"));
    fs::write(&path, "Late#1\n").unwrap();
    list.reload();
    assert!(list.is_banned("Late#1"));
    fs::write(&path, "// everyone is forgiven\n").unwrap();
    list.reload();
    assert!(!list.is_banned("Late#1"));
    fs::write(&path, "Again#1\n").unwrap();
    list.reload();
    assert!(list.is_banned("Again#1"));
    fs::remove_file(&path).unwrap();
    list.reload();
    assert!(list.is_empty());
}

#[test]
fn unusable_names_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let (mut list, _) = list_in(&directory);
    assert!(list.add("").is_err());
    assert!(list.add("   ").is_err());
    assert!(list.add("two\nlines").is_err());
    assert!(list.add("// comment").is_err());
    assert!(list.add(&"x".repeat(200)).is_err());
    assert!(list.is_empty());
}

#[test]
fn without_a_location_nothing_is_saved_but_the_error_is_clear() {
    let mut list = BanList::open(None);
    assert!(list.add("Someone#1").is_err());
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
