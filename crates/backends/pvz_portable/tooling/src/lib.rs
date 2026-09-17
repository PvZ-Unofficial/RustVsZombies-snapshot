//! Build and run tooling for the PvZ-Portable backend.

pub mod run_cli;
#[path = "../../backend/src/host/wire.rs"]
#[allow(
    dead_code,
    reason = "tooling writes the run config and only deserializes results in the launched host"
)]
mod wire;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use cargo_metadata::{MetadataCommand, TargetKind};
use clap::ValueEnum;

pub const MSVC_TARGET: &str = "x86_64-pc-windows-msvc";
pub const MINGW_TARGET: &str = "x86_64-pc-windows-gnu";
const BACKEND_RELATIVE: &str = "crates/backends/pvz_portable/backend";
const RSVZ_RELATIVE: &str = "crates/scripting/api";
const FINAL_ENTRY_RELATIVE: &str = "crates/backends/pvz_portable/final_entry/lib.rs";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum PortableToolchain {
    #[default]
    Msvc,
    #[value(name = "mingw-ucrt64")]
    MingwUcrt64,
}

impl PortableToolchain {
    #[must_use]
    pub const fn rust_target(self) -> &'static str {
        match self {
            Self::Msvc => MSVC_TARGET,
            Self::MingwUcrt64 => MINGW_TARGET,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Msvc => "msvc",
            Self::MingwUcrt64 => "mingw-ucrt64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug)]
pub struct PluginBuild {
    pub manifest_path: PathBuf,
    pub plugin_path: PathBuf,
    pub script_name: String,
    target_dir: PathBuf,
    target: &'static str,
    profile: BuildProfile,
}

pub fn prepare_plugin(
    root: &Path, current_dir: &Path, script_crate: &Path, script_name: Option<&str>, script_package: Option<&str>,
    script_features: &[String], toolchain: PortableToolchain, profile: BuildProfile,
) -> Result<PluginBuild> {
    let script_manifest = resolve_manifest(current_dir, script_crate)?;
    let metadata = MetadataCommand::new()
        .manifest_path(&script_manifest)
        .no_deps()
        .exec()
        .with_context(|| format!("读取脚本 metadata 失败: {}", script_manifest.display()))?;
    let packages = metadata
        .packages
        .iter()
        .filter(|package| metadata.workspace_members.contains(&package.id))
        .collect::<Vec<_>>();
    let package = if let Some(name) = script_package {
        packages
            .iter()
            .copied()
            .find(|package| package.name.as_str() == name)
            .with_context(|| format!("脚本 workspace 中没有 package `{name}`"))?
    } else if packages.len() == 1 {
        packages[0]
    } else {
        packages
            .iter()
            .copied()
            .find(|package| package.manifest_path.as_std_path() == script_manifest)
            .context("脚本是多 package workspace，请使用 --script-package")?
    };
    if package
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == &TargetKind::Lib))
        .count()
        != 1
    {
        bail!("用户脚本 package 必须恰好包含一个 lib target");
    }

    let script_name = script_name.unwrap_or(package.name.as_str());
    let mut script_features = script_features.to_vec();
    script_features.sort();
    script_features.dedup();
    let package_id = package.id.to_string();
    let manifest_path = package.manifest_path.to_string();
    let mut identity = vec![
        "rsvz-generated-target-v3",
        script_name,
        &package_id,
        &manifest_path,
        "default-features=false",
    ];
    identity.extend(script_features.iter().map(String::as_str));
    let identity = stable_hash(&identity);
    let safe_name = safe_name(script_name);
    let lib_name = format!("rsvz_pvzp_plugin_{}_{}", safe_name.replace('-', "_"), identity);
    let generated_dir = root
        .join("target/rsvz/generated/pvz-portable")
        .join(format!("{safe_name}-{identity}"));
    let target_dir = root
        .join("target/rsvz/pvz-portable")
        .join(&safe_name)
        .join(toolchain.name())
        .join(profile.dir_name())
        .join("rust");
    fs::create_dir_all(&generated_dir)?;
    fs::create_dir_all(&target_dir)?;
    let manifest_path = generated_dir.join("Cargo.toml");
    let final_entry = root.join(FINAL_ENTRY_RELATIVE);
    let backend = root.join(BACKEND_RELATIVE);
    let rsvz = root.join(RSVZ_RELATIVE);
    let script_dir = package.manifest_path.parent().context("脚本 manifest 缺少父目录")?;
    let features = if script_features.is_empty() {
        String::new()
    } else {
        format!(", features = {}", toml_string_array(&script_features))
    };
    let manifest = format!(
        "[package]\nname = {package_name}\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
         [lib]\nname = {lib_name_toml}\npath = {entry}\ncrate-type = [\"cdylib\"]\n\n\
         [dependencies]\nrsvz = {{ path = {rsvz}, default-features = false, features = [\"pvz-portable\"] }}\n\
         rsvz-pvz-portable-backend = {{ path = {backend} }}\n\
         user_script = {{ package = {user_package}, path = {user_path}, default-features = false{features} }}\n\n\
         [profile.release]\nlto = \"thin\"\n\n[workspace]\nresolver = \"3\"\n",
        package_name = toml_string(&format!("rsvz-pvzp-plugin-{safe_name}-{identity}")),
        lib_name_toml = toml_string(&lib_name),
        entry = toml_path(&final_entry),
        rsvz = toml_path(&rsvz),
        backend = toml_path(&backend),
        user_package = toml_string(package.name.as_str()),
        user_path = toml_path(script_dir.as_std_path()),
    );
    if fs::read_to_string(&manifest_path).ok().as_deref() != Some(&manifest) {
        fs::write(&manifest_path, manifest)?;
    }

    let plugin_path = target_dir
        .join(toolchain.rust_target())
        .join(profile.dir_name())
        .join(format!("{lib_name}.dll"));
    Ok(PluginBuild {
        manifest_path,
        plugin_path,
        script_name: script_name.to_owned(),
        target_dir,
        target: toolchain.rust_target(),
        profile,
    })
}

pub fn build_plugin(plugin: &PluginBuild, portable_root: &Path, import_dir: &Path) -> Result<()> {
    let mut cargo = Command::new("cargo");
    cargo
        .arg("build")
        .arg("--manifest-path")
        .arg(&plugin.manifest_path)
        .arg("--target")
        .arg(plugin.target)
        .arg("--target-dir")
        .arg(&plugin.target_dir)
        .env("PVZP_ROOT", portable_root)
        .env("RSVZ_PORTABLE_IMPORT_DIR", import_dir);
    let native_dir = if plugin.target == MSVC_TARGET {
        import_dir.parent().expect("MSVC config directory")
    } else {
        import_dir
    };
    let sdk = native_dir.join("sdk").join(match plugin.profile {
        BuildProfile::Debug => "Debug",
        BuildProfile::Release => "Release",
    });
    if !sdk.join("build.txt").is_file() {
        bail!("Portable did not produce its SDK: {}", sdk.display());
    }
    cargo.env("PVZP_SDK", sdk);
    if plugin.profile == BuildProfile::Debug && plugin.target == MSVC_TARGET {
        // Rust's default CRT import is release; the matching C++ Debug SDK uses /MDd.
        let mut flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
        for flag in [
            "-Clink-arg=/NODEFAULTLIB:msvcrt.lib",
            "-Clink-arg=/DEFAULTLIB:msvcrtd.lib",
        ] {
            if !flags.is_empty() {
                flags.push('\x1f');
            }
            flags.push_str(flag);
        }
        cargo.env("CARGO_ENCODED_RUSTFLAGS", flags);
    }
    if plugin.profile == BuildProfile::Release {
        cargo.arg("--release");
        // Encoded arguments preserve spaces in the installed LLVM directory.
        let mut args: Vec<String> = std::env::var("CARGO_ENCODED_RUSTFLAGS")
            .unwrap_or_default()
            .split('\x1f')
            .filter(|flag| !flag.is_empty())
            .map(str::to_owned)
            .collect();
        args.push("-Clinker-plugin-lto".to_owned());
        if plugin.target == MSVC_TARGET {
            args.push("-Clinker=C:/Program Files/LLVM/bin/lld-link.exe".to_owned());
        } else {
            args.extend(
                [
                    "-Clinker=C:/Program Files/LLVM/bin/clang.exe",
                    "-Clink-arg=--target=x86_64-w64-windows-gnu",
                    "-Clink-arg=--sysroot=C:/msys64/ucrt64",
                    "-Clink-arg=-fuse-ld=lld",
                ]
                .map(str::to_owned),
            );
        }
        cargo.env("CARGO_ENCODED_RUSTFLAGS", args.join("\x1f"));
    }
    let status = cargo.status().context("启动 Cargo 构建 Portable plugin 失败")?;
    if !status.success() {
        bail!("Portable plugin Cargo 构建失败");
    }
    if !plugin.plugin_path.is_file() {
        bail!("Cargo 未生成预期的 plugin DLL: {}", plugin.plugin_path.display());
    }
    Ok(())
}

fn resolve_manifest(current_dir: &Path, path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    };
    let path = if path.file_name().is_some_and(|name| name == "Cargo.toml") {
        path
    } else {
        path.join("Cargo.toml")
    };
    fs::canonicalize(&path).with_context(|| format!("脚本 Cargo.toml 不存在: {}", path.display()))
}

fn stable_hash(parts: &[&str]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in parts.iter().flat_map(|part| part.bytes().chain([0])) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn safe_name(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars().take(48) {
        let ch = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        if ch != '-' || !result.ends_with('-') {
            result.push(ch);
        }
    }
    let result = result.trim_matches('-');
    if result.is_empty() {
        "script".to_owned()
    } else {
        result.to_owned()
    }
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("JSON strings are valid TOML basic strings")
}

fn toml_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    toml_string(&path.replace('\\', "/"))
}

fn toml_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolchain_names_match_their_rust_targets() {
        assert_eq!(PortableToolchain::Msvc.name(), "msvc");
        assert_eq!(PortableToolchain::Msvc.rust_target(), "x86_64-pc-windows-msvc");
        assert_eq!(PortableToolchain::MingwUcrt64.name(), "mingw-ucrt64");
        assert_eq!(PortableToolchain::MingwUcrt64.rust_target(), "x86_64-pc-windows-gnu");
    }

    #[test]
    fn generated_names_and_hashes_are_stable() {
        assert_eq!(safe_name("  My 脚本!!  "), "my");
        assert_eq!(safe_name("---"), "script");

        assert_ne!(stable_hash(&["a", "bc"]), stable_hash(&["ab", "c"]));
    }

    #[test]
    fn generated_toml_uses_portable_paths_and_escaped_features() {
        assert_eq!(toml_path(Path::new(r"\\?\C:\work\rsvz")), r#""C:/work/rsvz""#);
        assert_eq!(
            toml_string_array(&["one".to_owned(), "quoted\"feature".to_owned()]),
            r#"["one", "quoted\"feature"]"#
        );
    }
}
