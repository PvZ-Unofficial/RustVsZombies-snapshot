use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use rsvz_backend_1051_tooling as tooling_1051;
use rsvz_pvz_emulator_tooling as pe_tooling;
use rsvz_pvz_portable_tooling as portable_tooling;
use serde::Serialize;
use toml::{Table, Value};

const IDENTITY_VERSION: &str = "rsvz-bare-script-v1";
const PREPARATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Backend {
    #[value(name = "pvz-emulator")]
    PvzEmulator,
    #[value(name = "pvz-1-0-0-1051")]
    Pvz1_0_0_1051,
    #[value(name = "pvz-portable")]
    PvzPortable,
}

impl Backend {
    pub const fn name(self) -> &'static str {
        match self {
            Self::PvzEmulator => "pvz-emulator",
            Self::Pvz1_0_0_1051 => "pvz-1-0-0-1051",
            Self::PvzPortable => "pvz-portable",
        }
    }

    const fn feature(self) -> &'static str {
        self.name()
    }

    const fn target(self) -> &'static str {
        match self {
            Self::PvzEmulator => pe_tooling::GENERATED_RUNNER_TARGET,
            Self::Pvz1_0_0_1051 => tooling_1051::DEFAULT_TARGET,
            Self::PvzPortable => portable_tooling::MSVC_TARGET,
        }
    }

    const fn command(self) -> &'static str {
        match self {
            Self::PvzEmulator => "run-pe",
            Self::Pvz1_0_0_1051 => "inject-1051",
            Self::PvzPortable => "run-portable",
        }
    }
}

#[derive(Args, Debug)]
pub struct PrepareScriptArgs {
    #[arg(value_name = "SCRIPT")]
    script: PathBuf,
    #[arg(long, value_enum)]
    backend: Backend,
    #[arg(long, required = true)]
    json: bool,
    /// RustVsZombies workspace root. Defaults to searching from the current directory and executable.
    #[arg(long, value_name = "PATH")]
    workspace_root: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedScript {
    schema_version: u32,
    backend: &'static str,
    script_path: PathBuf,
    config_path: Option<PathBuf>,
    script_manifest: PathBuf,
    analysis_manifest: PathBuf,
    package_name: String,
    lib_name: String,
    cargo: PreparedCargo,
    execution: PreparedExecution,
}

pub struct PreparedCommandScript {
    script_manifest: PathBuf,
    script_name: String,
}

impl PreparedCommandScript {
    pub fn script_manifest(&self) -> &Path {
        &self.script_manifest
    }

    pub fn script_name(&self) -> &str {
        &self.script_name
    }
}

#[derive(Debug, Serialize)]
struct PreparedCargo {
    target: &'static str,
}

#[derive(Debug, Serialize)]
struct PreparedExecution {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
}

pub fn run(args: PrepareScriptArgs) -> Result<()> {
    debug_assert!(args.json);
    let current_dir = env::current_dir().context("读取当前目录失败")?;
    let root = resolve_workspace_root(&current_dir, args.workspace_root.as_deref())?;
    let program = env::current_exe().context("读取 rsvz CLI 路径失败")?;
    let prepared = prepare(&root, &current_dir, &args.script, args.backend, None, program, None)?;
    println!(
        "{}",
        serde_json::to_string(&prepared).context("序列化 prepared script JSON 失败")?
    );
    Ok(())
}

pub fn prepare_for_command(
    root: &Path, current_dir: &Path, script: &Path, backend: Backend, script_name: Option<&str>,
) -> Result<PreparedCommandScript> {
    let hidden = prepare_hidden(root, current_dir, script, backend, script_name)?;
    Ok(PreparedCommandScript {
        script_manifest: hidden.script_manifest,
        script_name: hidden.script_name,
    })
}

struct HiddenScript {
    source: PathBuf,
    config_path: Option<PathBuf>,
    script_manifest: PathBuf,
    package_name: String,
    lib_name: String,
    script_name: String,
}

fn prepare(
    root: &Path, current_dir: &Path, source: &Path, backend: Backend, script_name: Option<&str>, program: PathBuf,
    portable_build: Option<(portable_tooling::PortableToolchain, portable_tooling::BuildProfile)>,
) -> Result<PreparedScript> {
    let HiddenScript {
        source,
        config_path,
        script_manifest,
        package_name,
        lib_name,
        script_name,
    } = prepare_hidden(root, current_dir, source, backend, script_name)?;

    let analysis_manifest = match backend {
        Backend::PvzEmulator => {
            let build_options = pe_tooling::PeBuildOptions::default();
            pe_tooling::prepare_script_runner(
                root,
                pe_tooling::ScriptRunnerOptions {
                    current_dir: root,
                    script_crate: &script_manifest,
                    script_name: Some(&script_name),
                    script_package: None,
                    script_features: &[],
                    script_default_features: true,
                    profile: pe_tooling::BuildProfile::Debug,
                    build_options: &build_options,
                },
            )?
            .manifest_path
        }
        Backend::Pvz1_0_0_1051 => {
            tooling_1051::prepare_script_runner(
                root,
                tooling_1051::ScriptRunnerOptions {
                    current_dir: root,
                    script_crate: &script_manifest,
                    script_name: &script_name,
                    script_package: None,
                    script_features: &[],
                    script_default_features: true,
                    target: tooling_1051::DEFAULT_TARGET,
                    profile: tooling_1051::BuildProfile::Release,
                    fast_forward_profiler: false,
                },
            )?
            .manifest_path
        }
        Backend::PvzPortable => {
            let (toolchain, profile) = portable_build.unwrap_or((
                portable_tooling::PortableToolchain::Msvc,
                portable_tooling::BuildProfile::Debug,
            ));
            portable_tooling::prepare_plugin(
                root,
                root,
                &script_manifest,
                Some(&script_name),
                None,
                &[],
                toolchain,
                profile,
            )?
            .manifest_path
        }
    };

    let root = canonical_user_path(root).with_context(|| format!("规范化 workspace root 失败: {}", root.display()))?;
    let program = fs::canonicalize(&program).unwrap_or(program);
    for (path, kind) in [
        (&root, "workspace root"),
        (&script_manifest, "hidden manifest"),
        (&analysis_manifest, "analysis manifest"),
        (&program, "CLI"),
    ] {
        ensure_unicode(path, kind)?;
    }
    let args = vec![
        backend.command().to_owned(),
        "--script-crate".to_owned(),
        path_text(&script_manifest, "hidden manifest")?.to_owned(),
        "--script-name".to_owned(),
        script_name,
    ];

    Ok(PreparedScript {
        schema_version: 1,
        backend: backend.name(),
        script_path: source,
        config_path,
        script_manifest,
        analysis_manifest,
        package_name,
        lib_name,
        cargo: PreparedCargo {
            target: backend.target(),
        },
        execution: PreparedExecution {
            program,
            args,
            cwd: root,
        },
    })
}

fn prepare_hidden(
    root: &Path, current_dir: &Path, source: &Path, backend: Backend, script_name: Option<&str>,
) -> Result<HiddenScript> {
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        current_dir.join(source)
    };
    let source = fs::canonicalize(&source).with_context(|| format!("规范化裸脚本路径失败: {}", source.display()))?;
    if !source.is_file() {
        bail!("裸脚本不是文件: {}", source.display());
    }
    ensure_unicode(&source, "裸脚本路径")?;
    if source.extension().and_then(|ext| ext.to_str()) != Some("rs") {
        bail!("裸脚本必须是 .rs 文件: {}", source.display());
    }

    let identity = stable_identity(&source)?;
    let output_dir = root
        .join("target")
        .join("rsvz")
        .join("scripts")
        .join(&identity)
        .join(backend.name());
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("创建 hidden script crate 目录失败: {}", output_dir.display()))?;
    let _lock = PreparationLock::acquire(&output_dir)?;

    let config_path = find_nearest_config(&source)?;
    if let Some(path) = config_path.as_deref() {
        ensure_unicode(path, "rsvz.toml 路径")?;
    }
    let dependencies = load_dependencies(config_path.as_deref(), backend)?;
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("裸脚本文件名不是有效 Unicode")?;
    let script_name = script_name.unwrap_or(stem).to_owned();
    let safe_stem = safe_name(stem, '-');
    let package_name = format!("rsvz-script-{safe_stem}-{identity}");
    let lib_name = format!("rsvz_script_{}_{}", safe_name(stem, '_'), identity);
    let script_manifest = output_dir.join("Cargo.toml");
    let manifest = hidden_manifest(root, &source, backend, &package_name, &lib_name, dependencies)?;
    write_if_changed(&script_manifest, &manifest)?;
    Ok(HiddenScript {
        source,
        config_path,
        script_manifest,
        package_name,
        lib_name,
        script_name,
    })
}

fn hidden_manifest(
    root: &Path, source: &Path, backend: Backend, package_name: &str, lib_name: &str, mut dependencies: Table,
) -> Result<String> {
    let rsvz_path = fs::canonicalize(root.join("crates/scripting/api")).context("规范化顶层 rsvz crate 路径失败")?;
    let mut rsvz = Table::new();
    rsvz.insert(
        "path".to_owned(),
        Value::String(path_text(&rsvz_path, "rsvz crate")?.to_owned()),
    );
    rsvz.insert("default-features".to_owned(), Value::Boolean(false));
    rsvz.insert(
        "features".to_owned(),
        Value::Array(vec![Value::String(backend.feature().to_owned())]),
    );
    dependencies.insert("rsvz".to_owned(), Value::Table(rsvz));

    let mut package = Table::new();
    package.insert("name".to_owned(), Value::String(package_name.to_owned()));
    package.insert("version".to_owned(), Value::String("0.0.0".to_owned()));
    package.insert("edition".to_owned(), Value::String("2024".to_owned()));
    package.insert("publish".to_owned(), Value::Boolean(false));
    for key in ["autobins", "autoexamples", "autotests", "autobenches"] {
        package.insert(key.to_owned(), Value::Boolean(false));
    }

    let mut library = Table::new();
    library.insert("name".to_owned(), Value::String(lib_name.to_owned()));
    library.insert(
        "path".to_owned(),
        Value::String(path_text(source, "裸脚本路径")?.to_owned()),
    );

    let mut workspace = Table::new();
    workspace.insert("resolver".to_owned(), Value::String("3".to_owned()));
    workspace.insert("members".to_owned(), Value::Array(vec![Value::String(".".to_owned())]));

    let mut manifest = Table::new();
    manifest.insert("package".to_owned(), Value::Table(package));
    manifest.insert("lib".to_owned(), Value::Table(library));
    manifest.insert("dependencies".to_owned(), Value::Table(dependencies));
    manifest.insert("workspace".to_owned(), Value::Table(workspace));
    toml::to_string(&manifest).context("序列化 hidden script manifest 失败")
}

fn load_dependencies(config_path: Option<&Path>, backend: Backend) -> Result<Table> {
    let Some(config_path) = config_path else {
        return Ok(Table::new());
    };
    let text = fs::read_to_string(config_path).with_context(|| format!("读取配置失败: {}", config_path.display()))?;
    let config: Table = toml::from_str(&text).with_context(|| format!("解析配置失败: {}", config_path.display()))?;
    for key in config.keys() {
        if key != "dependencies" && key != "backend" {
            bail!("{}: 不支持顶层字段 `{key}`", config_path.display());
        }
    }

    let common = dependency_table(config.get("dependencies"), config_path, "dependencies")?;
    let backend_root = optional_table(config.get("backend"), config_path, "backend")?;
    for key in backend_root.keys() {
        if key != Backend::PvzEmulator.name()
            && key != Backend::Pvz1_0_0_1051.name()
            && key != Backend::PvzPortable.name()
        {
            bail!("{}: 未知 backend `{key}`", config_path.display());
        }
    }
    let section_name = format!("backend.{}", backend.name());
    let section = optional_table(backend_root.get(backend.name()), config_path, &section_name)?;
    for key in section.keys() {
        if key != "dependencies" {
            bail!("{}: 不支持字段 `{section_name}.{key}`", config_path.display());
        }
    }
    let selected = dependency_table(
        section.get("dependencies"),
        config_path,
        &format!("{section_name}.dependencies"),
    )?;
    for key in common.keys() {
        if selected.contains_key(key) {
            bail!(
                "{}: dependency `{key}` 同时出现在 dependencies 与 {section_name}.dependencies",
                config_path.display()
            );
        }
    }

    let base = config_path.parent().context("rsvz.toml 没有父目录")?;
    let mut dependencies = BTreeMap::new();
    for (key, value) in common.into_iter().chain(selected) {
        validate_dependency_name(&key, config_path)?;
        dependencies.insert(key.clone(), rewrite_dependency(value, base, config_path, &key)?);
    }
    Ok(dependencies.into_iter().collect())
}

fn dependency_table(value: Option<&Value>, path: &Path, field: &str) -> Result<Table> {
    optional_table(value, path, field).cloned()
}

fn optional_table<'a>(value: Option<&'a Value>, path: &Path, field: &str) -> Result<&'a Table> {
    static EMPTY: std::sync::LazyLock<Table> = std::sync::LazyLock::new(Table::new);
    match value {
        None => Ok(&EMPTY),
        Some(Value::Table(table)) => Ok(table),
        Some(_) => bail!("{}: `{field}` 必须是 table", path.display()),
    }
}

fn validate_dependency_name(name: &str, path: &Path) -> Result<()> {
    if [
        "rsvz",
        "user_script",
        "pvz-emulator",
        "pvz-1-0-0-1051",
        "pvz-portable",
        "rsvz-pvz-emulator-backend",
        "rsvz-pvz1051-injected",
        "rsvz-pvz-portable-backend",
    ]
    .contains(&name)
    {
        bail!("{}: dependency `{name}` 是保留名称", path.display());
    }
    Ok(())
}

fn rewrite_dependency(mut value: Value, base: &Path, config: &Path, name: &str) -> Result<Value> {
    match &mut value {
        Value::String(_) => {}
        Value::Table(table) => {
            if let Some(path_value) = table.get_mut("path") {
                let raw = path_value
                    .as_str()
                    .with_context(|| format!("{}: dependencies.{name}.path 必须是字符串", config.display()))?;
                let dependency_path = Path::new(raw);
                let dependency_path = if dependency_path.is_absolute() {
                    dependency_path.to_path_buf()
                } else {
                    base.join(dependency_path)
                };
                let dependency_path = fs::canonicalize(&dependency_path).with_context(|| {
                    format!(
                        "{}: 规范化 dependencies.{name}.path 失败: {}",
                        config.display(),
                        dependency_path.display()
                    )
                })?;
                *path_value = Value::String(path_text(&dependency_path, "dependency path")?.to_owned());
            }
            if let Some(features) = table.get("features") {
                let features = features
                    .as_array()
                    .with_context(|| format!("{}: dependencies.{name}.features 必须是字符串数组", config.display()))?;
                if features.iter().any(|feature| !feature.is_str()) {
                    bail!("{}: dependencies.{name}.features 必须是字符串数组", config.display());
                }
            }
            if table.get("default-features").is_some_and(|value| !value.is_bool()) {
                bail!("{}: dependencies.{name}.default-features 必须是 bool", config.display());
            }
        }
        _ => bail!(
            "{}: dependency `{name}` 必须是 Cargo version 字符串或 table",
            config.display()
        ),
    }
    Ok(value)
}

fn find_nearest_config(source: &Path) -> Result<Option<PathBuf>> {
    let mut directory = source.parent().context("裸脚本没有父目录")?;
    loop {
        let candidate = directory.join("rsvz.toml");
        if candidate.is_file() {
            return fs::canonicalize(&candidate)
                .map(Some)
                .with_context(|| format!("规范化配置路径失败: {}", candidate.display()));
        }
        let Some(parent) = directory.parent() else {
            return Ok(None);
        };
        directory = parent;
    }
}

fn stable_identity(path: &Path) -> Result<String> {
    let path = path_text(path, "裸脚本路径")?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in IDENTITY_VERSION
        .bytes()
        .chain([0])
        .chain(path.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

fn safe_name(name: &str, separator: char) -> String {
    let mut output = String::with_capacity(name.len().min(48));
    let mut separated = false;
    for ch in name.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            separator
        };
        if mapped == separator {
            if separated || output.is_empty() {
                continue;
            }
            separated = true;
        } else {
            separated = false;
        }
        output.push(mapped);
        if output.len() == 48 {
            break;
        }
    }
    let output = output.trim_end_matches(separator);
    if output.is_empty() {
        "script".to_owned()
    } else {
        output.to_owned()
    }
}

pub(crate) fn resolve_workspace_root(current_dir: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        let root = if root.is_absolute() {
            root.to_path_buf()
        } else {
            current_dir.join(root)
        };
        return validate_workspace_root(root);
    }
    if let Some(root) = current_dir.ancestors().find(|dir| is_workspace_root(dir)) {
        return Ok(root.to_path_buf());
    }
    if let Ok(executable) = env::current_exe()
        && let Some(root) = executable.ancestors().find(|dir| is_workspace_root(dir))
    {
        return Ok(root.to_path_buf());
    }
    bail!("找不到 RustVsZombies workspace root；请使用 --workspace-root")
}

fn validate_workspace_root(root: PathBuf) -> Result<PathBuf> {
    let root = canonical_user_path(&root).with_context(|| format!("规范化 workspace root 失败: {}", root.display()))?;
    if !is_workspace_root(&root) {
        bail!("不是 RustVsZombies workspace root: {}", root.display());
    }
    Ok(root)
}

fn canonical_user_path(path: &Path) -> std::io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(local) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(local));
        }
    }
    Ok(path)
}

fn is_workspace_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file() && path.join("crates/scripting/api/Cargo.toml").is_file()
}

fn path_text<'a>(path: &'a Path, kind: &str) -> Result<&'a str> {
    path.to_str()
        .with_context(|| format!("{kind} 不是有效 Unicode: {}", path.display()))
}

fn ensure_unicode(path: &Path, kind: &str) -> Result<()> {
    path_text(path, kind).map(|_| ())
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    let temporary = path.with_file_name(format!(".Cargo.toml.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("创建临时 manifest 失败: {}", temporary.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("写入临时 manifest 失败: {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("同步临时 manifest 失败: {}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path).with_context(|| format!("原子替换 manifest 失败: {}", path.display()))
    })();
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}

struct PreparationLock {
    path: PathBuf,
    file: Option<File>,
}

impl PreparationLock {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join(".prepare.lock");
        let started = Instant::now();
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    writeln!(file, "{}", std::process::id())
                        .with_context(|| format!("写入 preparation lock 失败: {}", path.display()))?;
                    return Ok(Self { path, file: Some(file) });
                }
                Err(error)
                    if error.kind() == ErrorKind::AlreadyExists && started.elapsed() < PREPARATION_LOCK_TIMEOUT =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    bail!("等待脚本 preparation lock 超时: {}", path.display());
                }
                Err(error) => {
                    return Err(error).with_context(|| format!("创建 preparation lock 失败: {}", path.display()));
                }
            }
        }
    }
}

impl Drop for PreparationLock {
    fn drop(&mut self) {
        self.file.take();
        let _cleanup_result = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovered_workspace_root_uses_a_process_compatible_path() {
        let start = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = resolve_workspace_root(start, None).expect("workspace root");
        assert!(is_workspace_root(&root));
        #[cfg(windows)]
        assert!(!root.to_string_lossy().starts_with(r"\\?\"));
    }
    use std::process::Command;

    fn temporary(name: &str) -> PathBuf {
        env::temp_dir().join(format!("rsvz-script-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn stable_identity_uses_the_canonical_path() {
        let first = PathBuf::from("C:/scripts/a/foo.rs");
        let second = PathBuf::from("C:/scripts/b/foo.rs");
        assert_eq!(
            stable_identity(&first).expect("identity"),
            stable_identity(&first).expect("identity")
        );
        assert_ne!(
            stable_identity(&first).expect("identity"),
            stable_identity(&second).expect("identity")
        );
    }

    #[test]
    fn nearest_config_and_backend_dependencies_are_selected_without_merging_parents() {
        let base = temporary("config");
        let script_dir = base.join("parent/child");
        fs::create_dir_all(&script_dir).expect("create script dir");
        fs::write(base.join("rsvz.toml"), "[dependencies]\nparent_only = \"1\"\n").expect("parent config");
        fs::write(
            base.join("parent/rsvz.toml"),
            "[dependencies]\nserde = \"1\"\n\
             [backend.pvz-emulator.dependencies]\npe_only = \"1\"\n\
             [backend.pvz-1-0-0-1051.dependencies]\nnative_only = \"1\"\n",
        )
        .expect("nearest config");
        let script = script_dir.join("测试.rs");
        fs::write(&script, "").expect("script");
        let script = fs::canonicalize(script).expect("canonical script");
        let config = find_nearest_config(&script).expect("find config").expect("config");
        assert_eq!(
            config,
            fs::canonicalize(base.join("parent/rsvz.toml")).expect("canonical config")
        );

        let pe = load_dependencies(Some(&config), Backend::PvzEmulator).expect("PE dependencies");
        assert!(pe.contains_key("serde"));
        assert!(pe.contains_key("pe_only"));
        assert!(!pe.contains_key("native_only"));
        assert!(!pe.contains_key("parent_only"));

        fs::remove_dir_all(base).expect("remove fixture");
    }

    #[test]
    fn duplicate_and_reserved_dependencies_are_field_errors() {
        let base = temporary("errors");
        fs::create_dir_all(&base).expect("create fixture");
        let config = base.join("rsvz.toml");
        fs::write(
            &config,
            "[dependencies]\nserde = \"1\"\n\
             [backend.pvz-emulator.dependencies]\nserde = \"1\"\n",
        )
        .expect("duplicate config");
        let error = load_dependencies(Some(&config), Backend::PvzEmulator)
            .expect_err("duplicate must fail")
            .to_string();
        assert!(error.contains("serde") && error.contains("同时"));

        fs::write(&config, "[dependencies]\nrsvz = \"1\"\n").expect("reserved config");
        let error = load_dependencies(Some(&config), Backend::PvzEmulator)
            .expect_err("reserved must fail")
            .to_string();
        assert!(error.contains("rsvz") && error.contains("保留"));
        fs::remove_dir_all(base).expect("remove fixture");
    }

    #[test]
    fn preparation_lock_serializes_the_same_hidden_crate() {
        let base = temporary("lock");
        fs::create_dir_all(&base).expect("create lock fixture");
        let first = PreparationLock::acquire(&base).expect("first lock");
        let (sent, received) = std::sync::mpsc::channel();
        let base_for_thread = base.clone();
        let thread = std::thread::spawn(move || {
            let second = PreparationLock::acquire(&base_for_thread).expect("second lock");
            sent.send(()).expect("report second lock");
            drop(second);
        });
        assert!(received.recv_timeout(Duration::from_millis(40)).is_err());
        drop(first);
        received
            .recv_timeout(Duration::from_secs(1))
            .expect("second lock must proceed");
        thread.join().expect("lock thread");
        fs::remove_dir_all(base).expect("remove lock fixture");
    }

    #[test]
    fn unchanged_manifest_is_not_rewritten() {
        let base = temporary("unchanged");
        fs::create_dir_all(&base).expect("create manifest fixture");
        let manifest = base.join("Cargo.toml");
        write_if_changed(&manifest, "same").expect("first write");
        let before = fs::metadata(&manifest)
            .expect("first metadata")
            .modified()
            .expect("first modified");
        std::thread::sleep(Duration::from_millis(20));
        write_if_changed(&manifest, "same").expect("second write");
        let after = fs::metadata(&manifest)
            .expect("second metadata")
            .modified()
            .expect("second modified");
        assert_eq!(before, after);
        write_if_changed(&manifest, "changed").expect("atomic replacement");
        assert_eq!(fs::read_to_string(&manifest).expect("replaced manifest"), "changed");
        fs::remove_dir_all(base).expect("remove manifest fixture");
    }

    #[test]
    fn hidden_manifest_uses_the_real_source_and_workspace_opt_out() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("workspace root")
            .to_path_buf();
        let source = root.join("dev-scripts/lowdsl/pe24/src/lib.rs");
        let text = hidden_manifest(
            &root,
            &source,
            Backend::PvzEmulator,
            "rsvz-script-pe24-0123",
            "rsvz_script_pe24_0123",
            Table::new(),
        )
        .expect("manifest");
        let manifest = Value::Table(toml::from_str(&text).expect("valid TOML"));
        assert_eq!(manifest["workspace"]["resolver"].as_str(), Some("3"));
        assert_eq!(manifest["workspace"]["members"][0].as_str(), Some("."));
        assert_eq!(
            manifest["lib"]["path"].as_str(),
            source.to_str(),
            "the source must be linked directly"
        );
        assert_eq!(
            manifest["dependencies"]["rsvz"]["features"][0].as_str(),
            Some("pvz-emulator")
        );
        assert!(!text.contains("include!"));
    }

    #[test]
    fn fixture_config_rewrites_only_the_selected_backend_path() {
        let config = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/裸 脚本/rsvz.toml");
        let pe = load_dependencies(Some(&config), Backend::PvzEmulator).expect("PE fixture dependencies");
        assert!(pe.contains_key("serde"));
        assert!(pe.contains_key("pe_script_model"));
        assert!(!pe.contains_key("native_script_model"));
        let model_path = pe["pe_script_model"]["path"].as_str().expect("rewritten path");
        assert!(Path::new(model_path).is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn external_unicode_script_and_sibling_module_pass_real_cargo_check() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("workspace root")
            .to_path_buf();
        let base = temporary("外部 路径");
        let source_dir = base.join("脚本 source");
        let manifest_dir = base.join("hidden crate");
        fs::create_dir_all(&source_dir).expect("create external source");
        fs::create_dir_all(&manifest_dir).expect("create hidden crate");
        let source = source_dir.join("测试.rs");
        fs::write(&source, "mod sibling;\npub fn answer() -> i32 { sibling::VALUE }\n").expect("write external source");
        fs::write(source_dir.join("sibling.rs"), "pub const VALUE: i32 = 42;\n").expect("write sibling");
        let source = fs::canonicalize(source).expect("canonical external source");
        let manifest = hidden_manifest(
            &root,
            &source,
            Backend::PvzEmulator,
            "rsvz-script-external-cargo-test",
            "rsvz_script_external_cargo_test",
            Table::new(),
        )
        .expect("render manifest");
        let manifest_path = manifest_dir.join("Cargo.toml");
        write_if_changed(&manifest_path, &manifest).expect("write manifest");
        let target_dir = env::temp_dir().join(format!("rsvz-cargo-target-{}", std::process::id()));

        let output = Command::new(env!("CARGO"))
            .args([
                "check",
                "--quiet",
                "--manifest-path",
                manifest_path.to_str().expect("manifest Unicode"),
                "--target-dir",
                target_dir.to_str().expect("target Unicode"),
            ])
            .output()
            .expect("run Cargo");
        assert!(
            output.status.success(),
            "Cargo check failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_dir_all(base).expect("remove external fixture");
        fs::remove_dir_all(target_dir).expect("remove external Cargo target");
    }

    #[cfg(windows)]
    #[test]
    fn non_unicode_paths_have_an_explicit_error() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        let raw = OsString::from_wide(&[0xd800]);
        let error = path_text(Path::new(&raw), "probe")
            .expect_err("unpaired surrogate must fail")
            .to_string();
        assert!(error.contains("Unicode"));
    }
}
