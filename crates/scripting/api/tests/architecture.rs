use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repository root should resolve")
}

fn read(relative: &str) -> String {
    fs::read_to_string(root().join(relative)).unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

fn normal_dependencies(manifest: &str) -> &str {
    manifest_table(manifest, "dependencies")
}

fn manifest_table<'a>(manifest: &'a str, name: &str) -> &'a str {
    let header = format!("[{name}]");
    let start = manifest
        .find(&header)
        .unwrap_or_else(|| panic!("manifest should have a {header} table"));
    let rest = &manifest[start + header.len()..];
    let end = rest.find("\n[").unwrap_or(rest.len());
    &rest[..end]
}

fn concrete_backend_features(manifest: &str) -> Vec<(&str, &str)> {
    manifest_table(manifest, "features")
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .filter(|(name, _)| name.starts_with("pvz-"))
        .collect()
}

fn feature_names<'a>(features: &[(&'a str, &str)]) -> Vec<&'a str> {
    let mut names = features.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn collect_rs_files(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())) {
        let path = entry.expect("source directory entry should be readable").path();
        if path.is_dir() {
            collect_rs_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}

fn assert_complete_sources_exclude(directories: &[&str], forbidden: &[&str]) {
    let mut sources = Vec::new();
    for directory in directories {
        collect_rs_files(&root().join(directory), &mut sources);
    }
    for path in sources {
        let source = fs::read_to_string(&path).expect("Rust source should be readable");
        for value in forbidden {
            assert!(
                !source.contains(value),
                "source {} must not contain `{value}`",
                path.display()
            );
        }
    }
}

fn assert_in_order(source: &str, labels: &[&str]) {
    let mut cursor = 0;
    for label in labels {
        let offset = source[cursor..]
            .find(label)
            .unwrap_or_else(|| panic!("missing ordered source marker `{label}`"));
        cursor += offset + label.len();
    }
}

#[test]
fn witness_is_an_external_after_tick_extension() {
    assert!(!read("crates/scripting/api/src/lib.rs").contains("pub mod witness;"));
    assert!(!read("crates/core/game/src/lib.rs").contains("pub mod witness;"));

    let source = read("extensions/rsvz-witness/src/capture.rs");
    assert!(!source.contains("spawn_finalizer"));
    assert!(!source.contains("WitnessRandomMode"));
    assert!(!source.contains("set_zombie_spawn_stopped"));
    assert!(source.contains("RandomMode::Locked(locked_random)"));
    assert!(source.contains("set_wave_spawn_random_seed(wave_spawn_seed)"));
    assert!(source.contains("register_fallible::<_>(StateEvent::AfterTick, 0"));
    let core = read("extensions/rsvz-witness/src/core.rs");
    assert!(core.contains("pub locked_random: u32"));
    assert!(core.contains("pub wave_spawn_random: bool"));
    assert!(!core.contains("pub random: RandomMode"));
    let install = &source[source.find("fn install(").expect("Witness install")
        ..source.find("fn initialize(").expect("Witness initialize")];
    assert_in_order(
        install,
        &[
            "rsvz::setup::with_script_setup(|setup| setup.reload_mode = ReloadMode::MainUiOrFightUi);",
            "rsvz::claim_session_job",
        ],
    );

    let manifest = read("extensions/rsvz-witness/Cargo.toml");
    assert!(manifest.contains("capture = []"));
    assert!(!source.contains("R::"));
    assert!(source.contains("rsvz::event::reserve(EVENT_CAPACITY)"));
    assert!(source.contains("rsvz::event::on("));
    assert!(manifest.contains("rsvz = { path = \"../../crates/scripting/api\", default-features = false }"));
}

#[test]
fn native_seed_chooser_scalars_share_one_backend_contract_classifier() {
    let setup = read("crates/core/backend-api/src/opening.rs");
    assert!(setup.contains("pub const fn classify_seed_chooser_readiness("));
    assert!(setup.contains("pub const fn opening_action(self) -> SeedChooserOpeningAction"));

    for backend in [
        "crates/backends/pvz_1_0_0_1051/injected/src/runtime/seed_chooser.rs",
        "crates/backends/pvz_portable/backend/src/runtime.rs",
    ] {
        let source = read(backend);
        assert!(source.contains("classify_seed_chooser_readiness("));
        assert!(!source.contains("fn classify_seed_chooser_readiness("));
    }
    for host in [
        "crates/backends/pvz_1_0_0_1051/injected/src/host/readiness.rs",
        "crates/backends/pvz_portable/backend/src/host/readiness.rs",
    ] {
        assert!(read(host).contains("readiness.opening_action()"));
    }
    let pe = read("crates/backends/pvz_emulator/backend/src/dispatch.rs");
    let opening_ready = &pe[pe.find("pub const fn opening_ready").expect("PE opening readiness")..];
    assert!(opening_ready.starts_with("pub const fn opening_ready(self) -> bool {\n        true"));
}

#[test]
fn fixed_current_dependency_chain_and_boundaries_are_frozen() {
    let rsvz = read("crates/scripting/api/Cargo.toml");
    let rsvz_dependencies = normal_dependencies(&rsvz);
    assert!(!rsvz_dependencies.contains("rsvz-runtime"));
    assert!(!root().join("crates/core/runtime").exists());
    assert!(!read("Cargo.toml").contains("rsvz-runtime"));
    assert!(rsvz_dependencies.contains("rsvz-current"));
    assert!(!rsvz_dependencies.contains("rsvz-pvz"));

    for manifest in [
        "crates/core/model/Cargo.toml",
        "crates/core/backend-api/Cargo.toml",
        "crates/core/profiling/Cargo.toml",
    ] {
        let source = read(manifest);
        let dependencies = normal_dependencies(&source);
        for forbidden in ["rsvz-current", "rsvz-runtime", "rsvz-pvz", "rsvz_backend_1051"] {
            assert!(
                !dependencies.contains(forbidden),
                "{manifest} must not have normal dependency `{forbidden}`"
            );
        }
    }

    for manifest in ["crates/core/game/Cargo.toml", "crates/core/schedule/Cargo.toml"] {
        let source = read(manifest);
        let dependencies = normal_dependencies(&source);
        assert!(dependencies.contains("rsvz-current.workspace = true"));
        for forbidden in ["rsvz-runtime", "rsvz-pvz", "rsvz_backend_1051"] {
            assert!(!dependencies.contains(forbidden), "{manifest}: {forbidden}");
        }
        assert!(concrete_backend_features(&source).is_empty());
    }

    for manifest in [
        "crates/backends/pvz_1_0_0_1051/injected/Cargo.toml",
        "crates/backends/pvz_emulator/backend/Cargo.toml",
        "crates/backends/pvz_portable/backend/Cargo.toml",
    ] {
        let source = read(manifest);
        for forbidden in ["rsvz-game", "rsvz-schedule"] {
            assert!(
                !source.contains(forbidden),
                "{manifest} must not depend on {forbidden}, including in tests"
            );
        }
        let dependencies = normal_dependencies(&source);
        for forbidden in [
            "rsvz-game",
            "rsvz-schedule",
            "rsvz-runtime",
            "rsvz-current",
            "\nrsvz =",
            "tooling",
        ] {
            assert!(
                !dependencies.contains(forbidden),
                "{manifest} must not have normal dependency `{forbidden}`"
            );
        }
    }

    assert_complete_sources_exclude(
        &[
            "crates/core/model/src",
            "crates/core/backend-api/src",
            "crates/core/schedule/src",
            "crates/core/game/src",
            "crates/core/profiling/src",
        ],
        &[
            "rsvz_runtime",
            "rsvz_pvz_emulator_backend",
            "rsvz_pvz_portable_backend",
            "rsvz_backend_1051",
            "Pvz1051Backend",
            "PeBackend",
            "PortableBackend",
        ],
    );
    assert_complete_sources_exclude(
        &[
            "crates/core/model/src",
            "crates/core/backend-api/src",
            "crates/core/profiling/src",
        ],
        &["rsvz_current"],
    );
    assert_complete_sources_exclude(
        &["crates/scripting/api/src"],
        &[
            "rsvz_pvz_emulator_backend",
            "rsvz_pvz_portable_backend",
            "rsvz_backend_1051",
            "Pvz1051Backend",
            "PeBackend",
            "PortableBackend",
        ],
    );
    assert_complete_sources_exclude(
        &[
            "crates/backends/pvz_1_0_0_1051/injected/src",
            "crates/backends/pvz_emulator/backend/src",
            "crates/backends/pvz_portable/backend/src",
        ],
        &[
            "rsvz_game",
            "rsvz_schedule",
            "rsvz_runtime",
            "rsvz_current",
            "RuntimeState<",
            "TickScheduler",
            "StateHookRegistry",
        ],
    );
    assert_complete_sources_exclude(
        &[
            "crates/backends/pvz_1_0_0_1051/injected/src",
            "crates/backends/pvz_emulator/backend/src",
            "crates/backends/pvz_portable/backend/src",
        ],
        &["rsvz_game", "rsvz_schedule"],
    );
}

#[test]
fn concrete_backend_features_are_pure_selectors() {
    let rsvz = read("crates/scripting/api/Cargo.toml");
    let current = read("crates/core/current/Cargo.toml");
    let rsvz_features = concrete_backend_features(&rsvz);
    let current_features = concrete_backend_features(&current);

    assert!(
        !rsvz_features.is_empty(),
        "at least one concrete backend feature should exist"
    );
    assert_eq!(feature_names(&rsvz_features), feature_names(&current_features));

    for (name, value) in rsvz_features {
        assert!(
            value.contains(&format!("\"rsvz-current/{name}\"")),
            "rsvz[{name}] must forward to the same rsvz-current feature"
        );
        assert_eq!(
            value.matches("rsvz-current/pvz-").count(),
            1,
            "rsvz[{name}] must select exactly one current backend"
        );
    }
    for manifest in [
        &rsvz,
        &current,
        &read("crates/core/game/Cargo.toml"),
        &read("crates/core/schedule/Cargo.toml"),
    ] {
        for removed in ["current-runtime", "item-collection", "plant-visual-state"] {
            assert!(!manifest_table(manifest, "features").contains(removed));
        }
    }
    for (name, value) in current_features {
        assert!(
            value.contains("dep:"),
            "rsvz-current[{name}] must select an optional concrete backend dependency"
        );
    }

    assert_complete_sources_exclude(&["crates/scripting/api/src"], &["rsvz_pvz_", "rsvz_backend_1051"]);
}

#[test]
fn functional_modules_own_state_and_current_only_selects_types() {
    assert_complete_sources_exclude(
        &["crates/scripting/api/src/dsl"],
        &[
            "RetentionState",
            "Cell<Option<PlantId>>",
            "plant_recorded",
            "normalize_effect_countdown_by_id",
        ],
    );
    assert!(!root().join("crates/core/runtime").exists());
    for (owner, slot) in [
        ("setup/current.rs", "SCRIPT_SETUP"),
        ("setup/current.rs", "OPENING"),
        ("session/current.rs", "RESET_STATE"),
        ("session/current.rs", "SESSION_PROGRESS"),
        ("session/current.rs", "ARTIFACT"),
        ("session/current.rs", "SESSION_JOB"),
        ("frame.rs", "FRAME"),
        ("event.rs", "EVENT_SINK_INSTALLED"),
        ("ice_filler.rs", "ICE_FILLER"),
        ("plant_fixer.rs", "PLANT_FIXER"),
        ("auto_collect.rs", "ITEM_COLLECTOR"),
        ("smart_remove.rs", "SMART_REMOVE"),
        ("fast_forward.rs", "FAST_FORWARD_TASK"),
        ("lifecycle.rs", "LIFECYCLE"),
        ("key.rs", "UNIQUE_KEY_BINDINGS"),
    ] {
        let declaration = format!("static {slot}:");
        assert!(read(&format!("crates/core/game/src/{owner}")).contains(&declaration));
    }
    for (module, slot) in [
        ("timeline", "TIMELINE"),
        ("tick", "SCHEDULER"),
        ("state_hook", "STATE_HOOKS"),
        ("event", "EVENTS"),
    ] {
        assert!(read(&format!("crates/core/schedule/src/{module}/current.rs")).contains(&format!("static {slot}:")));
    }
    assert!(read("crates/core/game/src/logic/cob.rs").contains("static COBS:"));
    for backend in [
        "pvz_emulator/backend",
        "pvz_portable/backend",
        "pvz_1_0_0_1051/injected",
    ] {
        let access = read(&format!("crates/backends/{backend}/src/access.rs"));
        assert!(access.contains("static BACKEND: BackendScope<"));
    }
    let logger_runtime = read("crates/core/game/src/diagnostics.rs");
    assert!(logger_runtime.contains("static LOGGER:"));
    assert!(logger_runtime.contains("static REPORTING:"));

    let game_session = read("crates/core/game/src/session.rs") + &read("crates/core/game/src/session/current.rs");
    for state in [
        "struct SessionControl",
        "struct WorldResetState",
        "struct ArtifactSlot",
        "struct SessionJobState",
        "struct SessionProgress",
        "struct RuntimeDispatchState",
    ] {
        assert!(game_session.contains(state), "game session state is missing `{state}`");
    }
    for forbidden in ["struct CurrentSession", "struct SessionState"] {
        assert!(
            !game_session.contains(forbidden),
            "session ownership violation: `{forbidden}`"
        );
    }

    let script_inputs = read("crates/scripting/api/src/setup/input.rs");
    for (module, parser) in [
        ("cards", "parse_card_abbreviations"),
        ("zombies", "parse_zombie_abbreviations"),
    ] {
        assert!(script_inputs.contains(parser));
        assert!(!read(&format!("crates/core/game/src/logic/{module}.rs")).contains(parser));
    }

    let game_waves = read("crates/core/game/src/logic/waves.rs");
    let scripting_waves = read("crates/scripting/api/src/dsl/input.rs");
    assert!(game_waves.contains("struct WaveSet"));
    assert!(!scripting_waves.contains("struct WaveSet"));
    assert!(read("crates/core/game/src/logic/card_timing.rs").contains("pub struct Retention("));
    assert!(!read("crates/scripting/api/src/dsl/card.rs").contains("pub struct Retention("));

    let selector = read("crates/core/current/src/lib.rs");
    for forbidden in [
        "thread_local!",
        "Timeline",
        "TickScheduler",
        "StateHookRegistry",
        "RuntimeLifecycle",
        "SessionArtifact",
    ] {
        assert!(!selector.contains(forbidden), "current selector contains `{forbidden}`");
    }
}

#[test]
fn scripting_has_no_runtime_binding_layer() {
    assert_complete_sources_exclude(
        &["crates/scripting/api/src", "extensions/rsvz-witness/src"],
        &[
            "trait Current",
            "struct Runtime;",
            "R::with_",
            "_with_runtime<",
            "_with_runtime::<",
            "connect_backend_with_runtime",
            "connect_with_runtime",
            "pub fn connect",
            "Runtime phantom",
            "TEST_SETUP",
            "with_tls",
        ],
    );
    let callable = read("crates/scripting/api/src/callable.rs");
    assert!(!callable.contains("with_backend"));
    assert!(!callable.contains("thread_local!"));
}

#[test]
fn cannon_operations_are_owned_by_game_and_script_overloads_only_adapt_inputs() {
    let api = read("crates/scripting/api/src/cob.rs");
    for removed in [
        "<R>",
        "R::",
        "with_tls",
        "CurrentRuntime",
        "CurrentTimelineRuntime",
        "pub struct CobManager",
    ] {
        assert!(!api.contains(removed), "cannon facade retained {removed}");
    }
    let owner = read("crates/core/game/src/cob.rs");
    assert!(owner.contains("pub use crate::logic::cob::CobManager;"));
    assert!(!owner.contains("pub struct CobManager"));
    let manager = read("crates/core/game/src/logic/cob/manager.rs");
    assert!(manager.contains("pub struct CobManager(Rc<RefCell<CobManagerState>>);"));
    assert!(!manager.contains("backend: &"));
    let primitives = read("crates/scripting/api/src/dsl/primitives.rs");
    assert!(primitives.contains("rsvz_game::cob::impact::prepare("));
    assert!(primitives.contains("rsvz_game::cob::impact::prepare_raw("));
    assert!(primitives.contains("rsvz_game::cob::impact::prepare_default_pair("));
    assert!(!primitives.contains("let second_row = if scene.has_pool()"));
    assert!(!primitives.contains("pool_land_targets"));
    assert!(!owner.contains("PhantomData"));
    let operations = read("crates/core/game/src/cob/current.rs");
    assert!(operations.contains("pub fn try_fire<"));
    let operations = syn::parse_file(&operations).expect("cob operation syntax");
    let fire = operations
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "try_fire" => Some(function),
            _ => None,
        })
        .expect("core try_fire operation");
    let bounds = fire.sig.generics.where_clause.as_ref().expect("try_fire capabilities")
        .predicates.iter().filter_map(|predicate| match predicate {
            syn::WherePredicate::Type(predicate) if matches!(&predicate.bounded_ty,
                syn::Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "CurrentBackend")) => Some(&predicate.bounds),
            _ => None,
        }).flatten().filter_map(|bound| match bound {
            syn::TypeParamBound::Trait(bound) => bound.path.segments.last().map(|segment| segment.ident.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
    for capability in ["ClockBackend", "CobBackend", "SceneBackend"] {
        assert!(
            bounds.iter().any(|bound| bound == capability),
            "try_fire lacks {capability}"
        );
    }
    assert!(read("crates/scripting/api/src/script.rs").contains("pub use rsvz_game::script::*"));
    assert!(read("crates/core/game/src/key/current.rs").contains("CurrentBackend: KeyboardStateBackend"));
}

#[test]
fn final_roots_and_macro_keep_one_thin_typed_dispatch() {
    let pe_root = read("crates/backends/pvz_emulator/final_entry/main.rs");
    assert_eq!(
        pe_root.trim(),
        "fn main() -> std::process::ExitCode {\n    rsvz_pvz_emulator_backend::host::run(user_script::__rsvz_dispatch)\n}"
    );
    let pe_entries = fs::read_dir(root().join("crates/backends/pvz_emulator/final_entry"))
        .expect("PE final entry should exist")
        .count();
    assert_eq!(pe_entries, 1);

    let pvz_root = read("crates/backends/pvz_1_0_0_1051/final_entry/lib.rs");
    assert_in_order(
        &pvz_root,
        &[
            "host::initialize(context, user_script::__rsvz_dispatch)",
            "host::request_unload(context)",
        ],
    );
    for forbidden in [
        "mod ",
        "#[path",
        "thread_local!",
        "static ",
        "loop {",
        "serde",
        "std::fs",
        "std::thread",
        "std::time",
        "user_script::script",
        "__rsvz_install_state_hooks",
        "runtime_dispatch",
    ] {
        assert!(!pvz_root.contains(forbidden), "1051 final root contains `{forbidden}`");
    }
    let pvz_entries = fs::read_dir(root().join("crates/backends/pvz_1_0_0_1051/final_entry"))
        .expect("1051 final entry should exist")
        .count();
    assert_eq!(pvz_entries, 1);

    let portable_root = read("crates/backends/pvz_portable/final_entry/lib.rs");
    assert!(portable_root.contains("user_script::__rsvz_dispatch"));
    assert_eq!(portable_root.matches("#[unsafe(no_mangle)]").count(), 3);
    assert!(portable_root.contains("fn pvzp_plugin_abi_version()"));
    assert!(portable_root.contains("fn pvzp_plugin_initialize("));
    assert!(portable_root.contains("fn pvzp_plugin_shutdown("));
    let backend_ffi = read("crates/backends/pvz_portable/backend/src/ffi.rs");
    let plugin_abi = backend_ffi
        .split("fn plugin_abi_version()")
        .nth(1)
        .unwrap()
        .split('{')
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let native_bridge = read("../PvZ-Portable/src/PvzpLib/Plugin.h");
    let native_abi = native_bridge
        .split("AbiVersion = ")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    assert_eq!(
        plugin_abi, native_abi,
        "Portable host and plugin must agree before initialization"
    );
    assert!(
        plugin_abi >= 4,
        "scalar-return bridge ABI must reject pre-migration DLLs"
    );
    for forbidden in [
        "mod ",
        "#[path",
        "thread_local!",
        "static ",
        "loop {",
        "serde",
        "std::fs",
        "std::thread",
        "std::time",
        "native_event",
        "begin_logic_frame",
        "begin_plant_effect",
        "event_interest",
        "runtime_dispatch",
    ] {
        assert!(
            !portable_root.contains(forbidden),
            "Portable final root contains `{forbidden}`"
        );
    }
    let portable_entries = fs::read_dir(root().join("crates/backends/pvz_portable/final_entry"))
        .expect("Portable final entry should exist")
        .count();
    assert_eq!(portable_entries, 1);

    let macro_source = read("crates/scripting/macros/src/script.rs");
    assert!(macro_source.contains("struct __RsvzStateHookInstaller"));
    assert!(macro_source.contains("trait __RsvzDefaultStateHookInstaller"));
    let dispatch = macro_source
        .split("pub fn __rsvz_dispatch(")
        .nth(1)
        .expect("script macro should generate one dispatch");
    assert_in_order(
        dispatch,
        &[
            "backend: &mut ::rsvz::__private::CurrentBackend",
            "input: ::rsvz::__private::DispatchInput",
            "::rsvz::__private::runtime_dispatch(",
            "backend",
            "input",
            "#ident",
            "__rsvz_install_state_hooks",
        ],
    );
    for forbidden in ["impl Fn", "Box<dyn", "callbacks", "usize"] {
        assert!(
            !dispatch.contains(forbidden),
            "generated dispatch contains `{forbidden}`"
        );
    }

    let state_hooks_macro = read("crates/scripting/macros/src/state_hooks.rs");
    assert!(state_hooks_macro.contains("impl __RsvzStateHookInstaller"));
    for forbidden in ["inventory", "linkme", "ctor", "link_section", "Vec<", "Box<dyn"] {
        assert!(
            !state_hooks_macro.contains(forbidden),
            "state-hooks macro contains `{forbidden}`"
        );
    }

    let pe_dispatch = read("crates/backends/pvz_emulator/backend/src/dispatch.rs");
    assert!(pe_dispatch.contains("pub type DispatchEntry = fn(&mut PeBackend, DispatchInput) -> DispatchResult;"));
    let pvz_dispatch = read("crates/backends/pvz_1_0_0_1051/injected/src/dispatch.rs");
    assert!(
        pvz_dispatch.contains("pub type DispatchEntry = fn(&mut Pvz1051Backend, DispatchInput) -> DispatchResult;")
    );
    let portable_dispatch = read("crates/backends/pvz_portable/backend/src/dispatch.rs");
    assert!(
        portable_dispatch
            .contains("pub type DispatchEntry = fn(&mut PortableBackend, DispatchInput) -> DispatchResult;")
    );

    let pe_host = read("crates/backends/pvz_emulator/backend/src/host/mod.rs");
    assert!(pe_host.contains("pub fn run(dispatch: DispatchEntry) -> ExitCode"));
    let pvz_host = read("crates/backends/pvz_1_0_0_1051/injected/src/host/mod.rs");
    assert!(pvz_host.contains("pub fn initialize(_context: *mut c_void, dispatch: DispatchEntry) -> u32"));
    let portable_host = read("crates/backends/pvz_portable/backend/src/host/mod.rs");
    assert!(portable_host.contains("pub fn initialize(config_json: &[u8], dispatch: DispatchEntry)"));
    for source in [&pe_host, &pvz_host, &portable_host] {
        for forbidden in ["user_script::", "rsvz_runtime", "rsvz_current", "impl Fn", "Box<dyn"] {
            assert!(!source.contains(forbidden), "backend host contains `{forbidden}`");
        }
    }
}

#[test]
fn native_event_ingress_is_one_fixed_copy_only_exception() {
    let sink = read("crates/core/backend-api/src/backend/event.rs");
    let body = sink
        .split("pub struct NativeEventSink {")
        .nth(1)
        .expect("NativeEventSink")
        .split('}')
        .next()
        .expect("NativeEventSink body");
    let fields: Vec<_> = body
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub "))
        .collect();
    assert_eq!(
        fields,
        [
            "pub interest: EventInterest,",
            "pub begin_logic_frame: fn(board_epoch: u64, main_counter: i32),",
            "pub begin_plant_effect: fn(PlantEffectAttemptFact) -> BeginPlantEffect,",
            "pub finish_plant_effect: fn(EventToken, PlantEffectOutcome),",
            "pub emit_home_entry: fn(HomeEntryFact),",
            "pub emit_gargantuar_spawned: fn(GargantuarSpawnedFact),",
            "pub emit_imp_thrown: fn(ImpThrownFact),",
            "pub emit_gargantuar_ash_hit: fn(GargantuarAshHitFact),",
            "pub end_logic_frame: fn(EventFrameStatus),",
        ]
    );
    for forbidden in ["Box<", "dyn ", "impl Fn", "*mut", "*const", "usize"] {
        assert!(!body.contains(forbidden), "NativeEventSink contains `{forbidden}`");
    }

    let registration = read("crates/core/game/src/registration/current.rs");
    let finish_generation = registration
        .split("pub fn finish_script_generation")
        .nth(1)
        .expect("finish_script_generation")
        .split("pub fn abort_script_generation")
        .next()
        .expect("finish_script_generation body");
    assert_in_order(
        finish_generation,
        &["dispatch_state_event(event)", "EventDispatcher::freeze"],
    );
    let events = read("crates/core/game/src/event.rs");
    let ingress = events.split("fn native_event_call").nth(1).expect("native_event_call");
    for required in ["catch_unwind", "fail_script", "fallback"] {
        assert!(
            ingress.contains(required),
            "native event ingress is missing `{required}`"
        );
    }
}

#[test]
fn physical_loops_dispatch_before_their_updates() {
    let native = read("crates/backends/pvz_1_0_0_1051/injected/src/runtime/hook/native_loop.rs");
    assert_in_order(&native, &["match dispatch_catching()", "call_original_update()"]);
    assert_in_order(
        &native,
        &[
            "profiler::measure_run_total(dispatch_catching)",
            "control::advance_fast_forward_update()",
        ],
    );

    let pe = read("crates/backends/pvz_emulator/backend/src/host/world.rs");
    assert_in_order(
        &pe,
        &[
            "let output = profile.dispatch",
            "dispatch(",
            "let update = profile.update",
            "world.update_world",
        ],
    );

    let pe_runtime = read("crates/backends/pvz_emulator/backend/src/runtime.rs");
    let backend_impl = pe_runtime
        .split("impl PeBackend {")
        .nth(1)
        .expect("PE backend implementation")
        .split("const fn main_counter_bits")
        .next()
        .expect("PE backend implementation body");
    for forbidden in [
        "pub fn update_world",
        "pub fn reset_with_config",
        "pub fn reset_spawn_random",
    ] {
        assert!(!backend_impl.contains(forbidden));
    }
}

#[test]
fn generic_host_protocols_and_startup_registries_cannot_return() {
    assert_complete_sources_exclude(
        &[
            "crates/core",
            "crates/scripting/api/src",
            "crates/backends/pvz_1_0_0_1051/final_entry",
            "crates/backends/pvz_emulator/final_entry",
        ],
        &[
            "struct ScriptHost",
            "trait ScriptHost",
            "struct RuntimeDriver",
            "trait RuntimeDriver",
            "struct RuntimeProtocol",
            "trait RuntimeProtocol",
            "struct HostEvent",
            "enum HostEvent",
            "struct HostAction",
            "enum HostAction",
            "struct Callbacks",
            "struct RuntimeCallbacks",
            ".CRT$XCU",
            "inventory::",
            "linkme::",
            "ctor::ctor",
        ],
    );
    assert!(!root().join("crates/core/rsvz-runtime-core").exists());
    assert!(!root().join("crates/backends/rsvz-backend-common").exists());
}

#[test]
fn measurement_tasks_stay_out_of_the_generic_runtime() {
    assert_complete_sources_exclude(
        &["crates/core/current/src"],
        &["rsvz_game::measure", "RefreshSession", "install_refresh", "MeasureMode"],
    );
    assert_complete_sources_exclude(
        &[
            "crates/core/current/src",
            "crates/backends/pvz_1_0_0_1051/injected/src",
            "crates/backends/pvz_emulator/backend/src",
            "crates/tools/rsvz-cli/src",
        ],
        &[
            "MeasureMode::DamageNarrow",
            "MeasureMode::BroadPass",
            "MeasureMode::Smash",
            "MeasureMode::Pogo",
            "damage-narrow",
            "broad-pass",
        ],
    );

    let session_runtime = read("crates/scripting/api/src/runtime.rs");
    assert!(!session_runtime.contains("install_refresh"));

    let facade = read("crates/scripting/api/src/measure.rs");
    assert!(!facade.contains("CurrentSessionRuntime"));
    assert!(!facade.contains("thread_local!"));
    let measure = read("crates/core/game/src/measure/current.rs");
    let ordinary_event_api = measure
        .split_once("pub fn broad_pass_trials")
        .expect("BroadPass API")
        .1
        .split_once("pub fn smash_trials")
        .expect("Smash API")
        .0;
    assert!(ordinary_event_api.contains("CurrentBackend: PlantReadBackend + WorldResetBackend"));
    assert!(!ordinary_event_api.contains("ZombieRawFactsBackend"));
    let damage_api = measure
        .split_once("pub fn damage_narrow_trials")
        .expect("DamageNarrow API")
        .1
        .split_once("pub fn broad_pass_trials")
        .expect("ordinary event APIs")
        .0;
    assert!(damage_api.contains("ZombieRawFactsBackend + ZombieStateBackend"));
    assert_in_order(
        &measure,
        &[
            "StateEvent::AfterScript",
            "initialize_refresh(limit)",
            "fn initialize_refresh",
        ],
    );
    for required in [
        "RefreshTask::new",
        "EventMeasureTask::new",
        "install_internal_event_interceptor",
        "StateEvent::BeforeTick",
        "spawn_finalizer",
        "crate::session::request_world_reset",
        "crate::session::set_session_artifact",
    ] {
        assert!(measure.contains(required), "measurement is missing {required}");
    }
    let opening = read("crates/core/game/src/setup/current.rs");
    assert!(!opening.contains("CommonZombieDanceBackend"));
    assert!(!opening.contains("CobImpactDelayBackend"));
    assert_in_order(
        &opening,
        &[
            "finish_script_opening(setup)",
            "apply_current_refresh_rules(setup.measurement.refresh())",
        ],
    );
    let dispatcher = read("crates/core/game/src/dispatch.rs");
    assert_in_order(
        &dispatcher,
        &[
            "crate::setup::finish_current_opening()",
            "BattleEntryBackend::start_battle",
        ],
    );

    let custom = read("crates/scripting/api/tests/custom_measure.rs");
    for public_api in [
        "rsvz::claim_session_job",
        "rsvz::request_world_reset",
        "rsvz::tick::spawn",
        "rsvz::publish_artifact",
        "rsvz::stop_script",
    ] {
        assert!(
            custom.contains(public_api),
            "custom measure proof is missing `{public_api}`"
        );
    }
}

#[test]
fn token_access_preserves_the_native_playing_requirement() {
    let frame = read("crates/core/game/src/frame/entities.rs");
    let frame = syn::parse_file(&frame)
        .expect("Frame source parses")
        .items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Struct(item) if item.ident == "Frame" => Some(item),
            _ => None,
        })
        .expect("Frame type");
    assert_eq!(frame.fields.len(), 1, "Frame owns only one backend borrow");
    let syn::Type::Reference(borrow) = &frame.fields.iter().next().unwrap().ty else {
        panic!("Frame must borrow its backend");
    };
    assert!(borrow.mutability.is_none(), "Frame shares its backend");
    assert!(matches!(&*borrow.elem, syn::Type::Path(ty) if ty.path.is_ident("CurrentBackend")));
    // Constructor behavior is covered by frame_api and no_board_frame_1051;
    // do not freeze closure/field variable spelling or a particular call expression.
    for path in [
        "crates/backends/none/src/lib.rs",
        "crates/backends/pvz_emulator/backend/src/access.rs",
        "crates/backends/pvz_portable/backend/src/access.rs",
        "crates/backends/pvz_1_0_0_1051/injected/src/access.rs",
    ] {
        let access = read(path);
        assert!(!access.contains("BoardAccess"), "duplicate Board API in {path}");
        assert!(!access.contains("struct Restore"), "access cache restoration in {path}");
    }
    let items = read("crates/backends/pvz_1_0_0_1051/injected/src/impls/item.rs");
    let body = items
        .split("fn items(&self)")
        .nth(1)
        .unwrap()
        .split_once('{')
        .unwrap()
        .1;
    // This is a source-level contract gate, not a live 1051 test: even a valid
    // LevelIntro Board must reach the Playing check before any item-pool access.
    assert!(body.trim_start().starts_with("self.ensure_game_ui(GameUi::Playing)?;"));
    let runtime = read("crates/backends/pvz_1_0_0_1051/injected/src/runtime/backend.rs");
    let guard = runtime.split("fn ensure_current_game_ui(").nth(1).unwrap();
    assert!(guard.contains("if actual != expected"));
    assert!(guard.contains("Err(Pvz1051Error::WrongGameUi { expected, actual })"));
}
