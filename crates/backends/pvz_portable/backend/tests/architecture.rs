use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("backend crate must stay below crates/backends/pvz_portable")
        .to_path_buf()
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .replace("\r\n", "\n")
}

fn bridge_source(root: &Path) -> String {
    let directory = root.join("crates/backends/pvz_portable/pvzp-rs/cpp");
    let mut files: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("cpp" | "inc")))
        .collect();
    files.sort();
    files.into_iter().map(read).collect::<Vec<_>>().join("\n")
}

#[test]
fn wrapper_dependency_points_only_downward() {
    let root = workspace_root();
    let wrapper = read(root.join("crates/backends/pvz_portable/pvzp-rs/Cargo.toml"));
    for forbidden in [
        "rsvz-pvz-portable-backend",
        "rsvz-runtime",
        "rsvz-current",
        "rsvz-pvz-portable-tooling",
        "rsvz =",
    ] {
        assert!(!wrapper.contains(forbidden), "pvzp-rs depends upward on {forbidden}");
    }
    let backend = read(root.join("crates/backends/pvz_portable/backend/Cargo.toml"));
    assert!(backend.contains("pvzp-rs.workspace = true"));
}

#[test]
fn object_pool_paths_do_not_materialize_collections() {
    let root = workspace_root();
    for relative in [
        "crates/backends/pvz_portable/pvzp-rs/src/world.rs",
        "crates/backends/pvz_portable/backend/src/handles.rs",
    ] {
        let source = read(root.join(relative));
        for forbidden in ["Vec<", "vec![", ".collect(", "copy_nonoverlapping", "memcpy"] {
            assert!(
                !source.contains(forbidden),
                "{relative} materializes pool traversal with {forbidden}"
            );
        }
    }

    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    for forbidden in [
        "CopyPlants",
        "CopyZombies",
        "GetAllPlants",
        "GetAllZombies",
        "SnapshotBuffer",
    ] {
        assert!(
            !bridge.contains(forbidden),
            "bridge contains bulk object path {forbidden}"
        );
    }
    for line in bridge.lines().filter(|line| line.contains("memcpy")) {
        assert!(
            line.contains("sizeof(float)")
                && [
                    "row_pick_weight_bits",
                    "row_pick_last_picked_bits",
                    "row_pick_second_last_picked_bits"
                ]
                .iter()
                .any(|name| line.contains(&format!("PVZP_RS_DEFINE_WORLD_ROW_VALUE({name},"))),
            "unexpected bridge bulk copy: {line}"
        );
    }
}

#[test]
fn portable_dispatch_precedes_each_native_logic_update() {
    let root = workspace_root();
    let source = read(root.join("../PvZ-Portable/src/LawnApp.cpp"));
    let start = source.find("void LawnApp::UpdateFrames()").expect("UpdateFrames");
    let body = &source[start
        ..source[start..]
            .find("\n}\n")
            .map_or(source.len(), |end| start + end + 3)];
    let dispatch = body.find("PvzpNative::BeforeUpdate()").expect("Portable dispatch");
    let counter = body.find("mAppCounter++").expect("native app counter");
    let begin = body.find("PvzpNative::BeginLogicFrame()").expect("event begin");
    let update = body.find("SexyApp::UpdateFrames()").expect("native update");
    let end = body.find("PvzpNative::EndLogicFrame()").expect("event end");
    assert!(dispatch < counter && counter < begin && begin < update && update < end);
}

#[test]
fn portable_readiness_handles_existing_endless_save_dialog() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    assert!(bridge.contains("KillDialog(Dialogs::DIALOG_CONTINUE)"));

    let readiness = read(root.join("crates/backends/pvz_portable/backend/src/host/readiness.rs"));
    assert!(readiness.contains("CONTINUE_DIALOG_COUNTDOWN"));
    assert!(readiness.contains("click_continue_dialog_if_present"));
}

#[test]
fn portable_seed_chooser_readiness_is_classified_in_shared_game_logic() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    let header = read(root.join("crates/backends/pvz_portable/pvzp-rs/cpp/bridge.h"));
    assert!(!bridge.contains("SeedChooserOpeningReady"));
    assert!(!bridge.contains("SeedChooserCardActionsReady"));
    for scalar in [
        "pvzp_rs_world_seed_choosing",
        "pvzp_rs_seed_chooser_mouse_visible",
        "pvzp_rs_seed_chooser_choose_state",
        "pvzp_rs_seed_chooser_view_lawn_time",
        "pvzp_rs_seed_chooser_seeds_in_flight",
    ] {
        assert!(header.contains(scalar), "missing raw chooser scalar {scalar}");
    }

    let backend = read(root.join("crates/backends/pvz_portable/backend/src/runtime.rs"));
    assert!(backend.contains("classify_seed_chooser_readiness("));
    let host = read(root.join("crates/backends/pvz_portable/backend/src/host/readiness.rs"));
    assert!(host.contains("readiness.opening_action()"));
}

#[test]
fn portable_game_speed_uses_native_timing_and_restores_at_boundaries() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    assert!(bridge.contains("gOriginalFrameTime = gLawnApp->mFrameTime"));
    assert!(bridge.contains("gOriginalUpdateMultiplier = gLawnApp->mUpdateMultiplier"));
    assert!(bridge.contains("gLawnApp->mFrameTime = static_cast<int>(10.0f / speed + 0.5f)"));
    assert!(bridge.contains("RestoreGameSpeed();"));

    let readiness = read(root.join("crates/backends/pvz_portable/backend/src/host/readiness.rs"));
    assert!(readiness.contains("if returned_to_menu(previous, ui)"));
    assert!(readiness.contains("pvzp_rs::restore_game_speed()?"));
}

#[test]
fn portable_fast_forward_options_reach_native_execution_points() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    assert!(bridge.contains("gFastForwardPerformance = enabled ? performance : 0"));
    assert!(bridge.contains("gFastForwardSuppressWindow = enabled && suppressWindow != 0"));
    assert!(bridge.contains("return INT_MAX;"));
    assert!(bridge.contains("gSeedChooserFastForwardRemaining, static_cast<std::uint32_t>(INT_MAX)"));
    assert!(!bridge.contains("std::min<std::uint32_t>(20, gSeedChooserFastForwardRemaining)"));
    assert_eq!(bridge.matches("gAcceleratedUpdateBatch = false;").count(), 2);
    assert!(!bridge.contains("if (gFastForward || gAdvancedPause)"));
    assert!(bridge.contains("gFastForwardSuppressWindow = false;\n\t\tgSeedChooserFastForwardRemaining = maxFrames"));
    assert!(bridge.contains("bool ContinueUpdateBatch(int updateIndex)"));
    assert!(!bridge.contains("(void)suppressWindow"));

    let before_update = &bridge[bridge.find("bool BeforeUpdate()").expect("BeforeUpdate")
        ..bridge.find("void DrawAdvancedPauseMask").expect("end of BeforeUpdate")];
    assert!(before_update.contains("gLawnApp->mGameScene != GameScenes::SCENE_PLAYING"));
    assert!(before_update.contains("gLawnApp->mBoard->mPaused"));
    assert!(before_update.contains("gLawnApp->mWidgetManager->mBaseModalWidget"));
    assert!(before_update.contains("gFastForwardSuppressWindow = false;"));

    let window_suppression = &bridge[bridge
        .find("bool WindowUpdateSuppressed()")
        .expect("window suppression")
        ..bridge.find("void NoteBattleSeed").expect("end of window suppression")];
    assert!(window_suppression.contains("gFastForward && gFastForwardSuppressWindow"));
    assert!(window_suppression.contains("gSeedChooserFastForwardRemaining && gLawnApp"));
    assert!(window_suppression.contains("GameScenes::SCENE_LEVEL_INTRO"));

    let lawn_app = read(root.join("../PvZ-Portable/src/LawnApp.cpp"));
    assert!(lawn_app.contains("PvzpNative::ContinueUpdateBatch(i)"));
    assert!(lawn_app.contains("if (!PvzpNative::BeforeUpdate())\n\t\t\tbreak;"));
    assert!(lawn_app.contains("PvzpNative::FastForwardPerformance()"));

    let app_base = read(root.join("../PvZ-Portable/src/SexyAppFramework/SexyAppBase.cpp"));
    assert!(app_base.contains(
        "if (PvzpNative::WindowUpdateSuppressed())\n\t{\n\t\tmHasPendingDraw = false;\n\t\tmLastDrawWasEmpty = true;"
    ));

    let aggressive_guard = "if (PvzpNative::FastForwardPerformance() >= 2)";
    let plant = read(root.join("../PvZ-Portable/src/Lawn/Plant.cpp"));
    assert!(plant.matches(aggressive_guard).count() >= 3);
    assert!(plant.contains("return 0.0f;"));
    assert!(read(root.join("../PvZ-Portable/src/Lawn/Board.cpp")).contains(aggressive_guard));
    assert!(read(root.join("../PvZ-Portable/src/Lawn/CursorObject.cpp")).contains(aggressive_guard));
    assert!(app_base.contains(aggressive_guard));

    let reanimator = read(root.join("../PvZ-Portable/src/PvzpLib/Reanimator.cpp"));
    assert!(reanimator.contains("PvzpNative::FastForwardPerformance() < 2"));
    assert!(reanimator.contains("aUpdateAttacherTracks && strncasecmp"));

    let particles = read(root.join("../PvZ-Portable/src/PvzpLib/PvzpParticle.cpp"));
    assert!(particles.contains(aggressive_guard));
    assert!(particles.contains("DeleteNonCrossFading();\n\t\tmDead = true;\n\t\treturn;"));
}

#[test]
fn portable_stop_control_uses_a_named_event_without_file_polling() {
    let root = workspace_root();
    let host = read(root.join("crates/backends/pvz_portable/backend/src/host/mod.rs"));
    assert!(host.contains("OpenEventW"));
    assert!(host.contains("WaitForSingleObject"));
    assert!(!host.contains("stop_request_path"));
    assert!(
        !host
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap()
            .contains(".exists()")
    );

    let tooling = read(root.join("crates/backends/pvz_portable/tooling/src/run_cli.rs"));
    assert!(tooling.contains("CreateEventW"));
    assert!(tooling.contains("SetEvent"));
    assert!(!tooling.contains("stop_request_path"));
    assert!(!tooling.contains(".stop\""));
}

#[test]
fn portable_witness_random_scopes_match_locked_setup_boundaries() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    assert!(bridge.contains("RandomConstructionGuard randomConstruction(seed)"));
    assert!(bridge.contains("BeginRandomConstruction(gRandomStreams[0].seed, true)"));
    assert!(bridge.contains("gWaveSpawnRandomSeed + static_cast<std::uint32_t>(board->mCurrentWave)"));
    assert!(bridge.contains("gWaveSpawnActive && gWaveRowPickActive && Sexy::IsBattleRandom(random)"));

    let board = read(root.join("../PvZ-Portable/src/Lawn/Board.cpp"));
    assert!(board.contains("PvzpNative::BeginSurvivalWaveInit()"));
    assert!(board.contains("PvzpNative::BeginWaveSpawn(this)"));
    assert!(board.contains("PvzpNative::BeginRowPick()"));

    let backend = read(root.join("crates/backends/pvz_portable/backend/src/impls.rs"));
    assert!(backend.contains("pvzp_rs::set_wave_spawn_random_seed(base_seed)"));
    assert!(!backend.contains("Unsupported(\"isolated wave-spawn randomness\")"));
}

#[test]
fn portable_completed_rounds_are_one_shot_session_deltas() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    assert!(bridge.contains("const std::uint64_t completedRounds = gPendingCompletedRounds;"));
    assert!(bridge.contains("gPendingCompletedRounds = 0;"));
    assert!(!bridge.contains("std::uint64_t gCompletedRounds ="));
    assert!(!bridge.contains("gPendingCompletedRounds = completedRounds"));

    let lawn_app = read(root.join("../PvZ-Portable/src/LawnApp.cpp"));
    assert!(lawn_app.contains("PvzpNative::RoundCompleted()"));
}

#[test]
fn portable_reset_normalizes_the_first_playing_origin() {
    let root = workspace_root();
    let bridge = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp")) + &bridge_source(&root);
    let normalize = &bridge[bridge.find("if (gPendingWorldOrigin").expect("pending origin")
        ..bridge
            .find("const std::uint64_t completedRounds")
            .expect("dispatch follows origin normalization")];
    assert!(normalize.contains("GameScenes::SCENE_PLAYING"));
    assert!(normalize.contains("gLawnApp->mBoard->mMainCounter = 0"));
    assert!(normalize.contains("gLawnApp->mAppCounter = static_cast<std::int32_t>(gPendingDancerClock)"));
}

#[test]
fn modifier_queries_do_not_call_back_into_rust() {
    let root = workspace_root();
    let source = read(root.join("../PvZ-Portable/src/PvzpLib/NativeControls.cpp"));
    let start = source
        .find("#define PVZP_DEFINE_RULE_QUERY")
        .expect("modifier query block");
    let end = source[start..]
        .find("int UpdateCount")
        .expect("end of modifier query block")
        + start;
    assert!(!source[start..end].contains("rsvz_pvzp_"));
}

#[test]
fn portable_scripts_are_runtime_plugins_not_game_link_inputs() {
    let root = workspace_root();
    let tooling = read(root.join("crates/backends/pvz_portable/tooling/src/lib.rs"));
    assert!(tooling.contains("crate-type = [\\\"cdylib\\\"]"));
    assert!(!tooling.contains("staticlib"));

    let run_cli = read(root.join("crates/backends/pvz_portable/tooling/src/run_cli.rs"));
    assert!(run_cli.contains("target/rsvz/pvz-portable/game"));
    assert!(run_cli.contains("PVZP_PLUGIN"));
    assert!(!run_cli.contains("RSVZ_PORTABLE_STATICLIB"));
    assert!(!run_cli.contains("--fresh"));

    let cmake = read(root.join("../PvZ-Portable/CMakeLists.txt"));
    assert!(!cmake.contains("RSVZ"));
    let loader = read(root.join("../PvZ-Portable/src/PvzpLib/Plugin.cpp"));
    assert!(loader.contains("LoadLibraryW"));
    assert!(loader.contains("FreeLibrary"));
    assert!(loader.contains("DLL retained"));
}
