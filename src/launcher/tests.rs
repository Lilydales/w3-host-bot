use super::{
    app::{App, Field, Message},
    profile::{Profile, Profiles},
    storage::{StorageError, Store},
};
use std::fs;

#[test]
fn started_games_are_named_highlighted_activity_entries() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    app.record_activity(ghost_reforged::hosting::Activity::GameStarted(
        "Test #7".into(),
    ));
    let entry = app.logs.front().unwrap();
    assert_eq!(entry.message, "Game started · Test #7");
    assert!(entry.highlight);
}

#[test]
fn reservation_ack_is_idempotent_within_a_worker_but_cannot_be_reused_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path(), None);
    app.reserve_number(1).unwrap();
    app.reserve_number(1).unwrap();
    assert_eq!(app.form.number, "2");
    drop(app);
    let mut app = App::new(dir.path(), None);
    assert_eq!(app.form.number, "2");
    assert!(app.reserve_number(1).is_err());
}

#[test]
fn failed_reservation_preserves_the_current_number() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path(), None);
    fs::create_dir(dir.path().join("profiles.json")).unwrap();
    assert!(app.reserve_number(1).is_err());
    assert_eq!(app.form.number, "1");
    assert_eq!(app.profiles.items[0].next_number, 1);
}

#[test]
fn profile_roundtrip_preserves_counter_and_rejects_a_second_writer() {
    let dir = tempfile::tempdir().unwrap();
    let (store, mut profiles) = Store::open(dir.path()).unwrap();
    assert!(matches!(Store::open(dir.path()), Err(StorageError::Locked)));
    profiles.items[0].next_number = 42;
    store.save(&profiles).unwrap();
    drop(store);
    let (store, mut profiles) = Store::open(dir.path()).unwrap();
    assert_eq!(profiles.items[0].next_number, 42);
    profiles.items[0].next_number = 43;
    store.save(&profiles).unwrap();
    drop(store);
    let (_, profiles) = Store::open(dir.path()).unwrap();
    assert_eq!(profiles.items[0].next_number, 43);
}

#[test]
fn corrupt_and_future_profiles_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    fs::write(&path, "broken!").unwrap();
    assert!(matches!(Store::open(dir.path()), Err(StorageError::Format)));
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken!");
    let future = Profiles {
        version: 3,
        ..Profiles::default()
    };
    let bytes = serde_json::to_vec(&future).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        Store::open(dir.path()),
        Err(StorageError::Invalid(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn new_commands_migrate_once_without_reenabling_user_disabled_commands() {
    use ghost_reforged::commands::ChatCommand;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    let mut legacy = Profiles {
        version: 1,
        ..Profiles::default()
    };
    legacy.items[0]
        .commands
        .0
        .retain(|c| !ChatCommand::NEW.contains(c));
    legacy.items[0].commands.set(ChatCommand::VoteKick, false);
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let (store, mut upgraded) = Store::open(dir.path()).unwrap();
    assert_eq!(upgraded.version, 2);
    assert!(!upgraded.items[0].commands.enabled(ChatCommand::VoteKick));
    assert!(
        ChatCommand::NEW
            .into_iter()
            .all(|c| upgraded.items[0].commands.enabled(c))
    );
    upgraded.items[0].commands.set(ChatCommand::Color, false);
    store.save(&upgraded).unwrap();
    drop(store);
    let (_, loaded) = Store::open(dir.path()).unwrap();
    assert!(!loaded.items[0].commands.enabled(ChatCommand::Color));
    assert!(!loaded.items[0].commands.enabled(ChatCommand::VoteKick));
}

#[test]
fn failed_save_does_not_replace_the_in_memory_profile() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path(), None);
    fs::create_dir(dir.path().join("profiles.json")).unwrap();
    let _ = app.update(Message::Edit(Field::Number, "42".into()));
    let _ = app.update(Message::Save);
    assert!(app.notice_error);
    assert_eq!(app.profiles.items[0].next_number, 1);
}

#[test]
fn invalid_profiles_cannot_be_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let (store, mut profiles) = Store::open(dir.path()).unwrap();
    profiles.items[0].lobby_pattern = "\u{e9}".repeat(11) + "{n}";
    assert!(store.save(&profiles).is_err());
    profiles.items[0].lobby_pattern = "SC #{n}".into();
    profiles.selected = 10;
    assert!(store.save(&profiles).is_err());
}

#[test]
fn new_profile_and_switch_save_edits_without_losing_existing_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path(), None);
    let _ = app.update(Message::Edit(Field::Number, "42".into()));
    let _ = app.update(Message::Edit(Field::Pattern, "Friends #{n}".into()));
    let _ = app.update(Message::NewProfile);
    assert_eq!(app.profiles.selected, 1);
    assert_eq!(app.profiles.items[0].next_number, 42);
    let _ = app.update(Message::SelectProfile(0));
    assert_eq!(app.form.number, "42");
    assert_eq!(app.form.profile.lobby_pattern, "Friends #{n}");
}

#[test]
fn invalid_numeric_input_does_not_fall_back_or_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path(), None);
    let _ = app.update(Message::Edit(Field::Number, "NaN".into()));
    let _ = app.update(Message::Save);
    assert!(app.notice_error);
    assert!(!dir.path().join("profiles.json").exists());
}

#[test]
fn unknown_imported_fields_are_rejected_and_map_path_is_kept_local() {
    let profile = Profile::default();
    let mut value = serde_json::to_value(&profile).unwrap();
    value["credentials"] = serde_json::json!("do not import");
    assert!(serde_json::from_value::<Profile>(value).is_err());
    let mut invalid = profile;
    invalid.map = Some("not-a-map.exe".into());
    assert!(invalid.validate().is_err());
}

#[test]
fn existing_profiles_keep_their_counter_after_the_product_rename() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        Store::default_directory(directory.path()),
        directory.path().join("GHostReforged")
    );
    let old = directory.path().join("GHostReborn");
    let (store, mut profiles) = Store::open(&old).unwrap();
    profiles.items[0].next_number = 42;
    store.save(&profiles).unwrap();
    drop(store);
    assert_eq!(Store::default_directory(directory.path()), old);
    let app = App::new(&Store::default_directory(directory.path()), None);
    assert_eq!(app.form.number, "42");
}

#[test]
fn hosting_settings_survive_profile_selection_and_restart() {
    use super::profile::GameSpeed;
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let _ = app.update(Message::Speed(GameSpeed::Slow));
    let _ = app.update(Message::RequireFull(true));
    let _ = app.update(Message::Edit(Field::RehostDelay, "45".into()));
    let _ = app.update(Message::Edit(Field::EmptyRefresh, "600".into()));
    let _ = app.update(Message::NewProfile);
    let _ = app.update(Message::SelectProfile(0));
    assert_eq!(app.form.profile.speed, GameSpeed::Slow);
    assert!(app.form.profile.require_full);
    assert_eq!(app.form.rehost_delay, "45");
    drop(app);
    let app = App::new(directory.path(), None);
    assert_eq!(app.form.empty_refresh, "600");
    assert_eq!(app.form.profile.speed, GameSpeed::Slow);
}

#[test]
fn old_profiles_load_new_settings_without_changing_saved_values() {
    let mut value = serde_json::to_value(Profile::default()).unwrap();
    for key in [
        "speed",
        "require_full",
        "rehost_delay_seconds",
        "empty_rehost_seconds",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["next_number"] = serde_json::json!(19);
    let profile: Profile = serde_json::from_value(value).unwrap();
    assert_eq!(profile.next_number, 19);
    assert_eq!(profile.rehost_delay_seconds, 15);
    assert_eq!(profile.empty_rehost_seconds, 0);
}

#[test]
fn map_selection_generates_the_default_name_and_respects_a_custom_override() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let _ = app.update(Message::Picked(Ok(Some(
        "5473_SurvivalChaosReborn_v1.59_fix1.w3x".into(),
    ))));
    assert_eq!(app.form.profile.lobby_pattern, "Survival Chaos #{n}");
    let _ = app.update(Message::Edit(Field::Pattern, "Friends #{n}".into()));
    let _ = app.update(Message::Picked(Ok(Some("CastleFight.w3x".into()))));
    assert_eq!(app.form.profile.lobby_pattern, "Friends #{n}");
    let _ = app.update(Message::MapName);
    assert_eq!(app.form.profile.lobby_pattern, "Castle Fight #{n}");
    assert!(app.form.profile.automatic_name);
}

#[test]
fn preview_modal_requires_an_image_and_blocks_covered_controls_until_closed() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let _ = app.update(Message::OpenPreview);
    assert!(!app.preview.expanded);
    app.preview.image = Some(iced::widget::image::Handle::from_rgba(1, 1, vec![255; 4]));
    let _ = app.update(Message::OpenPreview);
    assert!(app.preview.expanded);
    assert!(app.preview.image.is_some());
    let _ = app.update(Message::Edit(Field::Pattern, "Changed #{n}".into()));
    assert_eq!(app.form.profile.lobby_pattern, "Game #{n}");
    let _ = app.update(Message::ClosePreview);
    assert!(!app.preview.expanded);
    let _ = app.update(Message::Animate(
        std::time::Instant::now() + std::time::Duration::from_secs(1),
    ));
    let _ = app.update(Message::Edit(Field::Pattern, "Changed #{n}".into()));
    assert_eq!(app.form.profile.lobby_pattern, "Changed #{n}");
}

#[test]
fn dropdown_selection_closes_the_menu_without_moving_the_hosting_page() {
    use super::{app::Page, motion::Menu};
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let _ = app.update(Message::Menu(Menu::StartRule));
    assert!(app.motion.menus[Menu::StartRule.index()].is_open());
    let _ = app.update(Message::RequireFull(false));
    assert!(!app.motion.menus[Menu::StartRule.index()].is_open());
    assert!(!app.form.profile.require_full);
    assert_eq!(app.page, Page::Hosting);
}

#[test]
fn an_inactive_window_suspends_video_requests_without_touching_lobby_settings() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let before = app.form.read().unwrap();
    let _ = app.update(Message::WindowFocus(false));
    assert!(app.backdrop.interval(app.focused).is_none());
    assert_eq!(app.form.read().unwrap(), before);
    let _ = app.update(Message::WindowFocus(true));
    assert!(app.backdrop.interval(app.focused).is_some());
}

#[test]
fn picker_completions_are_processed_even_when_the_preview_is_open() {
    for selected in [None, Some(std::path::PathBuf::from("CastleFight.w3x"))] {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path(), None);
        app.picking = true;
        app.preview.image = Some(iced::widget::image::Handle::from_rgba(1, 1, vec![255; 4]));
        let _ = app.update(Message::OpenPreview);
        assert!(app.preview.expanded);
        let _ = app.update(Message::Picked(Ok(selected.clone())));
        assert!(!app.picking);
        assert_eq!(app.form.profile.map, selected);
    }
}

#[test]
fn rejected_map_selection_preserves_the_profile_and_allows_picking_again() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), Some("Current.w3x".into()));
    let original = app.form.read().unwrap();
    app.picking = true;
    let _ = app.update(Message::Picked(Err(
        "Select a map inside Warcraft III / Maps.".into(),
    )));
    assert!(!app.picking);
    assert!(app.notice_error);
    assert_eq!(app.form.read().unwrap(), original);
}

#[test]
fn old_external_map_profiles_cannot_launch_a_hosting_worker() {
    let directory = tempfile::tempdir().unwrap();
    let map = directory.path().join("External.w3x");
    fs::write(&map, b"fixture").unwrap();
    let mut app = App::new(directory.path(), Some(map));
    assert!(app.start_hosting().is_err());
    assert!(app.worker.is_none());
}

#[test]
fn upgrading_profiles_discards_the_old_host_name_and_keeps_the_map_and_counter() {
    let dir = tempfile::tempdir().unwrap();
    let mut saved = serde_json::to_value(Profiles::default()).unwrap();
    saved["items"][0]["bot_name"] = serde_json::json!("Ghost++");
    saved["items"][0]["next_number"] = serde_json::json!(42);
    saved["items"][0]["map"] = serde_json::json!("C:/Maps/Test.w3x");
    fs::write(
        dir.path().join("profiles.json"),
        serde_json::to_vec(&saved).unwrap(),
    )
    .unwrap();
    let (store, profiles) = Store::open(dir.path()).unwrap();
    assert_eq!(profiles.items[0].next_number, 42);
    assert_eq!(
        profiles.items[0].map.as_deref(),
        Some(std::path::Path::new("C:/Maps/Test.w3x"))
    );
    store.save(&profiles).unwrap();
    let updated: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("profiles.json")).unwrap()).unwrap();
    assert!(updated["items"][0].get("bot_name").is_none());
    drop(store);
    assert_eq!(Store::open(dir.path()).unwrap().1, profiles);
}

#[test]
fn manual_and_last_game_bans_are_saved_with_reason_and_remembered() {
    use ghost_reforged::hosting::{Activity, Slot, Snapshot};
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let path = directory.path().join("bans-under-test.json");
    app.ban_file = path.clone();
    let slot = |number, name: &str, own, computer| Slot {
        id: Some(number as i64), name: name.into(), number, status: 2, computer,
        observer: false, own, change_type: true, change_team: true,
    };
    let lobby = Snapshot {
        name: "Test #3".into(), humans: 2, can_start: false, can_edit_slots: true,
        slots: vec![slot(0, "Host#1", true, false), slot(1, "Rude#5", false, false),
                    slot(2, "Computer", false, true), slot(3, "", false, false)],
    };
    assert_eq!(super::ban_list::guests(&lobby), vec!["Rude#5".to_string()]);
    app.last_roster = ("Test #3".into(), super::ban_list::guests(&lobby));
    app.record_activity(Activity::GameStarted("Test #3".into()));
    assert_eq!(app.last_game.as_ref().unwrap().players, vec!["Rude#5".to_string()]);

    let _ = app.update(Message::BanFormSubmit);
    assert!(app.notice_error, "An empty BattleTag is rejected");
    let _ = app.update(Message::BanFormTag("Typed#42".into()));
    let _ = app.update(Message::BanFormReason("spam".into()));
    let _ = app.update(Message::BanFormSubmit);
    assert!(app.ban_form_tag.is_empty());
    let _ = app.update(Message::BanLastPlayer("Rude#5".into()));
    assert!(app.is_recently_banned("rude#5"));

    let list = ghost_reforged::bans::BanList::open(Some(&path));
    assert_eq!(list.get("typed#42").unwrap().reason, "spam");
    assert!(list.is_banned("Rude#5"));
}

#[test]
fn zero_observer_slots_turn_observers_off_only_for_player_hosts() {
    use ghost_reforged::config::Observers;
    let off = super::hosting::map_settings(true, 0);
    assert!(matches!(off.observers, Observers::None));
    assert_eq!(off.observers.wire(), 0);
    assert!(matches!(super::hosting::map_settings(true, 2).observers, Observers::Full));
    // The observer-host mode sits in an observer slot itself, so it always keeps them.
    assert!(matches!(super::hosting::map_settings(false, 0).observers, Observers::Full));
}

#[test]
fn kicks_are_listed_newest_first_with_their_reason_and_logged() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    for index in 0..10 {
        app.record_kick(format!("P#{index}"), "Direct Strike filter: level 3 is below 10".into());
    }
    assert_eq!(app.recent_kicks.len(), 8);
    assert_eq!(app.recent_kicks[0].name, "P#9");
    assert!(app.recent_kicks[0].reason.contains("level 3"));
    assert!(app.logs.front().unwrap().message.starts_with("Kicked · P#9"));
}

#[test]
fn ds_minimums_are_live_settings_and_game_players_stay_listed_with_levels() {
    use ghost_reforged::{ds_report::DsStats, hosting::Activity};
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    app.form.profile.min_ds_level = 12;
    app.form.profile.min_ds_matches = 80;
    let live = app.form.profile.live_settings();
    assert_eq!((live.min_ds_level, live.min_ds_matches), (12, 80));
    assert!(live.validate().is_ok());

    app.last_roster = ("DS #4".into(), vec!["Mate#1".into()]);
    app.ds_stats.insert("Mate#1".into(), Some(DsStats { level: 20, matches: 300 }));
    app.ds_stats.insert("Gone#2".into(), None);
    app.record_activity(Activity::GameStarted("DS #4".into()));
    assert!(app.in_game);
    assert_eq!(app.last_game.as_ref().unwrap().players, vec!["Mate#1".to_string()]);
    assert!(app.ds_stats.contains_key("Mate#1"), "levels of game players are kept");
    assert!(!app.ds_stats.contains_key("Gone#2"));
}

#[test]
fn host_settings_collapse_on_toggle_and_reopen_when_hosting_stops() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    assert!(app.settings_open, "open while idle");
    let _ = app.update(Message::ToggleHostSettings);
    assert!(!app.settings_open);
    let _ = app.update(Message::ToggleHostSettings);
    assert!(app.settings_open);
    // A failed Host (no map) keeps the settings open so the error can be fixed.
    let _ = app.update(Message::Host);
    assert!(app.worker.is_none() && app.settings_open);
}

#[test]
fn a_short_match_is_marked_as_ended_early_and_leavers_get_the_early_reason() {
    use ghost_reforged::hosting::Activity;
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path(), None);
    let path = directory.path().join("bans-early.json");
    app.ban_file = path.clone();
    app.last_roster = ("DS #9".into(), vec!["Quitter#1".into(), "Stayer#2".into()]);
    app.record_activity(Activity::GameStarted("DS #9".into()));
    assert!(app.in_game);
    app.match_ended();
    assert!(!app.in_game);
    let early = app.last_game.as_ref().unwrap().ended_early;
    assert!(early.is_some_and(|s| s < 7 * 60), "an instant end is early");
    let _ = app.update(Message::BanEarlyLeaver("Quitter#1".into()));
    let list = ghost_reforged::bans::BanList::open(Some(&path));
    assert_eq!(list.get("quitter#1").unwrap().reason, "Left the game early");
    assert!(!list.is_banned("Stayer#2"));
    assert!(super::ban_list::ended_early(6 * 60 + 59));
    assert!(!super::ban_list::ended_early(7 * 60));
}

#[test]
fn settings_start_folded_only_when_a_real_map_is_ready() {
    let directory = tempfile::tempdir().unwrap();
    assert!(App::new(directory.path(), None).settings_open, "no map yet");
    let missing = App::new(directory.path().join("other").as_path(), Some("Nope.w3x".into()));
    assert!(missing.settings_open, "a missing map file needs attention");
}
