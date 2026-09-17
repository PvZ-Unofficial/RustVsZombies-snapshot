use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let rsvz_root = manifest_dir
        .ancestors()
        .nth(4)
        .expect("pe-rs must live under RustVsZombies/crates/backends/pvz_emulator");
    let pe_root = env::var_os("PE_RS_SOURCE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        let bundled = rsvz_root.join("vendor/pvz-emulator");
        if bundled.join("CMakeLists.txt").is_file() {
            bundled
        } else {
            rsvz_root
                .parent()
                .expect("RustVsZombies must have a workspace parent")
                .join("PvZ-Emulator")
        }
    });
    assert!(
        pe_root.join("CMakeLists.txt").is_file(),
        "PvZ-Emulator source tree not found at {}; set PE_RS_SOURCE_DIR to override",
        pe_root.display()
    );
    println!("cargo:warning=PvZ-Emulator source: {}", pe_root.display());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let build_config = NativeBuildConfig::from_env();
    let cmake_build_dir = out_dir.join(build_config.build_dir_name());
    let cmake_config = cmake_config();

    emit_rerun_paths(&manifest_dir, &pe_root);
    generate_bindings(&manifest_dir, &pe_root, &out_dir);
    configure_pvzemu(&pe_root, &cmake_build_dir, cmake_config, &build_config);
    build_pvzemu(&cmake_build_dir, cmake_config, &build_config);

    compile_bridge(&manifest_dir, &pe_root, &build_config);

    println!("cargo:rustc-link-search=native={}", cmake_build_dir.display());
    println!(
        "cargo:rustc-link-search=native={}",
        cmake_build_dir.join(cmake_config).display()
    );
    println!("cargo:rustc-link-lib=static=pvzemu");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeToolchain {
    Msvc,
    ClangCl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LlvmLto {
    None,
    Thin,
    Full,
}

impl LlvmLto {
    const fn compiler_flag(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Thin => Some("/clang:-flto=thin"),
            Self::Full => Some("/clang:-flto"),
        }
    }
}

#[derive(Clone, Debug)]
struct NativeBuildConfig {
    toolchain: NativeToolchain,
    lto: LlvmLto,
    pgo_generate: bool,
    pgo_use: Option<PathBuf>,
    llvm_bin: Option<PathBuf>,
}

impl NativeBuildConfig {
    fn from_env() -> Self {
        let lto = match env::var("PE_RS_LLVM_LTO") {
            Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
                "" | "off" | "none" => LlvmLto::None,
                "thin" => LlvmLto::Thin,
                "full" | "fat" => LlvmLto::Full,
                other => panic!("unsupported PE_RS_LLVM_LTO value: {other}"),
            },
            Err(_) => LlvmLto::None,
        };
        let toolchain = match env::var("PE_RS_TOOLCHAIN") {
            Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
                "" | "default" | "clang" | "clang-cl" => NativeToolchain::ClangCl,
                "msvc" => NativeToolchain::Msvc,
                other => panic!("unsupported PE_RS_TOOLCHAIN value: {other}"),
            },
            Err(_) => NativeToolchain::ClangCl,
        };
        assert!(
            toolchain != NativeToolchain::Msvc || lto == LlvmLto::None,
            "LLVM LTO requires clang-cl"
        );
        Self {
            toolchain,
            lto,
            pgo_generate: env::var_os("PE_RS_PGO_GENERATE").is_some(),
            pgo_use: env::var_os("PE_RS_PGO_USE").map(PathBuf::from),
            llvm_bin: resolve_llvm_bin(),
        }
    }

    fn build_dir_name(&self) -> String {
        let base = match (self.toolchain, self.lto) {
            (NativeToolchain::Msvc, LlvmLto::None) => "pvzemu-build-msvc",
            (NativeToolchain::ClangCl, LlvmLto::None) => "pvzemu-build-clang",
            (NativeToolchain::ClangCl, LlvmLto::Thin) => "pvzemu-build-clang-thinlto",
            (NativeToolchain::ClangCl, LlvmLto::Full) => "pvzemu-build-clang-fulllto",
            (NativeToolchain::Msvc, LlvmLto::Thin | LlvmLto::Full) => {
                unreachable!("LLVM LTO implies clang-cl")
            }
        };
        match (self.pgo_generate, self.pgo_use.is_some()) {
            (true, false) => format!("{base}-pgo-generate"),
            (false, true) => format!("{base}-pgo-use"),
            (false, false) => base.to_owned(),
            (true, true) => panic!("PE_RS_PGO_GENERATE and PE_RS_PGO_USE are mutually exclusive"),
        }
    }

    const fn uses_clang_cl(&self) -> bool {
        matches!(self.toolchain, NativeToolchain::ClangCl)
    }

    fn llvm_tool(&self, name: &str) -> PathBuf {
        if let Some(llvm_bin) = self.llvm_bin.as_ref() {
            let path = llvm_bin.join(tool_exe(name));
            if path.exists() {
                return path;
            }
        }
        PathBuf::from(tool_exe(name))
    }

    fn apply_tool_path_env(&self, command: &mut Command) {
        if !self.uses_clang_cl() {
            return;
        }

        let mut paths = Vec::new();
        if let Some(llvm_bin) = self.llvm_bin.as_ref() {
            paths.push(llvm_bin.clone());
        }
        if let Some(rc_dir) = resolve_windows_kit_tool("rc").and_then(|path| path.parent().map(Path::to_path_buf)) {
            paths.push(rc_dir);
        }
        prepend_path(command, paths);
    }

    fn clang_compile_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        if let Some(flag) = self.lto.compiler_flag() {
            flags.push(flag.to_owned());
        }
        if self.pgo_generate {
            flags.push("/clang:-fprofile-generate".to_owned());
        }
        if let Some(profile) = self.pgo_use.as_ref() {
            flags.push(format!("/clang:-fprofile-use={}", cmake_path(profile)));
        }
        flags
    }
}

fn cmake_config() -> &'static str {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") || env::var("PROFILE").as_deref() == Ok("release") {
        "Release"
    } else {
        "Debug"
    }
}

fn emit_rerun_paths(manifest_dir: &Path, pe_root: &Path) {
    for var in [
        "PE_RS_SOURCE_DIR",
        "PE_RS_TOOLCHAIN",
        "PE_RS_LLVM_LTO",
        "PE_RS_LLVM_BIN",
        "PE_RS_PGO_GENERATE",
        "PE_RS_PGO_USE",
        "PE_RS_PGO_INPUT_FINGERPRINT",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    if let Some(profile) = env::var_os("PE_RS_PGO_USE") {
        println!("cargo:rerun-if-changed={}", PathBuf::from(profile).display());
    }
    for path in [
        manifest_dir.join("cpp/bridge.h"),
        manifest_dir.join("cpp/bridge.cpp"),
        manifest_dir.join("cpp/bridge_gameplay.inc"),
        manifest_dir.join("cpp/raw_layout.h"),
        pe_root.join("CMakeLists.txt"),
        pe_root.join("world.h"),
        pe_root.join("world.cpp"),
        pe_root.join("lib"),
        pe_root.join("object"),
        pe_root.join("system"),
        pe_root.join("learning"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn generate_bindings(manifest_dir: &Path, pe_root: &Path, out_dir: &Path) {
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("cpp/bridge.h").to_string_lossy())
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17");
    for include_dir in [
        pe_root.to_path_buf(),
        pe_root.join("lib"),
        pe_root.join("object"),
        pe_root.join("system"),
    ] {
        builder = builder.clang_arg(format!("-I{}", cmake_path(&include_dir)));
    }

    let bindings = builder
        .allowlist_type("pe_rs_.*")
        .allowlist_function("pe_rs_.*")
        .allowlist_type("pvz_emulator::object::plant_type")
        .allowlist_type("pvz_emulator::object::plant_status")
        .allowlist_type("pvz_emulator::object::zombie_type")
        .allowlist_type("pvz_emulator::object::zombie_status")
        .allowlist_type("pvz_emulator::object::zombie_action")
        .ignore_methods()
        .blocklist_type(".*obj_list.*")
        .blocklist_type(".*obj_wrap.*")
        .opaque_type("std::.*")
        .opaque_type("rapidjson::.*")
        .opaque_type("pvz_emulator::object::scene")
        .derive_debug(false)
        .layout_tests(true)
        .generate()
        .expect("failed to generate PE raw layout bindings");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write PE bindings");
}

fn configure_pvzemu(pe_root: &Path, build_dir: &Path, cmake_config: &str, build_config: &NativeBuildConfig) {
    let mut command = Command::new("cmake");
    command
        .arg("-S")
        .arg(pe_root)
        .arg("-B")
        .arg(build_dir)
        .arg(format!("-DCMAKE_BUILD_TYPE={cmake_config}"))
        .arg("-DPVZEMU_BUILD_TESTS=OFF")
        .arg("-DPVZEMU_BUILD_PYBIND=OFF")
        .arg("-DPVZEMU_BUILD_DEBUGGER=OFF");
    if build_config.uses_clang_cl() {
        build_config.apply_tool_path_env(&mut command);
        command
            .arg("-G")
            .arg("Ninja")
            .arg(format!(
                "-DCMAKE_CXX_COMPILER={}",
                cmake_path(&build_config.llvm_tool("clang-cl"))
            ))
            .arg(format!(
                "-DCMAKE_LINKER={}",
                cmake_path(&build_config.llvm_tool("lld-link"))
            ))
            .arg(format!(
                "-DCMAKE_AR={}",
                cmake_path(&build_config.llvm_tool("llvm-lib"))
            ));
        if let Some(rc) = resolve_windows_kit_tool("rc") {
            command.arg(format!("-DCMAKE_RC_COMPILER={}", cmake_path(&rc)));
        }
        if let Some(mt) = resolve_windows_kit_tool("mt") {
            command.arg(format!("-DCMAKE_MT={}", cmake_path(&mt)));
        }
    }
    let mut native_flags = build_config.clang_compile_flags();
    if build_config.uses_clang_cl() {
        native_flags.push("/EHsc".to_owned());
    }
    if !native_flags.is_empty() {
        // CMake/Ninja parses this string again, so preserve each flag as one argument.
        command.arg(format!("-DCMAKE_CXX_FLAGS=\"{}\"", native_flags.join("\" \"")));
    }
    run_cmake(&mut command, "configure pvzemu");
}

fn build_pvzemu(build_dir: &Path, cmake_config: &str, build_config: &NativeBuildConfig) {
    let mut command = Command::new("cmake");
    command
        .arg("--build")
        .arg(build_dir)
        .arg("--target")
        .arg("pvzemu")
        .arg("--config")
        .arg(cmake_config);
    build_config.apply_tool_path_env(&mut command);
    run_cmake(&mut command, "build pvzemu");
}

fn compile_bridge(manifest_dir: &Path, pe_root: &Path, build_config: &NativeBuildConfig) {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file(manifest_dir.join("cpp/bridge.cpp"))
        .include(pe_root)
        .include(pe_root.join("lib"))
        .include(pe_root.join("system"))
        .include(pe_root.join("object"))
        .include(pe_root.join("learning"));
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.flag("/EHsc");
    }
    if build_config.uses_clang_cl() {
        build.compiler(build_config.llvm_tool("clang-cl"));
        if env::var("PROFILE").as_deref() == Ok("release") {
            build.flag("/clang:-O3");
        }
    }
    for flag in build_config.clang_compile_flags() {
        build.flag(&flag);
    }
    build.compile("pe_rs_bridge");
}

fn run_cmake(command: &mut Command, action: &str) {
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("failed to invoke cmake to {action}: {err}"));

    assert!(status.success(), "cmake failed to {action}: {status}");
}

fn resolve_llvm_bin() -> Option<PathBuf> {
    env::var_os("PE_RS_LLVM_BIN")
        .map(PathBuf::from)
        .filter(|path| path.join(tool_exe("clang-cl")).exists())
        .or_else(|| {
            let path = PathBuf::from(r"C:\Program Files\LLVM\bin");
            path.join(tool_exe("clang-cl")).exists().then_some(path)
        })
}

fn resolve_windows_kit_tool(name: &str) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }

    let root = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    versions.sort();
    for version in versions.iter().rev() {
        let path = version.join("x64").join(tool_exe(name));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn prepend_path(command: &mut Command, new_paths: Vec<PathBuf>) {
    if new_paths.is_empty() {
        return;
    }

    let mut paths = new_paths;
    if let Some(current) = env::var_os("PATH") {
        paths.extend(env::split_paths(&current));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env("PATH", joined);
    }
}

fn cmake_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn tool_exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}
