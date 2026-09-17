//! Build and run native PvZ-Emulator final runners.

use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::report::ReportFormat;
use crate::runner_config::{
    RunRequest, configure_generated_runner_command, default_thread_count, generate_default_seed_base,
};
use crate::{BuildProfile, PeBuildOptions, PeBuildToolchain, PePgoMode, ScriptRunnerOptions, wire};

const DEV_SCRIPTS_DIR: &str = "dev-scripts";

/// Build and run a PvZ-Emulator final runner.
#[derive(Args, Debug, Clone)]
pub struct RunPeArgs {
    /// Standalone Rust script source prepared by the rsvz CLI.
    #[arg(
        value_name = "SCRIPT",
        conflicts_with_all = [
            "script_crate",
            "dev_script",
            "script_package",
            "script_features",
            "no_script_default_features"
        ],
        required_unless_present_any = ["script_crate", "dev_script"]
    )]
    script: Option<PathBuf>,
    /// External script crate directory or Cargo.toml to link into a generated runner.
    #[arg(long, value_name = "PATH", conflicts_with = "dev_script")]
    script_crate: Option<PathBuf>,
    /// Worktree-local script under dev-scripts, or an absolute script crate path.
    #[arg(long, value_name = "PATH", conflicts_with = "script_crate")]
    dev_script: Option<PathBuf>,
    /// Script name used for generated runner paths. Defaults to the dev script or package name.
    #[arg(long)]
    script_name: Option<String>,
    /// Package name when the script path belongs to a multi-package workspace.
    #[arg(long)]
    script_package: Option<String>,
    /// Feature to enable on the user script package. May be repeated.
    #[arg(long = "script-feature")]
    script_features: Vec<String>,
    /// Disable the user script package's default features.
    #[arg(long)]
    no_script_default_features: bool,
    /// Build release artifacts instead of debug artifacts.
    #[arg(long)]
    release: bool,
    /// Generate and build the runner without advancing runtime simulation.
    #[arg(long)]
    build_only: bool,
    /// Number of worker-local PE worlds.
    #[arg(long)]
    threads: Option<NonZeroUsize>,
    /// Use the same deterministic app seed for every worker.
    #[arg(long, conflicts_with = "base_seed")]
    seed: Option<u32>,
    /// Derive distinct deterministic worker seeds from this base seed.
    #[arg(long, conflicts_with = "seed")]
    base_seed: Option<u32>,
    /// Maximum wall-clock seconds for runtime execution.
    #[arg(long)]
    max_wall_secs: Option<u64>,
    /// Maximum PE logical frames per worker.
    #[arg(long)]
    max_sim_frames: Option<u64>,
    /// Maximum completed levels per worker.
    #[arg(long)]
    max_levels: Option<u64>,
    /// Write a script-published artifact as its complete root JSON value.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
    /// Include detailed worker statistics in the report.
    #[arg(long, conflicts_with_all = ["json", "no_report"])]
    stats: bool,
    /// Print machine-readable JSON report.
    #[arg(long, conflicts_with_all = ["stats", "no_report"])]
    json: bool,
    /// Suppress report output.
    #[arg(long, conflicts_with_all = ["stats", "json", "profile", "profile_detail"])]
    no_report: bool,
    /// Include low-overhead PE runtime frame profiling in the report.
    #[arg(long)]
    profile: bool,
    /// Include opt-in detailed PE runtime stage profiling in the report.
    #[arg(long)]
    profile_detail: bool,
    /// Record performance for this initial window, without stopping the script.
    #[arg(long)]
    performance_window_secs: Option<u64>,
    /// Toolchain mode for PE native dependencies.
    #[arg(long, value_enum, default_value_t = PeBuildToolchain::Clang)]
    pe_toolchain: PeBuildToolchain,
    /// Build with PGO instrumentation and write runtime .profraw files under this directory.
    #[arg(long, value_name = "DIR", conflicts_with = "pgo_use")]
    pgo_generate: Option<PathBuf>,
    /// Rebuild with LLVM PGO optimization using a merged .profdata file.
    #[arg(long, value_name = "FILE", conflicts_with = "pgo_generate")]
    pgo_use: Option<PathBuf>,
    /// RustVsZombies workspace root. Defaults to searching from the current directory, then the CLI executable path.
    #[arg(long, value_name = "PATH")]
    workspace_root: Option<PathBuf>,
}

impl RunPeArgs {
    pub fn take_bare_script(&mut self) -> Option<(PathBuf, Option<String>)> {
        self.script.take().map(|script| (script, self.script_name.clone()))
    }

    #[must_use]
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    pub fn use_prepared_script(&mut self, manifest: PathBuf, default_name: String) {
        self.script_crate = Some(manifest);
        self.script_name.get_or_insert(default_name);
    }
}

pub fn run(args: RunPeArgs, root: &Path, current_dir: &Path) -> Result<()> {
    if args.script.is_some() {
        bail!("bare .rs scripts must be prepared by the rsvz CLI");
    }
    let script_source = resolve_script_source(root, &args)?;
    let profile = BuildProfile::from_release_flag(args.release);
    let pgo = match (args.pgo_generate.as_deref(), args.pgo_use.as_deref()) {
        (Some(dir), None) => {
            let dir = absolute(current_dir, dir);
            fs::create_dir_all(&dir).with_context(|| format!("创建 PGO profile 目录失败: {}", dir.display()))?;
            PePgoMode::Generate(dir)
        }
        (None, Some(path)) => PePgoMode::Use(absolute(current_dir, path)),
        (None, None) => PePgoMode::None,
        (Some(_), Some(_)) => unreachable!("clap rejects simultaneous PGO generate and use modes"),
    };
    let build_options = PeBuildOptions {
        toolchain: args.pe_toolchain,
        pgo,
    };
    let runner = crate::prepare_script_runner(
        root,
        ScriptRunnerOptions {
            current_dir,
            script_crate: &script_source.script_crate,
            script_name: script_source.script_name.as_deref(),
            script_package: args.script_package.as_deref(),
            script_features: &args.script_features,
            script_default_features: !args.no_script_default_features,
            profile,
            build_options: &build_options,
        },
    )?;
    let exe_path = crate::cargo_build_generated_runner(root, &runner)?;

    if args.build_only {
        println!("PE runner EXE: {}", exe_path.display());
        println!("PE runner manifest: {}", runner.manifest_path.display());
        return Ok(());
    }

    let request = resolve_run_request(&args, current_dir);
    let report_format = if args.no_report {
        ReportFormat::None
    } else if args.json {
        ReportFormat::Json
    } else {
        ReportFormat::Text { stats: args.stats }
    };
    run_generated_runner(root, &exe_path, &request, runner.pgo_generate_dir(), report_format)
}

fn resolve_run_request(args: &RunPeArgs, current_dir: &Path) -> RunRequest {
    let seed_mode = match (args.seed, args.base_seed) {
        (Some(seed), None) => wire::SeedMode::Fixed { seed },
        (None, Some(seed)) => wire::SeedMode::Base { seed },
        (None, None) => wire::SeedMode::Base {
            seed: generate_default_seed_base(),
        },
        (Some(_), Some(_)) => unreachable!("clap rejects simultaneous seed modes"),
    };
    RunRequest {
        threads: args.threads.unwrap_or_else(default_thread_count),
        seed_mode,
        max_wall: args.max_wall_secs.map(Duration::from_secs),
        max_sim_frames: args.max_sim_frames,
        max_levels: args.max_levels,
        profile: args.profile || args.profile_detail,
        profile_detail: args.profile_detail,
        performance_window: args.performance_window_secs.map(Duration::from_secs),
        output_path: args.output.as_deref().map(|path| absolute(current_dir, path)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScriptSource {
    script_crate: PathBuf,
    script_name: Option<String>,
}

fn resolve_script_source(root: &Path, args: &RunPeArgs) -> Result<ScriptSource> {
    if let Some(script_crate) = args.script_crate.as_deref() {
        return Ok(ScriptSource {
            script_crate: script_crate.to_path_buf(),
            script_name: args.script_name.clone(),
        });
    }

    let dev_script = args.dev_script.as_deref().context("missing dev script")?;
    let script_crate = if dev_script.is_absolute() {
        dev_script.to_path_buf()
    } else {
        root.join(DEV_SCRIPTS_DIR).join(dev_script)
    };
    let script_name = match args.script_name.clone() {
        Some(script_name) => script_name,
        None => dev_script_name(dev_script)?,
    };
    Ok(ScriptSource {
        script_crate,
        script_name: Some(script_name),
    })
}

fn run_generated_runner(
    root: &Path, exe_path: &Path, request: &RunRequest, pgo_generate: Option<&Path>, report_format: ReportFormat,
) -> Result<()> {
    let mut command = Command::new(exe_path);
    command.current_dir(root);
    if let Some(dir) = pgo_generate {
        command.env("LLVM_PROFILE_FILE", dir.join("rsvz-pe-%m-%p.profraw"));
    }
    let invocation = configure_generated_runner_command(
        &mut command,
        &root.join("target").join("rsvz").join("runtime").join("pe"),
        request,
    )?;
    let status = command
        .status()
        .with_context(|| format!("执行 generated PE runner 失败: {}", exe_path.display()));
    let config_cleanup = remove_if_exists(&invocation.config_path);
    let status = status?;
    config_cleanup?;

    let result_path = &invocation.config.raw_result_path;
    let raw = fs::read_to_string(result_path).with_context(|| {
        format!(
            "generated PE runner 未发布有效结果（exit code: {:?}）: {}",
            status.code(),
            result_path.display()
        )
    })?;
    let result: wire::PeRunResult = serde_json::from_str(&raw).context("解析 generated PE raw result JSON 失败")?;
    result
        .validate_for(&invocation.config)
        .map_err(anyhow::Error::msg)
        .context("校验 generated PE raw result 失败")?;
    remove_if_exists(result_path)?;

    if matches!(result.terminal, wire::Terminal::Success {}) && !status.success() {
        bail!(
            "generated PE runner published success but exited unsuccessfully: {:?}",
            status.code()
        );
    }
    crate::report::render_result(&result, report_format)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("删除本次 PE runtime 文件失败: {}", path.display())),
    }
}

fn dev_script_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .with_context(|| format!("无法从 --dev-script 推断 script name: {}", path.display()))?;
    Ok(name.to_owned())
}

fn absolute(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> RunPeArgs {
        RunPeArgs {
            script: None,
            script_crate: None,
            dev_script: Some(PathBuf::from("lowdsl/pe24")),
            script_name: None,
            script_package: None,
            script_features: Vec::new(),
            no_script_default_features: false,
            release: false,
            build_only: true,
            threads: None,
            seed: None,
            base_seed: None,
            max_wall_secs: None,
            max_sim_frames: None,
            max_levels: None,
            output: None,
            stats: false,
            json: false,
            no_report: false,
            profile: false,
            profile_detail: false,
            performance_window_secs: None,
            pe_toolchain: PeBuildToolchain::Clang,
            pgo_generate: None,
            pgo_use: None,
            workspace_root: None,
        }
    }

    #[test]
    fn run_request_uses_nonzero_thread_default_and_seed_modes() {
        let current_dir = Path::new("C:/invocation");
        let default = resolve_run_request(&args(), current_dir);
        assert!(default.threads.get() >= 1);
        assert!(matches!(default.seed_mode, wire::SeedMode::Base { .. }));

        let mut fixed_args = args();
        fixed_args.threads = NonZeroUsize::new(1);
        fixed_args.seed = Some(10);
        let fixed = resolve_run_request(&fixed_args, current_dir);
        assert_eq!(fixed.threads.get(), 1);
        assert_eq!(fixed.seed_mode, wire::SeedMode::Fixed { seed: 10 });

        let mut base_args = args();
        base_args.base_seed = Some(20);
        assert_eq!(
            resolve_run_request(&base_args, current_dir).seed_mode,
            wire::SeedMode::Base { seed: 20 }
        );

        let mut output_args = args();
        output_args.output = Some(PathBuf::from("result.json"));
        assert_eq!(
            resolve_run_request(&output_args, current_dir).output_path,
            Some(current_dir.join("result.json"))
        );
    }
}
