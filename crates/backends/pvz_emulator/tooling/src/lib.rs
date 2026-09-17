//! Build and run helpers for the PvZ-Emulator backend family.

mod build_options;
mod report;
pub mod run_cli;
mod runner_config;
#[path = "../../backend/src/host/wire.rs"]
pub(crate) mod wire;

pub use build_options::{BuildProfile, PeBuildOptions, PeBuildToolchain, PePgoMode};

use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use cargo_metadata::{Message, Metadata, MetadataCommand, PackageId, TargetKind};

use build_options::{ResolvedPeBuildOptions, stable_hash_hex};

const RSVZ_CRATE_RELATIVE: &str = "crates/scripting/api";
const PE_BACKEND_RELATIVE: &str = "crates/backends/pvz_emulator/backend";
const PE_FINAL_ENTRY_RELATIVE: &str = "crates/backends/pvz_emulator/final_entry/main.rs";
const PE_FINAL_ENTRY_FROM_GENERATED: &str = "../../../../../crates/backends/pvz_emulator/final_entry/main.rs";
const GENERATED_ROOT_NAME: &str = "pe";
pub const GENERATED_RUNNER_TARGET: &str = "x86_64-pc-windows-msvc";
const GENERATED_RUNNER_TARGET_ENV: &str = "X86_64_PC_WINDOWS_MSVC";
static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRunnerOptions<'a> {
    pub current_dir: &'a Path,
    pub script_crate: &'a Path,
    pub script_name: Option<&'a str>,
    pub script_package: Option<&'a str>,
    pub script_features: &'a [String],
    pub script_default_features: bool,
    pub profile: BuildProfile,
    pub build_options: &'a PeBuildOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRunnerBuild {
    pub manifest_path: PathBuf,
    profile: BuildProfile,
    build_options: ResolvedPeBuildOptions,
    target: ValidatedTarget,
}

impl ScriptRunnerBuild {
    pub(crate) fn pgo_generate_dir(&self) -> Option<&Path> {
        self.build_options.pgo_generate_dir()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedTarget {
    package_id: PackageId,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScriptPackageInfo {
    id: PackageId,
    name: String,
    manifest_path: PathBuf,
    manifest_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScriptRunnerNames {
    package_name: String,
    bin_name: String,
    generated_dir_name: String,
}

pub fn prepare_script_runner(root: &Path, options: ScriptRunnerOptions<'_>) -> Result<ScriptRunnerBuild> {
    let build_options = options.build_options.resolve(options.profile, root)?;
    let script_manifest = resolve_script_manifest_path(options.current_dir, options.script_crate)?;
    let package = resolve_script_package(&script_manifest, options.script_package)?;
    let script_name = options.script_name.unwrap_or(&package.name);
    let script_features = normalized_features(options.script_features);
    let names = script_runner_names(script_name, &package, &script_features, options.script_default_features);
    let generated_dir = root
        .join("target")
        .join("rsvz")
        .join("generated")
        .join(GENERATED_ROOT_NAME)
        .join(&names.generated_dir_name);
    fs::create_dir_all(&generated_dir)
        .with_context(|| format!("创建 generated PE runner crate 目录失败: {}", generated_dir.display()))?;
    ensure_manifest_only_generated_package(&generated_dir)?;
    let rsvz_path = root.join(RSVZ_CRATE_RELATIVE);
    let backend_path = root.join(PE_BACKEND_RELATIVE);
    write_if_changed(
        &generated_dir.join("Cargo.toml"),
        &render_runner_manifest(
            &names.package_name,
            &names.bin_name,
            PE_FINAL_ENTRY_FROM_GENERATED,
            &rsvz_path,
            &backend_path,
            &package,
            &script_features,
            options.script_default_features,
        ),
    )?;
    let manifest_path = generated_dir.join("Cargo.toml");
    let target = validate_final_graph(
        root,
        &manifest_path,
        GENERATED_RUNNER_TARGET,
        &names.bin_name,
        &package.id,
    )?;

    Ok(ScriptRunnerBuild {
        manifest_path,
        profile: options.profile,
        build_options,
        target,
    })
}

fn script_runner_names(
    script_name: &str, package: &ScriptPackageInfo, script_features: &[String], script_default_features: bool,
) -> ScriptRunnerNames {
    let safe_name = safe_path_name(script_name);
    let ident = safe_rust_ident_fragment(script_name);
    let package_id = package.id.to_string();
    let package_path = normalized_identity_path(&package.manifest_path);
    let mut identity = vec![
        "rsvz-generated-target-v3".to_owned(),
        script_name.to_owned(),
        package_id,
        package_path,
        "default-features".to_owned(),
        script_default_features.to_string(),
        "script-features".to_owned(),
        script_features.len().to_string(),
    ];
    identity.extend(script_features.iter().cloned());
    let identity = identity.iter().map(String::as_str).collect::<Vec<_>>();
    let hash = stable_hash_hex(&identity);
    ScriptRunnerNames {
        package_name: format!("rsvz-pe-runner-{safe_name}-{hash}"),
        bin_name: format!("rsvz_pe_runner_{ident}_{hash}"),
        generated_dir_name: format!("{safe_name}-{hash}"),
    }
}

fn resolve_script_manifest_path(current_dir: &Path, script_crate: &Path) -> Result<PathBuf> {
    let path = if script_crate.is_absolute() {
        script_crate.to_path_buf()
    } else {
        current_dir.join(script_crate)
    };
    let manifest = if path.file_name().is_some_and(|name| name == "Cargo.toml") {
        path
    } else {
        path.join("Cargo.toml")
    };
    fs::canonicalize(&manifest).with_context(|| format!("规范化 script Cargo.toml 路径失败: {}", manifest.display()))
}

fn resolve_script_package(script_manifest: &Path, script_package: Option<&str>) -> Result<ScriptPackageInfo> {
    let metadata = MetadataCommand::new()
        .manifest_path(script_manifest)
        .no_deps()
        .exec()
        .with_context(|| format!("读取 script crate metadata 失败: {}", script_manifest.display()))?;
    let workspace_packages = metadata
        .packages
        .iter()
        .filter(|package| metadata.workspace_members.contains(&package.id))
        .collect::<Vec<_>>();
    let package = if let Some(name) = script_package {
        let mut matches = workspace_packages
            .iter()
            .copied()
            .filter(|package| package.name.as_str() == name);
        let package = matches.next().with_context(|| {
            format!(
                "script package `{name}` 不在 workspace 中: {}",
                script_manifest.display()
            )
        })?;
        if matches.next().is_some() {
            bail!("script package `{name}` 匹配到多个 package");
        }
        package
    } else {
        let mut matches = workspace_packages.iter().copied().filter(|package| {
            fs::canonicalize(package.manifest_path.as_std_path()).is_ok_and(|path| path == script_manifest)
        });
        match (matches.next(), matches.next()) {
            (Some(package), None) => package,
            (None, _) if workspace_packages.len() == 1 => workspace_packages[0],
            (None, _) => bail!("script crate 是 virtual 或多 package workspace，请使用 --script-package 指定 package"),
            (Some(_), Some(_)) => bail!("script Cargo.toml 匹配到多个 package"),
        }
    };
    let manifest_path = fs::canonicalize(package.manifest_path.as_std_path())
        .with_context(|| format!("规范化 script package Cargo.toml 路径失败: {}", package.manifest_path))?;
    let manifest_dir = manifest_path
        .parent()
        .context("script package manifest 缺少父目录")?
        .to_path_buf();
    let lib_targets = package
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == &TargetKind::Lib))
        .collect::<Vec<_>>();
    if lib_targets.len() != 1 {
        bail!(
            "用户 package 必须恰好有一个 lib target，实际为 {}: {}",
            lib_targets.len(),
            script_manifest.display()
        );
    }
    Ok(ScriptPackageInfo {
        id: package.id.clone(),
        name: package.name.clone(),
        manifest_path,
        manifest_dir,
    })
}

fn metadata_for_target(manifest_path: &Path, target: &str) -> Result<Metadata> {
    MetadataCommand::new()
        .manifest_path(manifest_path)
        .other_options(vec!["--filter-platform".to_owned(), target.to_owned()])
        .exec()
        .with_context(|| {
            format!(
                "读取 generated PE runner 最终 target metadata 失败: {}",
                manifest_path.display()
            )
        })
}

fn validate_final_graph(
    root_dir: &Path, manifest_path: &Path, target: &str, expected_target_name: &str, expected_user: &PackageId,
) -> Result<ValidatedTarget> {
    let metadata = metadata_for_target(manifest_path, target)?;
    let root = metadata
        .root_package()
        .context("generated PE runner metadata 缺少 root package")?;
    let resolve = metadata
        .resolve
        .as_ref()
        .context("generated PE runner metadata 缺少 resolve graph")?;
    let root_node = node_by_id(resolve, &root.id)?;
    let targets = root
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == &TargetKind::Bin))
        .collect::<Vec<_>>();
    let [root_target] = targets.as_slice() else {
        bail!(
            "generated PE runner 必须恰好有一个 bin target，实际为 {}",
            targets.len()
        );
    };
    if root_target.name != expected_target_name {
        bail!(
            "generated PE runner target name 不一致：期望 {expected_target_name}，实际 {}",
            root_target.name
        );
    }
    let expected_source = fs::canonicalize(root_dir.join(PE_FINAL_ENTRY_RELATIVE)).with_context(|| {
        format!(
            "规范化 fixed PE entry 失败: {}",
            root_dir.join(PE_FINAL_ENTRY_RELATIVE).display()
        )
    })?;
    let actual_source = fs::canonicalize(root_target.src_path.as_std_path())
        .with_context(|| format!("规范化 generated PE target source 失败: {}", root_target.src_path))?;
    if actual_source != expected_source {
        bail!(
            "generated PE target source 不一致：期望 {}，实际 {}",
            expected_source.display(),
            actual_source.display()
        );
    }

    let user = root_node
        .deps
        .iter()
        .find(|dependency| dependency.name == "user_script")
        .context("generated PE runner 缺少重命名依赖 `user_script`")?;
    if &user.pkg != expected_user {
        bail!(
            "generated PE runner 的 user_script PackageId 发生变化：期望 {expected_user}，实际 {}",
            user.pkg
        );
    }
    let rsvz = root_node
        .deps
        .iter()
        .find(|dependency| dependency.name == "rsvz")
        .context("generated PE runner 缺少依赖 `rsvz`")?;
    require_pe_backend_features(node_by_id(resolve, &rsvz.pkg)?)?;
    let backend = root_node
        .deps
        .iter()
        .find(|dependency| dependency.name == "rsvz_pvz_emulator_backend")
        .context("generated PE runner 缺少依赖 `rsvz_pvz_emulator_backend`")?;
    let backend_node = node_by_id(resolve, &backend.pkg)?;
    require_feature(backend_node, "host")?;

    for forbidden in [
        "rsvz-pvz-emulator-tooling",
        "rsvz-backend-1051-tooling",
        "rsvz-pvz-portable-tooling",
        "rsvz-pvz1051-injected",
        "rsvz-pvz-portable-backend",
        "pvzp-rs",
    ] {
        if metadata.packages.iter().any(|package| package.name == forbidden) {
            bail!("generated PE final graph 不得包含未选 backend/tooling package `{forbidden}`");
        }
    }
    Ok(ValidatedTarget {
        package_id: root.id.clone(),
        name: root_target.name.clone(),
    })
}

fn node_by_id<'a>(resolve: &'a cargo_metadata::Resolve, id: &PackageId) -> Result<&'a cargo_metadata::Node> {
    resolve
        .nodes
        .iter()
        .find(|node| &node.id == id)
        .with_context(|| format!("PackageId `{id}` 不在 generated final resolve graph 中"))
}

fn require_feature(node: &cargo_metadata::Node, feature: &str) -> Result<()> {
    if !node.features.iter().any(|enabled| enabled == feature) {
        bail!("PackageId `{}` 未启用必要 feature `{feature}`", node.id)
    }
    Ok(())
}

fn forbid_feature(node: &cargo_metadata::Node, feature: &str) -> Result<()> {
    if node.features.iter().any(|enabled| enabled == feature) {
        bail!("PackageId `{}` 意外启用了 feature `{feature}`", node.id)
    }
    Ok(())
}

fn require_pe_backend_features(node: &cargo_metadata::Node) -> Result<()> {
    require_feature(node, "pvz-emulator")?;
    for forbidden in ["pvz-1-0-0-1051", "pvz-portable"] {
        forbid_feature(node, forbidden)?;
    }
    Ok(())
}

fn render_runner_manifest(
    package_name: &str, bin_name: &str, target_path: &str, rsvz_path: &Path, backend_path: &Path,
    script_package: &ScriptPackageInfo, script_features: &[String], script_default_features: bool,
) -> String {
    let script_features = script_features
        .iter()
        .map(|feature| toml_string(feature))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.95\"\npublish = false\nbuild = false\nautolib = false\nautobins = false\nautoexamples = false\nautotests = false\nautobenches = false\n\n[[bin]]\nname = {}\npath = {}\ntest = false\nbench = false\n\n[dependencies]\nrsvz = {{ path = {}, default-features = false, features = [\"pvz-emulator\"] }}\nrsvz-pvz-emulator-backend = {{ path = {}, default-features = false, features = [\"host\"] }}\nuser_script = {{ package = {}, path = {}, default-features = {script_default_features}, features = [{script_features}] }}\n\n[workspace]\nresolver = \"3\"\nmembers = [\".\"]\n",
        toml_string(package_name),
        toml_string(bin_name),
        toml_string(target_path),
        toml_path_string(rsvz_path),
        toml_path_string(backend_path),
        toml_string(&script_package.name),
        toml_path_string(&script_package.manifest_dir),
    )
}

pub(crate) fn cargo_build_generated_runner(root: &Path, runner: &ScriptRunnerBuild) -> Result<PathBuf> {
    let manifest_path = &runner.manifest_path;
    let root_id = &runner.target.package_id;
    let target_name = &runner.target.name;

    let mut command = Command::new(cargo_bin());
    command
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--target-dir")
        .arg(root.join("target"))
        .arg("--message-format=json-render-diagnostics")
        .arg("--target")
        .arg(GENERATED_RUNNER_TARGET)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .current_dir(root);
    if runner.profile.is_release() {
        command.arg("--release");
    }
    runner.build_options.apply_to_cargo_command(&mut command);
    let mut child = command
        .spawn()
        .with_context(|| format!("执行 cargo build generated PE runner 失败: {}", manifest_path.display()))?;
    let stdout = child.stdout.take().context("cargo JSON stdout pipe 不可用")?;
    let mut artifacts = Vec::new();
    let mut parse_error = None;
    for message in Message::parse_stream(BufReader::new(stdout)) {
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                parse_error.get_or_insert_with(|| anyhow::Error::new(error).context("解析 Cargo JSON message 失败"));
                continue;
            }
        };
        match message {
            Message::CompilerArtifact(artifact)
                if &artifact.package_id == root_id
                    && &artifact.target.name == target_name
                    && artifact.target.kind.iter().any(|kind| kind == &TargetKind::Bin) =>
            {
                if let Some(executable) = artifact.executable {
                    artifacts.push(executable.as_std_path().to_path_buf());
                }
            }
            Message::CompilerMessage(message) => {
                if let Some(rendered) = message.message.rendered {
                    eprint!("{rendered}");
                }
            }
            Message::TextLine(line) => eprintln!("{line}"),
            _ => {}
        }
    }
    let status = child.wait().context("等待 generated PE runner build 失败")?;
    if let Some(error) = parse_error {
        return Err(error);
    }
    if !status.success() {
        bail!("cargo build generated PE runner 失败，exit code: {:?}", status.code());
    }
    artifacts.sort();
    artifacts.dedup();
    let artifact = match artifacts.as_slice() {
        [executable] => executable.clone(),
        [] => bail!("Cargo JSON 未报告 root PackageId `{root_id}` / bin target `{target_name}` 的 executable artifact"),
        _ => bail!(
            "Cargo JSON 为 root PackageId `{root_id}` / bin target `{target_name}` 报告了多个 executable artifact: {artifacts:?}"
        ),
    };
    Ok(artifact)
}

fn cargo_bin() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn ensure_manifest_only_generated_package(generated_dir: &Path) -> Result<()> {
    if generated_dir.join("src").try_exists()? {
        bail!("generated PE package 不得保留 src 目录：{}", generated_dir.display());
    }
    Ok(())
}

fn write_if_changed(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let contents = contents.as_ref();
    if fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    let sequence = WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("manifest");
    let temporary = path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("创建临时文件失败: {}", temporary.display()))?;
        file.write_all(contents)
            .with_context(|| format!("写入临时文件失败: {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("同步临时文件失败: {}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path).with_context(|| format!("原子替换文件失败: {}", path.display()))
    })();
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}

fn safe_normalized_name(name: &str, separator: char) -> String {
    let mut output = String::new();
    let mut separated = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if separated && !output.is_empty() {
                output.push(separator);
            }
            output.push(ch.to_ascii_lowercase());
            separated = false;
        } else {
            separated = true;
        }
    }
    if output.is_empty() { "script".to_owned() } else { output }
}

fn safe_path_name(name: &str) -> String {
    safe_normalized_name(name, '-')
}

fn safe_rust_ident_fragment(name: &str) -> String {
    safe_normalized_name(name, '_')
}

fn normalized_features(features: &[String]) -> Vec<String> {
    let mut features = features.to_vec();
    features.sort();
    features.dedup();
    features
}

fn normalized_identity_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    let mut normalized = path.replace('\\', "/");
    if normalized.as_bytes().get(1) == Some(&b':') {
        let drive = normalized[..1].to_ascii_lowercase();
        normalized.replace_range(0..1, &drive);
    }
    normalized
}

fn toml_path_string(path: &Path) -> String {
    let path = path.to_string_lossy();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    toml_string(&path.replace('\\', "/"))
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_node(features: &[&str]) -> cargo_metadata::Node {
        serde_json::from_value(serde_json::json!({
            "id": "path+file:///C:/repo#test@0.1.0",
            "deps": [],
            "dependencies": [],
            "features": features,
        }))
        .expect("metadata node")
    }

    #[test]
    fn final_graph_feature_gate_accepts_only_pe() {
        assert!(require_pe_backend_features(&metadata_node(&["pvz-emulator"])).is_ok());
        assert!(require_pe_backend_features(&metadata_node(&["pvz-emulator", "pvz-portable"])).is_err());
        assert!(require_pe_backend_features(&metadata_node(&["pvz-emulator", "pvz-1-0-0-1051"])).is_err());
        assert!(require_pe_backend_features(&metadata_node(&[])).is_err());
    }

    #[test]
    fn generated_runner_names_include_stable_disambiguator() {
        let first = ScriptPackageInfo {
            id: PackageId {
                repr: "path+file:///C:/scripts/first#user-script@0.1.0".to_owned(),
            },
            name: "user-script".to_owned(),
            manifest_path: PathBuf::from("C:/scripts/first/Cargo.toml"),
            manifest_dir: PathBuf::from("C:/scripts/first"),
        };
        let second = ScriptPackageInfo {
            id: PackageId {
                repr: "path+file:///C:/scripts/second#user-script@0.1.0".to_owned(),
            },
            name: "user-script".to_owned(),
            manifest_path: PathBuf::from("C:/scripts/second/Cargo.toml"),
            manifest_dir: PathBuf::from("C:/scripts/second"),
        };

        let first_names = script_runner_names("pe24", &first, &[], true);
        let second_names = script_runner_names("pe24", &second, &[], true);
        let hyphen_names = script_runner_names("a-b", &first, &[], true);
        let underscore_names = script_runner_names("a_b", &first, &[], true);
        let same_manifest_names = script_runner_names("pe24", &first, &[], true);
        let no_default_names = script_runner_names("pe24", &first, &[], false);
        let features = normalized_features(&["b".to_owned(), "a".to_owned()]);
        let reordered_features = normalized_features(&["a".to_owned(), "b".to_owned(), "a".to_owned()]);
        let feature_names = script_runner_names("pe24", &first, &features, true);
        let reordered_feature_names = script_runner_names("pe24", &first, &reordered_features, true);

        assert_ne!(first_names.bin_name, second_names.bin_name);
        assert_ne!(hyphen_names.bin_name, underscore_names.bin_name);
        assert_eq!(first_names.bin_name, same_manifest_names.bin_name);
        assert_ne!(first_names.bin_name, no_default_names.bin_name);
        assert_eq!(feature_names.bin_name, reordered_feature_names.bin_name);
        assert!(first_names.bin_name.starts_with("rsvz_pe_runner_pe24_"));
    }

    #[test]
    fn generated_runner_depends_on_user_package_rsvz_and_backend_directly() {
        let package = ScriptPackageInfo {
            id: PackageId {
                repr: "path+file:///C:/source/user-script#user-script@0.1.0".to_owned(),
            },
            name: "user-script".to_owned(),
            manifest_path: PathBuf::from("C:/source/user-script/Cargo.toml"),
            manifest_dir: PathBuf::from("C:/source/user-script"),
        };

        let runner_manifest = render_runner_manifest(
            "rsvz-pe-runner-pe24",
            "rsvz_pe_runner_pe24",
            "../../../../../crates/backends/pvz_emulator/final_entry/main.rs",
            Path::new("C:/repo/crates/scripting/api"),
            Path::new("C:/repo/crates/backends/pvz_emulator/backend"),
            &package,
            &["extra".to_owned()],
            false,
        );
        assert!(runner_manifest.contains("[[bin]]"));
        assert!(runner_manifest.contains("path = \"../../../../../crates/backends/pvz_emulator/final_entry/main.rs\""));
        assert!(runner_manifest.contains("test = false"));
        assert!(runner_manifest.contains("features = [\"host\"]"));
        assert!(runner_manifest.contains("features = [\"pvz-emulator\"]"));
        assert!(runner_manifest.contains("rsvz-pvz-emulator-backend = { path = "));
        assert!(runner_manifest.contains("user_script = { package = \"user-script\""));
        assert!(runner_manifest.contains("default-features = false, features = [\"extra\"]"));
    }

    #[test]
    fn pe_wire_config_round_trips_golden_json_and_rejects_unknown_or_stale_data() {
        let config = wire::PeRunConfig {
            schema_version: wire::SCHEMA_VERSION,
            run_id: "run-1".to_owned(),
            threads: 1,
            seed_mode: wire::SeedMode::Base { seed: 42 },
            limits: wire::RunLimits {
                max_wall_ms: Some(1000),
                max_sim_frames: Some(50),
                max_levels: None,
            },
            profile: true,
            profile_detail: false,
            performance_window_ms: None,
            output_path: Some(PathBuf::from("artifact.json")),
            raw_result_path: PathBuf::from("raw.json"),
        };
        let value = serde_json::to_value(&config).expect("serialize config");
        assert_eq!(
            value,
            serde_json::json!({
                "schema_version": 2,
                "run_id": "run-1",
                "threads": 1,
                "seed_mode": {"kind": "base", "seed": 42},
                "limits": {"max_wall_ms": 1000, "max_sim_frames": 50, "max_levels": null},
                "profile": true,
                "profile_detail": false,
                "output_path": "artifact.json",
                "raw_result_path": "raw.json"
            })
        );
        let decoded: wire::PeRunConfig = serde_json::from_value(value.clone()).expect("round trip");
        decoded.validate(Some("run-1")).expect("validate");
        assert!(decoded.validate(Some("stale")).is_err());
        let mut invalid = decoded.clone();
        invalid.threads = 0;
        assert!(invalid.validate(None).is_err());
        invalid = decoded.clone();
        invalid.raw_result_path = PathBuf::new();
        assert!(invalid.validate(None).is_err());
        invalid = decoded;
        invalid.output_path = Some(invalid.raw_result_path.clone());
        assert!(invalid.validate(None).is_err());

        let mut unknown_root = value;
        unknown_root["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<wire::PeRunConfig>(unknown_root).is_err());
        let mut wrong_version = serde_json::to_value(config).expect("serialize config");
        wrong_version["schema_version"] = serde_json::json!(3);
        let wrong_version: wire::PeRunConfig =
            serde_json::from_value(wrong_version).expect("version is semantic validation");
        assert!(wrong_version.validate(None).is_err());
    }

    #[test]
    fn pe_wire_result_round_trips_its_direct_root_and_strict_terminal() {
        let result = wire::PeRunResult {
            schema_version: wire::SCHEMA_VERSION,
            run_id: "run-1".to_owned(),
            terminal: wire::Terminal::Failure {
                message: "boom".to_owned(),
            },
            parent_wall_ns: 12,
            workers: Vec::new(),
        };
        let value = serde_json::to_value(&result).expect("serialize result");
        assert_eq!(
            value,
            serde_json::json!({
                "schema_version": 2,
                "run_id": "run-1",
                "terminal": {"status": "failure", "message": "boom"},
                "parent_wall_ns": 12,
                "workers": []
            })
        );
        let decoded: wire::PeRunResult = serde_json::from_value(value).expect("round trip");
        decoded.validate(Some("run-1")).expect("validate");
        let mut invalid_success = decoded;
        invalid_success.terminal = wire::Terminal::Success {};
        assert!(invalid_success.validate(Some("run-1")).is_err());
    }

    #[test]
    fn pe_success_result_must_cover_every_configured_worker() {
        let config = wire::PeRunConfig {
            schema_version: wire::SCHEMA_VERSION,
            run_id: "run-coverage".to_owned(),
            threads: 2,
            seed_mode: wire::SeedMode::Fixed { seed: 7 },
            limits: wire::RunLimits::default(),
            profile: false,
            profile_detail: false,
            performance_window_ms: None,
            output_path: None,
            raw_result_path: PathBuf::from("raw.json"),
        };
        let worker = |worker_index| wire::WorkerResult {
            worker_index,
            seed: 7,
            stop_reason: wire::StopReason::Script {},
            wall_ns: 0,
            frames: 0,
            completed_levels: 0,
            partial_frames: 0,
            profile: None,
            performance_window: None,
        };
        let mut result = wire::PeRunResult {
            schema_version: wire::SCHEMA_VERSION,
            run_id: config.run_id.clone(),
            terminal: wire::Terminal::Success {},
            parent_wall_ns: 0,
            workers: vec![worker(0), worker(1)],
        };
        result.validate_for(&config).expect("complete coverage");
        result.workers[1].worker_index = 2;
        assert!(result.validate_for(&config).is_err());
    }
}
