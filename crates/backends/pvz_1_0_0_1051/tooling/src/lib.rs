//! Build recipe helpers for the PvZ 1.0.0.1051 backend family.

pub mod inject_cli;
#[path = "../../../../tools/live_process.rs"]
mod live_process;
mod owned_game;
#[path = "../../injected/src/host/wire.rs"]
pub(crate) mod wire;

use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use cargo_metadata::{Message, Metadata, MetadataCommand, PackageId, TargetKind};

pub const DEFAULT_TARGET: &str = "i686-pc-windows-msvc";
pub const BACKEND_PACKAGE: &str = "rsvz-pvz1051-injected";
pub const INJECTOR_PACKAGE: &str = "injector";
pub const INJECTOR_EXE_NAME: &str = "injector.exe";
pub const BACKEND_CRATE_RELATIVE: &str = "crates/backends/pvz_1_0_0_1051/injected";
pub const RSVZ_CRATE_RELATIVE: &str = "crates/scripting/api";
pub const FINAL_ENTRY_RELATIVE: &str = "crates/backends/pvz_1_0_0_1051/final_entry/lib.rs";
#[cfg(test)]
const FINAL_ENTRY_SOURCE: &str = include_str!("../../final_entry/lib.rs");
#[cfg(test)]
const NATIVE_DLL_MAIN_SOURCE: &str = include_str!("../../injected/src/host/lifecycle.rs");
static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn ensure_supported_target(target: &str) -> Result<()> {
    if target != DEFAULT_TARGET {
        bail!("PvZ 1.0.0.1051 only supports Cargo target {DEFAULT_TARGET}, got {target}");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    #[must_use]
    pub const fn from_debug_flag(debug: bool) -> Self {
        if debug { Self::Debug } else { Self::Release }
    }

    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }

    const fn is_release(self) -> bool {
        matches!(self, Self::Release)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRunnerOptions<'a> {
    pub current_dir: &'a Path,
    pub script_crate: &'a Path,
    pub script_name: &'a str,
    pub script_package: Option<&'a str>,
    pub script_features: &'a [String],
    pub script_default_features: bool,
    pub target: &'a str,
    pub profile: BuildProfile,
    pub fast_forward_profiler: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRunnerBuild {
    pub script_name: String,
    pub manifest_path: PathBuf,
    profile: BuildProfile,
    cargo_target: String,
    target: ValidatedTarget,
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
    lib_name: String,
    generated_dir_name: String,
}

pub fn prepare_script_runner(root: &Path, options: ScriptRunnerOptions<'_>) -> Result<ScriptRunnerBuild> {
    ensure_supported_target(options.target)?;
    let script_manifest = resolve_script_manifest_path(options.current_dir, options.script_crate)?;
    let package = resolve_script_package(&script_manifest, options.script_package)?;
    let script_features = normalized_features(options.script_features);
    let names = script_runner_names(&options, &package, &script_features);
    let generated_dir = root
        .join("target")
        .join("rsvz")
        .join("generated")
        .join("1051")
        .join(&names.generated_dir_name);
    fs::create_dir_all(&generated_dir)
        .with_context(|| format!("创建 generated runner crate 目录失败: {}", generated_dir.display()))?;
    ensure_manifest_only_generated_package(&generated_dir)?;
    let fixed_entry = fs::canonicalize(root.join(FINAL_ENTRY_RELATIVE)).with_context(|| {
        format!(
            "规范化 fixed 1051 entry 失败: {}",
            root.join(FINAL_ENTRY_RELATIVE).display()
        )
    })?;
    let target_path = normalized_identity_path(&fixed_entry);

    let backend_path = root.join(BACKEND_CRATE_RELATIVE);
    let rsvz_path = root.join(RSVZ_CRATE_RELATIVE);
    write_if_changed(
        &generated_dir.join("Cargo.toml"),
        &render_runner_manifest(
            &names,
            &target_path,
            &backend_path,
            &rsvz_path,
            &package,
            &script_features,
            &options,
        ),
    )?;
    let target = validate_final_graph(
        root,
        &generated_dir.join("Cargo.toml"),
        options.target,
        &names.lib_name,
        &package.id,
        options.fast_forward_profiler,
    )?;

    Ok(ScriptRunnerBuild {
        script_name: options.script_name.to_owned(),
        manifest_path: generated_dir.join("Cargo.toml"),
        profile: options.profile,
        cargo_target: options.target.to_owned(),
        target,
    })
}

fn script_runner_names(
    options: &ScriptRunnerOptions<'_>, package: &ScriptPackageInfo, script_features: &[String],
) -> ScriptRunnerNames {
    let script_name = options.script_name;
    let safe_name = safe_path_name(script_name);
    let ident = safe_rust_ident_fragment(script_name);
    let package_id = package.id.to_string();
    let package_path = normalized_identity_path(&package.manifest_path);
    let script_features = normalized_features(script_features);
    let mut identity = vec![
        "rsvz-generated-target-v3".to_owned(),
        script_name.to_owned(),
        package_id,
        package_path,
        "script-features".to_owned(),
        script_features.len().to_string(),
    ];
    identity.extend(script_features);
    identity.extend([
        "default-features".to_owned(),
        options.script_default_features.to_string(),
    ]);
    let identity = identity.iter().map(String::as_str).collect::<Vec<_>>();
    let hash = stable_hash_hex(&identity);
    let suffix = &hash;
    ScriptRunnerNames {
        package_name: format!("rsvz-1051-runner-{safe_name}-{suffix}"),
        lib_name: format!("rsvz_1051_runner_{ident}_{suffix}"),
        generated_dir_name: format!("{safe_name}-{suffix}"),
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
        let Some(package) = matches.next() else {
            bail!(
                "script package `{name}` 不在 workspace 中: {}",
                script_manifest.display()
            );
        };
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
            manifest_path.display()
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
                "读取 generated 1051 runner 最终 target metadata 失败: {}",
                manifest_path.display()
            )
        })
}

fn validate_final_graph(
    root_dir: &Path, manifest_path: &Path, target: &str, expected_target_name: &str, expected_user: &PackageId,
    fast_forward_profiler: bool,
) -> Result<ValidatedTarget> {
    let metadata = metadata_for_target(manifest_path, target)?;
    let root = metadata
        .root_package()
        .context("generated 1051 runner metadata 缺少 root package")?;
    let resolve = metadata
        .resolve
        .as_ref()
        .context("generated 1051 runner metadata 缺少 resolve graph")?;
    let root_node = resolve
        .nodes
        .iter()
        .find(|node| node.id == root.id)
        .context("generated 1051 runner root package 不在 resolve graph 中")?;
    let targets = root
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == &TargetKind::CDyLib))
        .collect::<Vec<_>>();
    let [root_target] = targets.as_slice() else {
        bail!(
            "generated 1051 runner 必须恰好有一个 cdylib target，实际为 {}",
            targets.len()
        );
    };
    if root_target.name != expected_target_name {
        bail!(
            "generated 1051 target name 不一致：期望 {expected_target_name}，实际 {}",
            root_target.name
        );
    }
    let expected_source = fs::canonicalize(root_dir.join(FINAL_ENTRY_RELATIVE)).with_context(|| {
        format!(
            "规范化 fixed 1051 entry 失败: {}",
            root_dir.join(FINAL_ENTRY_RELATIVE).display()
        )
    })?;
    let actual_source = fs::canonicalize(root_target.src_path.as_std_path())
        .with_context(|| format!("规范化 generated 1051 target source 失败: {}", root_target.src_path))?;
    if actual_source != expected_source {
        bail!(
            "generated 1051 target source 不一致：期望 {}，实际 {}",
            expected_source.display(),
            actual_source.display()
        );
    }

    let user = root_node
        .deps
        .iter()
        .find(|dep| dep.name == "user_script")
        .context("generated 1051 runner 缺少重命名依赖 `user_script`")?;
    if &user.pkg != expected_user {
        bail!(
            "generated 1051 runner 的 user_script PackageId 发生变化：期望 {expected_user}，实际 {}",
            user.pkg
        );
    }
    let rsvz = root_node
        .deps
        .iter()
        .find(|dependency| dependency.name == "rsvz")
        .context("generated 1051 runner 缺少依赖 `rsvz`")?;
    let rsvz_node = node_by_id(resolve, &rsvz.pkg)?;
    require_feature(rsvz_node, "pvz-1-0-0-1051")?;
    forbid_feature(rsvz_node, "pvz-emulator")?;
    forbid_feature(rsvz_node, "pvz-portable")?;
    let backend = root_node
        .deps
        .iter()
        .find(|dependency| dependency.name == "rsvz_backend_1051")
        .context("generated 1051 runner 缺少依赖 `rsvz_backend_1051`")?;
    let backend_node = node_by_id(resolve, &backend.pkg)?;
    if fast_forward_profiler {
        require_feature(backend_node, "fast-forward-profiler")?;
    } else {
        forbid_feature(backend_node, "fast-forward-profiler")?;
    }

    for forbidden in [
        "rsvz-backend-1051-tooling",
        "rsvz-pvz-emulator-tooling",
        "rsvz-pvz-portable-tooling",
        "rsvz-pvz-portable-backend",
        "pvzp-rs",
    ] {
        if metadata.packages.iter().any(|package| package.name == forbidden) {
            bail!("generated 1051 final graph 不得包含 tooling crate `{forbidden}`");
        }
    }
    if metadata
        .packages
        .iter()
        .any(|package| package.name == "rsvz-pvz-emulator-backend")
    {
        bail!("generated 1051 final graph 不得包含 PE backend");
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

fn render_runner_manifest(
    names: &ScriptRunnerNames, target_path: &str, backend_path: &Path, rsvz_path: &Path,
    script_package: &ScriptPackageInfo, script_features: &[String], options: &ScriptRunnerOptions<'_>,
) -> String {
    let script_default_features = options.script_default_features;
    let backend_features = if options.fast_forward_profiler {
        "[\"fast-forward-profiler\"]"
    } else {
        "[]"
    };
    let script_features = script_features
        .iter()
        .map(|feature| toml_string(feature))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.95\"\npublish = false\nbuild = false\nautolib = false\nautobins = false\nautoexamples = false\nautotests = false\nautobenches = false\n\n[lib]\nname = {}\npath = {}\ncrate-type = [\"cdylib\"]\ntest = false\nbench = false\ndoctest = false\n\n[dependencies]\nrsvz = {{ path = {}, default-features = false, features = [\"pvz-1-0-0-1051\"] }}\nrsvz_backend_1051 = {{ package = {}, path = {}, default-features = false, features = {backend_features} }}\nuser_script = {{ package = {}, path = {}, default-features = {script_default_features}, features = [{script_features}] }}\n\n[workspace]\nresolver = \"3\"\nmembers = [\".\"]\n",
        toml_string(&names.package_name),
        toml_string(&names.lib_name),
        toml_string(target_path),
        toml_path_string(rsvz_path),
        toml_string(BACKEND_PACKAGE),
        toml_path_string(backend_path),
        toml_string(&script_package.name),
        toml_path_string(&script_package.manifest_dir),
    )
}

fn ensure_manifest_only_generated_package(generated_dir: &Path) -> Result<()> {
    if generated_dir.join("src").try_exists()? {
        bail!("generated 1051 package 不得保留 src 目录：{}", generated_dir.display());
    }
    Ok(())
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
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
        file.write_all(contents.as_bytes())
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

fn safe_normalized_name(name: &str, keep: fn(char) -> bool, sep: char) -> String {
    let mut output = String::with_capacity(name.len());
    let mut last_was_sep = false;
    for ch in name.chars() {
        let ch = if ch.is_ascii_alphanumeric() || keep(ch) {
            ch.to_ascii_lowercase()
        } else {
            sep
        };
        if ch == sep {
            if last_was_sep {
                continue;
            }
            last_was_sep = true;
        } else {
            last_was_sep = false;
        }
        output.push(ch);
    }
    let output = output.trim_matches(|ch| ch == sep || ch == '_').to_owned();
    if output.is_empty() { "script".to_owned() } else { output }
}

fn safe_path_name(name: &str) -> String {
    safe_normalized_name(name, |ch| ch == '-' || ch == '_', '-')
}

fn safe_rust_ident_fragment(name: &str) -> String {
    safe_normalized_name(name, |_| false, '_')
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
        normalized.replace_range(0..1, &normalized[..1].to_ascii_lowercase());
    }
    normalized
}

fn stable_hash_hex(parts: &[&str]) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn toml_path_string(path: &Path) -> String {
    let path = path.to_string_lossy();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    toml_string(&path.replace('\\', "/"))
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

pub fn cargo_build_injector(root: &Path, target: &str, profile: BuildProfile) -> Result<()> {
    ensure_supported_target(target)?;
    let mut command = Command::new(cargo_bin());
    command
        .arg("build")
        .arg("-p")
        .arg(INJECTOR_PACKAGE)
        .arg("--target")
        .arg(target)
        .current_dir(root);
    if profile.is_release() {
        command.arg("--release");
    }
    let status = command.status().context("执行 cargo build -p injector 失败")?;
    if !status.success() {
        bail!("cargo build -p injector 失败，exit code: {:?}", status.code());
    }
    Ok(())
}

pub(crate) fn cargo_build_generated_runner(root: &Path, runner: &ScriptRunnerBuild) -> Result<PathBuf> {
    ensure_supported_target(&runner.cargo_target)?;
    let manifest_path = &runner.manifest_path;
    let root_id = &runner.target.package_id;
    let target_name = &runner.target.name;

    let mut command = Command::new(cargo_bin());
    command
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--locked")
        .arg("--target")
        .arg(&runner.cargo_target)
        .arg("--target-dir")
        .arg(root.join("target"))
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .current_dir(root);
    if runner.profile.is_release() {
        command.arg("--release");
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("执行 cargo build generated runner 失败: {}", manifest_path.display()))?;
    let stdout = child.stdout.take().context("cargo JSON stdout pipe 不可用")?;
    let mut artifacts = Vec::new();
    for message in Message::parse_stream(BufReader::new(stdout)) {
        match message.context("解析 Cargo JSON message 失败")? {
            Message::CompilerArtifact(artifact)
                if &artifact.package_id == root_id
                    && &artifact.target.name == target_name
                    && artifact.target.kind.iter().any(|kind| kind == &TargetKind::CDyLib) =>
            {
                artifacts.extend(
                    artifact
                        .filenames
                        .iter()
                        .filter(|path| {
                            path.extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
                        })
                        .map(|path| path.as_std_path().to_path_buf()),
                );
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
    let status = child.wait().context("等待 generated 1051 runner build 失败")?;
    if !status.success() {
        bail!("cargo build generated runner 失败，exit code: {:?}", status.code());
    }
    artifacts.sort();
    artifacts.dedup();
    match artifacts.as_slice() {
        [dll] => Ok(dll.clone()),
        [] => bail!("Cargo JSON 未报告 root PackageId `{root_id}` / cdylib target `{target_name}` 的 DLL artifact"),
        _ => bail!(
            "Cargo JSON 为 root PackageId `{root_id}` / cdylib target `{target_name}` 报告了多个 DLL artifact: {artifacts:?}"
        ),
    }
}

fn cargo_bin() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

#[must_use]
pub fn injector_command_args(dll: &Path, pid: Option<u32>, run_config_json: Option<&str>) -> Vec<OsString> {
    let mut args = vec![OsString::from("--dll"), dll.as_os_str().to_owned()];
    if let Some(pid) = pid {
        args.push(OsString::from("--pid"));
        args.push(OsString::from(pid.to_string()));
    }
    if let Some(json) = run_config_json {
        args.push(OsString::from("--rsvz-run-config-json"));
        args.push(OsString::from(json));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_1051_target() {
        assert!(ensure_supported_target(DEFAULT_TARGET).is_ok());
        assert!(ensure_supported_target("x86_64-pc-windows-msvc").is_err());
    }

    fn script_package() -> ScriptPackageInfo {
        ScriptPackageInfo {
            id: PackageId {
                repr: "path+file:///C:/scripts/user#user@0.1.0".to_owned(),
            },
            name: "user".to_owned(),
            manifest_path: PathBuf::from("C:/scripts/user/Cargo.toml"),
            manifest_dir: PathBuf::from("C:/scripts/user"),
        }
    }

    #[test]
    fn runner_identity_ignores_build_profile_and_profiler() {
        let package = script_package();
        let features = vec!["extra".to_owned()];
        let mut options = ScriptRunnerOptions {
            current_dir: Path::new("C:/repo"),
            script_crate: Path::new("C:/scripts/user/Cargo.toml"),
            script_name: "user",
            script_package: None,
            script_features: &features,
            script_default_features: true,
            target: DEFAULT_TARGET,
            profile: BuildProfile::Release,
            fast_forward_profiler: false,
        };
        let release = script_runner_names(&options, &package, &features);
        options.profile = BuildProfile::Debug;
        options.fast_forward_profiler = true;
        assert_eq!(release, script_runner_names(&options, &package, &features));
        options.script_default_features = false;
        assert_ne!(release, script_runner_names(&options, &package, &features));
    }

    #[test]
    fn generated_manifest_has_the_fixed_chain_and_no_root_serialization() {
        let names = ScriptRunnerNames {
            package_name: "runner".to_owned(),
            lib_name: "runner".to_owned(),
            generated_dir_name: "runner".to_owned(),
        };
        let options = ScriptRunnerOptions {
            current_dir: Path::new("C:/repo"),
            script_crate: Path::new("C:/scripts/user"),
            script_name: "script",
            script_package: None,
            script_features: &[],
            script_default_features: true,
            target: DEFAULT_TARGET,
            profile: BuildProfile::Debug,
            fast_forward_profiler: false,
        };
        let manifest = render_runner_manifest(
            &names,
            "../../final_entry/lib.rs",
            Path::new("C:/repo/injected"),
            Path::new("C:/repo/rsvz"),
            &script_package(),
            &[],
            &options,
        );
        assert!(manifest.contains("default-features = false, features = []"));
        assert!(manifest.contains("features = [\"pvz-1-0-0-1051\"]"));
        assert!(!manifest.contains("\nserde ="));
        assert!(!manifest.contains("\nserde_json ="));
    }

    #[test]
    fn final_root_is_only_two_typed_abi_forwards() {
        assert_eq!(FINAL_ENTRY_SOURCE.matches("#[unsafe(no_mangle)]").count(), 2);
        assert!(FINAL_ENTRY_SOURCE.contains("host::initialize(context, user_script::__rsvz_dispatch)"));
        assert!(FINAL_ENTRY_SOURCE.contains("host::request_unload(context)"));
        for forbidden in ["Bench", "Measure", "Opening", "match ", "thread_local!"] {
            assert!(!FINAL_ENTRY_SOURCE.contains(forbidden));
        }
    }

    #[test]
    fn rust_dll_main_is_record_only_and_not_in_the_final_root() {
        let body = NATIVE_DLL_MAIN_SOURCE
            .split_once("fn record_dll_main")
            .expect("Rust DllMain")
            .1;
        assert!(body.contains("record_loader_event(module, reason)"));
        for forbidden in ["user_script", "__rsvz_dispatch", "initialize(", "serde_json"] {
            assert!(!body.contains(forbidden), "DllMain contains {forbidden}");
        }
        assert!(!FINAL_ENTRY_SOURCE.contains("DllMain"));
    }

    #[test]
    fn host_wire_round_trips_without_business_modes() {
        let config = wire::RunConfig {
            schema_version: wire::SCHEMA_VERSION,
            run_id: "run-1".to_owned(),
            output_path: Some(PathBuf::from("artifact.json")),
            control_result_path: PathBuf::from("control.json"),
        };
        config.validate(Some("run-1")).expect("valid");
        let json = serde_json::to_string(&config).expect("serialize");
        for forbidden in ["bench", "measure", "finish_policy", "entry_scene", "auto_enter"] {
            assert!(!json.contains(forbidden));
        }
        let decoded: wire::RunConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, config);
    }

    #[test]
    fn injector_receives_only_the_dll_and_opaque_config() {
        let args = injector_command_args(Path::new("runner.dll"), Some(42), Some("{\"schema_version\":2}"));
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "--dll");
        assert_eq!(args[2], "--pid");
        assert_eq!(args[4], "--rsvz-run-config-json");
    }
}
