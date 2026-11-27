use super::*;

fn online() -> Value {
    event(
        "GameList",
        json!({"games":[{"id":42,"region":"EU","currentPlayers":1,"maxPlayers":4}]}),
    )
}

#[test]
fn sign_in_can_complete_after_a_long_wait_without_repeated_initialization() {
    let mut engine = Engine::new(config());
    assert_eq!(
        engine.on_event(&event("GameList", json!({"games":[]})), 0)[0].message,
        "InitializeNetProvider"
    );
    assert!(engine.tick(31).is_empty());
    assert_eq!(engine.phase, Phase::AwaitSignIn);
    for time in [60, 120, 600] {
        assert!(engine.tick(time).is_empty());
        assert!(
            engine
                .on_event(&event("GameList", json!({"games":[]})), time)
                .is_empty()
        );
    }
    assert_eq!(engine.on_event(&online(), 601)[0].message, "CreateLobby");
    assert!(engine.on_event(&online(), 602).is_empty());
}

#[test]
fn disconnect_revokes_lobby_controls_and_consumes_its_number_once() {
    let mut engine = armed();
    assert!(
        engine
            .on_event(&event("LoggedOut", json!({})), 1)
            .is_empty()
    );
    assert_eq!(engine.phase, Phase::AwaitSignIn);
    assert_eq!(engine.number(), 2);
    assert!(engine.owned_lobby(1).is_none());
    assert!(engine.start_now(1).is_empty());
    assert!(engine.slot_command(0, false, 1).is_none());
    engine.on_event(&event("LoggedOut", json!({})), 2);
    assert_eq!(engine.number(), 2);
    assert!(
        engine.on_event(&online(), 3).is_empty(),
        "A stale listing cannot resume a disconnected lobby"
    );
    engine.on_event(&event("SetGlueScreen", json!({"screen":"MAIN_MENU"})), 4);
    let commands = engine.on_event(&online(), 5);
    assert_eq!(commands[0].payload["gameName"], "GHost Rust #2");
}

#[test]
fn normal_native_countdown_completion_is_not_misread_as_cancellation() {
    let mut engine = armed();
    engine.config.auto_start = false;
    engine.on_event(
        &lobby(vec![
            player(0, true, false, false),
            player(1, false, false, true),
        ]),
        1,
    );
    engine.start_now(1);
    for (time, active, locked) in [(2, true, false), (5, true, true), (8, false, false)] {
        assert!(
            engine
                .on_event(
                    &event(
                        "LobbyCountdown",
                        json!({"details":{"countdownActive":active,"countdownLocked":locked}})
                    ),
                    time
                )
                .is_empty()
        );
    }
    assert!(matches!(engine.phase, Phase::Starting(_)));
    engine.on_event(
        &event("SetGlueScreen", json!({"screen":"LOADING_SCREEN"})),
        8,
    );
    assert_eq!(engine.phase, Phase::InGame);
}

#[test]
fn a_countdown_cancelled_before_lock_returns_to_observation() {
    let mut engine = armed();
    engine.start_now(1);
    engine.on_event(
        &event(
            "LobbyCountdown",
            json!({"details":{"countdownActive":false,"countdownLocked":false}}),
        ),
        2,
    );
    assert_eq!(engine.phase, Phase::Lobby);
    assert!(engine.tick(3).is_empty());
}

#[test]
fn observer_leaves_at_countdown_completion_without_waiting_for_loading() {
    let mut engine = armed();
    engine.config.auto_start = false;
    engine.config.host_mode = HostMode::Observer;
    engine.config.leave_on_load = true;
    engine.on_event(
        &lobby(vec![
            player(0, true, true, false),
            player(1, false, false, true),
        ]),
        1,
    );
    engine.start_now(1);
    engine.on_event(
        &event(
            "LobbyCountdown",
            json!({"details":{
                "countdownActive":true,"countdownLocked":true
            }}),
        ),
        4,
    );
    let done = event(
        "LobbyCountdown",
        json!({"details":{
            "countdownActive":false,"countdownLocked":false
        }}),
    );
    assert_eq!(engine.on_event(&done, 7)[0].message, "LeaveGame");
    assert_eq!(engine.phase, Phase::Releasing(7));
    assert!(engine.on_event(&done, 7).is_empty());
    assert!(
        engine
            .on_event(
                &event("SetGlueScreen", json!({"screen":"LOADING_SCREEN"})),
                8
            )
            .is_empty()
    );
    let commands = engine.on_event(
        &event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})),
        10,
    );
    assert_eq!(
        commands.iter().map(|c| c.message).collect::<Vec<_>>(),
        ["ScoreScreenClose", "InitializeNetProvider"]
    );
    assert_eq!(engine.number(), 2);
}

#[test]
fn process_restart_requires_confirmed_loading_and_acknowledged_process_exit() {
    let mut engine = armed();
    engine.config.auto_start = false;
    engine.config.host_mode = HostMode::Observer;
    engine.config.leave_on_load = true;
    engine.config.restart_on_load = true;
    engine.on_event(
        &lobby(vec![
            player(0, true, true, false),
            player(1, false, false, true),
        ]),
        1,
    );
    engine.start_now(1);
    for (time, active, locked) in [(2, true, false), (4, true, true), (7, false, false)] {
        assert!(
            engine
                .on_event(
                    &event(
                        "LobbyCountdown",
                        json!({"details":{
                            "countdownActive":active,"countdownLocked":locked
                        }})
                    ),
                    time
                )
                .is_empty()
        );
        assert!(matches!(engine.phase, Phase::Starting(_)));
    }
    let loading = event("SetGlueScreen", json!({"screen":"LOADING_SCREEN"}));
    assert!(engine.on_event(&loading, 8).is_empty());
    assert_eq!(engine.phase, Phase::Restarting);
    assert_eq!(engine.number(), 1);
    assert!(engine.on_event(&loading, 8).is_empty());
    assert!(engine.on_event(&online(), 8).is_empty());
    engine.client_stopped(8);
    assert_eq!(engine.phase, Phase::Cooldown(23));
    assert_eq!(engine.number(), 2);
    engine.client_stopped(9);
    assert_eq!(engine.number(), 2);
    assert!(engine.on_event(&online(), 22).is_empty());
    engine.tick(23);
    assert_eq!(
        engine.on_event(&online(), 24)[0].payload["gameName"],
        "GHost Rust #2"
    );
}

#[test]
fn restart_never_closes_a_client_whose_host_is_still_a_player() {
    let mut engine = armed();
    engine.config.auto_start = false;
    engine.config.host_mode = HostMode::Observer;
    engine.config.leave_on_load = true;
    engine.config.restart_on_load = true;
    engine.on_event(&lobby(vec![player(0, true, false, false)]), 1);
    engine.on_event(
        &event(
            "LobbyCountdown",
            json!({"details":{
                "countdownActive":true,"countdownLocked":true
            }}),
        ),
        2,
    );
    engine.on_event(
        &event("SetGlueScreen", json!({"screen":"LOADING_SCREEN"})),
        3,
    );
    assert_eq!(engine.phase, Phase::InGame);
}

#[test]
fn a_fresh_client_waits_for_initialization_not_just_provider_selection() {
    let mut engine = Engine::new(config());
    engine.expect_client_initialization();
    assert_eq!(
        engine.on_event(&online(), 0)[0].message,
        "InitializeNetProvider"
    );
    assert!(
        engine
            .on_event(
                &event("OnNetProviderChanged", json!({"providerId":"BNET"})),
                1
            )
            .is_empty()
    );
    engine.on_event(&event("SetGlueScreen", json!({"screen":"MAIN_MENU"})), 2);
    assert!(engine.on_event(&online(), 2).is_empty());
    assert_eq!(engine.phase, Phase::Initializing(0));
    let initialized = event(
        "OnNetProviderInitialized",
        json!({"message":{"isOnline":true}}),
    );
    assert_eq!(engine.on_event(&initialized, 3)[0].message, "CreateLobby");
    assert!(engine.on_event(&initialized, 4).is_empty());
}

#[test]
fn a_player_host_sees_the_results_then_returns_and_hosts_the_next_lobby() {
    let mut engine = armed();
    assert_eq!(engine.config.host_mode, HostMode::Player);
    engine.config.rehost_delay_seconds = 15;
    engine.on_event(&event("SetGlueScreen", json!({"screen":"LOADING_SCREEN"})), 5);
    // `armed` never starts a game; force the confirmed in-game state.
    engine.phase = Phase::InGame;
    assert!(
        engine
            .on_event(&event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})), 100)
            .is_empty(),
        "the results stay open"
    );
    assert_eq!(engine.phase, Phase::Cooldown(115));
    assert_eq!(engine.number(), 2);
    assert!(engine.take_notice().unwrap().contains("next lobby in 15 s"));
    assert!(engine.tick(114).is_empty());
    let commands = engine.tick(115);
    assert_eq!(
        commands.iter().map(|c| c.message).collect::<Vec<_>>(),
        ["ScoreScreenClose", "GetGameList"]
    );
    assert_eq!(engine.phase, Phase::AwaitMenu);
    assert!(engine.tick(116).is_empty(), "closed once");
    // The signed-in session is reused: no menu callback is required.
    let next = engine.on_event(&event("GameList", json!({})), 117);
    assert_eq!(next[0].message, "InitializeNetProvider");
    let created = engine.on_event(
        &event("OnNetProviderChanged", json!({"providerId":"BNET"})),
        120,
    );
    assert_eq!(created[0].message, "CreateLobby");
}

#[test]
fn a_player_host_who_clicks_back_early_is_not_sent_a_stale_close() {
    let mut engine = armed();
    engine.phase = Phase::InGame;
    engine.on_event(&event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})), 100);
    engine.on_event(
        &event("SetGlueScreen", json!({"screen":"MULTIPLAYER_CUSTOM_GAMES_SETUP"})),
        102,
    );
    let until = match engine.phase {
        Phase::Cooldown(until) => until,
        ref other => panic!("expected cooldown, got {other:?}"),
    };
    assert!(
        !engine
            .tick(until)
            .iter()
            .any(|c| c.message == "ScoreScreenClose")
    );
}

#[test]
fn a_player_host_without_rehost_keeps_the_results_and_stops() {
    let mut engine = armed();
    engine.config.max_games = 1;
    engine.phase = Phase::InGame;
    assert!(
        engine
            .on_event(&event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})), 100)
            .is_empty()
    );
    assert_eq!(
        engine.phase,
        Phase::Stopped("Configured game limit reached".into())
    );
}

#[test]
fn a_player_match_that_ended_unnoticed_is_still_closed_and_rehosted() {
    // The game reached the results without the bot seeing it start (no countdown or
    // loading callbacks), e.g. after a dropped connection. The lobby is still ours.
    let mut engine = armed();
    assert_eq!(engine.phase, Phase::Lobby);
    engine.on_event(&event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})), 50);
    assert!(matches!(engine.phase, Phase::Cooldown(_)));
    assert_eq!(engine.number(), 2);
    let Phase::Cooldown(until) = engine.phase else { unreachable!() };
    assert_eq!(engine.tick(until)[0].message, "ScoreScreenClose");
}

#[test]
fn results_seen_during_cooldown_are_closed_when_it_ends() {
    let mut engine = armed();
    engine.phase = Phase::InGame;
    engine.on_event(&event("GameClosed", json!({})), 10);
    // A late results screen after the game was already counted.
    assert!(engine.on_event(&event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})), 12).is_empty());
    assert_eq!(engine.number(), 2);
    let Phase::Cooldown(until) = engine.phase else { panic!("cooldown") };
    assert_eq!(engine.tick(until)[0].message, "ScoreScreenClose");
}

#[test]
fn the_end_of_a_player_match_is_reported_once() {
    let mut engine = armed();
    engine.phase = Phase::InGame;
    assert!(!engine.take_match_ended());
    engine.on_event(&event("GameClosed", json!({})), 10);
    assert!(engine.take_match_ended());
    engine.on_event(&event("SetGlueScreen", json!({"screen":"SCORE_SCREEN"})), 11);
    assert!(!engine.take_match_ended(), "the late results screen is not a second match");
}

#[test]
fn the_finished_matchs_roster_after_quit_does_not_stop_a_player_host() {
    let mut engine = armed();
    engine.phase = Phase::InGame;
    engine.on_event(&event("GameClosed", json!({})), 10);
    assert!(matches!(engine.phase, Phase::Cooldown(_)));
    // Warcraft III reports the old lobby again after Quit; it must not look foreign.
    engine.on_event(&lobby(vec![player(0, true, false, false)]), 11);
    assert!(matches!(engine.phase, Phase::Cooldown(_)), "{:?}", engine.phase);
    assert_eq!(engine.config.finished_lobby.as_deref(), Some("GHost Rust #1"));
    // A genuinely different lobby still stops hosting.
    let mut other = lobby(vec![player(0, true, false, false)]);
    other["payload"]["lobbyName"] = json!("Someone else #9");
    engine.on_event(&other, 12);
    assert!(matches!(engine.phase, Phase::Stopped(_)));
}

#[test]
fn live_rehost_settings_never_turn_on_observer_only_leaving_for_a_player_host() {
    let mut engine = armed();
    assert_eq!(engine.config.host_mode, HostMode::Player);
    engine.update_rehost(true, 15, 1);
    assert!(!engine.config.leave_on_load && !engine.config.restart_on_load);
    assert_eq!(engine.config.max_games, 0, "rehost on never stops");
    engine.config.host_mode = HostMode::Observer;
    engine.update_rehost(true, 15, 2);
    assert!(engine.config.leave_on_load && engine.config.restart_on_load);
}

#[test]
fn a_player_host_rehosts_from_an_online_listing_without_reinitializing() {
    let mut engine = armed();
    engine.phase = Phase::InGame;
    engine.on_event(&event("GameClosed", json!({})), 10);
    let Phase::Cooldown(until) = engine.phase else { panic!("cooldown") };
    engine.tick(until);
    let created = engine.on_event(&online(), until + 1);
    assert_eq!(created[0].message, "CreateLobby", "an online listing reuses the session");
    // A reconnect while waiting does not demand a fresh initialization either.
    let mut engine = armed();
    engine.phase = Phase::AwaitMenu;
    engine.recover(5);
    assert_eq!(engine.on_event(&online(), 6)[0].message, "CreateLobby");
}

#[test]
fn leaving_a_player_match_straight_to_a_menu_counts_it_and_rehosts() {
    let mut engine = armed();
    engine.phase = Phase::InGame;
    let menu = event("SetGlueScreen", json!({"screen":"MULTIPLAYER_CUSTOM_GAMES_SETUP"}));
    assert!(engine.on_event(&menu, 100).is_empty());
    assert!(engine.take_match_ended(), "the replay is read for this match");
    assert_eq!(engine.number(), 2);
    let Phase::Cooldown(until) = engine.phase else { panic!("cooldown") };
    assert!(
        !engine.tick(until).iter().any(|c| c.message == "ScoreScreenClose"),
        "no results screen to close"
    );
    assert_eq!(engine.on_event(&online(), until + 1)[0].message, "CreateLobby");
}
