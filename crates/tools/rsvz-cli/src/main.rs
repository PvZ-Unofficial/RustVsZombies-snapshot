use std::env;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rsvz_backend_1051_tooling::inject_cli;
use rsvz_pvz_emulator_tooling::run_cli;
use rsvz_pvz_portable_tooling::run_cli as portable_run_cli;

mod script;

#[derive(Parser)]
#[command(name = "rsvz")]
#[command(about = "RustVsZombies CLI tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "inject-1051")]
    Inject1051(inject_cli::Inject1051Args),
    #[command(name = "run-1051")]
    Run1051(inject_cli::Run1051Args),
    #[command(name = "run-pe")]
    RunPe(run_cli::RunPeArgs),
    #[command(name = "run-portable")]
    RunPortable(portable_run_cli::RunPortableArgs),
    #[command(name = "prepare-script")]
    PrepareScript(script::PrepareScriptArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run1051(mut args) => {
            let current_dir = env::current_dir().context("读取当前目录失败")?;
            let root = script::resolve_workspace_root(&current_dir, args.script.workspace_root())?;
            if let Some((source, script_name)) = args.script.take_bare_script() {
                let prepared = script::prepare_for_command(
                    &root,
                    &current_dir,
                    &source,
                    script::Backend::Pvz1_0_0_1051,
                    script_name.as_deref(),
                )?;
                args.script.use_prepared_script(
                    prepared.script_manifest().to_path_buf(),
                    prepared.script_name().to_owned(),
                )?;
            }
            inject_cli::run_owned(args, &root, &current_dir)
        }
        Commands::Inject1051(mut args) => {
            let current_dir = env::current_dir().context("读取当前目录失败")?;
            let root = script::resolve_workspace_root(&current_dir, args.workspace_root())?;
            if let Some((source, script_name)) = args.take_bare_script() {
                let prepared = script::prepare_for_command(
                    &root,
                    &current_dir,
                    &source,
                    script::Backend::Pvz1_0_0_1051,
                    script_name.as_deref(),
                )?;
                args.use_prepared_script(
                    prepared.script_manifest().to_path_buf(),
                    prepared.script_name().to_owned(),
                )?;
            }
            inject_cli::run(args, &root, &current_dir)
        }
        Commands::RunPe(mut args) => {
            let current_dir = env::current_dir().context("读取当前目录失败")?;
            let root = script::resolve_workspace_root(&current_dir, args.workspace_root())?;
            if let Some((source, script_name)) = args.take_bare_script() {
                let prepared = script::prepare_for_command(
                    &root,
                    &current_dir,
                    &source,
                    script::Backend::PvzEmulator,
                    script_name.as_deref(),
                )?;
                args.use_prepared_script(
                    prepared.script_manifest().to_path_buf(),
                    prepared.script_name().to_owned(),
                );
            }
            run_cli::run(args, &root, &current_dir)
        }
        Commands::RunPortable(mut args) => {
            let current_dir = env::current_dir().context("读取当前目录失败")?;
            let root = script::resolve_workspace_root(&current_dir, args.workspace_root())?;
            if let Some((source, script_name)) = args.take_bare_script() {
                let prepared = script::prepare_for_command(
                    &root,
                    &current_dir,
                    &source,
                    script::Backend::PvzPortable,
                    script_name.as_deref(),
                )?;
                args.use_prepared_script(
                    prepared.script_manifest().to_path_buf(),
                    prepared.script_name().to_owned(),
                )?;
            }
            portable_run_cli::run(args, &root, &current_dir)
        }
        Commands::PrepareScript(args) => script::run(args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_native_commands_require_isolation_inputs() {
        assert!(Cli::try_parse_from(["rsvz", "run-1051", "--dev-script", "live_smoke"]).is_err());
        assert!(
            Cli::try_parse_from([
                "rsvz",
                "run-1051",
                "--dev-script",
                "live_smoke",
                "--game",
                "game.exe",
                "--profile-template",
                "userdata",
                "--build-only"
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from([
                "rsvz",
                "run-portable",
                "--dev-script",
                "live_smoke",
                "--close-game-on-finish"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "rsvz",
                "run-portable",
                "--dev-script",
                "live_smoke",
                "--close-game-on-finish",
                "--profile-template",
                "profile",
                "--no-wait"
            ])
            .is_err()
        );
    }

    #[test]
    fn bare_scripts_conflict_with_other_script_sources_and_opaque_dlls() {
        assert!(Cli::try_parse_from(["rsvz", "run-pe", "foo.rs", "--script-crate", "script/Cargo.toml"]).is_err());
        assert!(Cli::try_parse_from(["rsvz", "inject-1051", "foo.rs", "--dll", "external.dll"]).is_err());
    }

    #[test]
    fn run_pe_rejects_invalid_runtime_and_build_combinations() {
        let base = ["rsvz", "run-pe", "--dev-script", "lowdsl/pe24"];
        for extra in [
            ["--threads", "0"],
            ["--json", "--stats"],
            ["--pgo-generate", "profiles"],
        ] {
            let mut args = base.to_vec();
            args.extend(extra);
            if extra[0] == "--pgo-generate" {
                args.extend(["--pgo-use", "merged.profdata"]);
            }
            assert!(Cli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn inject_1051_requires_a_script_or_an_opaque_dll() {
        assert!(Cli::try_parse_from(["rsvz", "inject-1051"]).is_err());
        assert!(Cli::try_parse_from(["rsvz", "inject-1051", "--dll", "external.dll"]).is_ok());
    }

    #[test]
    fn prepare_script_requires_json_and_accepts_both_backend_names() {
        for backend in ["pvz-emulator", "pvz-1-0-0-1051", "pvz-portable"] {
            assert!(Cli::try_parse_from(["rsvz", "prepare-script", "foo.rs", "--backend", backend, "--json"]).is_ok());
        }
        assert!(Cli::try_parse_from(["rsvz", "prepare-script", "foo.rs", "--backend", "pvz-emulator"]).is_err());
    }
}
