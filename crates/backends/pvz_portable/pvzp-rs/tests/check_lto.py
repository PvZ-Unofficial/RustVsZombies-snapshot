"""Build a generated Portable runner in a fresh directory and verify its ThinLTO.

Pass the existing runner --manifest and matching --sdk; no game is launched.
The new directory retains the DLL, IR and build log as evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib

NEEDLE = "pvzp_rs_set_sun_cost_ignored"
LLVM = Path("C:/Program Files/LLVM/bin")


def new_build_dir(path):
    if path is None:
        return Path(tempfile.mkdtemp(prefix="rsvz-lto-")).resolve()
    path = path.resolve()
    path.mkdir(parents=True, exist_ok=False)
    return path


def digest(path):
    return {"path": str(path.resolve()), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def verify_ir(before, after):
    calls = sum(NEEDLE in line and "call " in line for line in before.splitlines())
    if not calls:
        return None
    after_calls = sum(NEEDLE in line and "call " in line for line in after.splitlines())
    if after_calls:
        raise ValueError("representative Rust-to-C++ call was not inlined")
    if "gModifiers" in before or "gModifiers" not in after:
        raise ValueError("native global was not imported into Rust IR")
    stores = [line.strip() for line in after.splitlines() if "store i8" in line and "gModifiers" in line]
    if not stores:
        raise ValueError("no inlined native modifier store")
    return {"representative": NEEDLE, "calls_before": calls, "calls_after": after_calls,
            "inlined_native_stores": stores}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True, help="generated runner Cargo.toml")
    parser.add_argument("--sdk", type=Path, required=True, help="matching native Release SDK")
    parser.add_argument("--abi", choices=["msvc", "gnu"], required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--build-dir", type=Path, help="must not exist; defaults to a new temporary directory")
    args = parser.parse_args()
    manifest, sdk = args.manifest.resolve(), args.sdk.resolve()
    package = tomllib.loads(manifest.read_text(encoding="utf-8"))
    library = package["lib"]
    if library.get("crate-type") != ["cdylib"]:
        parser.error("manifest must describe one generated cdylib runner")
    if "profile=Release" not in sdk.joinpath("build.txt").read_text(encoding="utf-8").splitlines():
        parser.error("SDK must use the Release profile")
    build_dir = new_build_dir(args.build_dir)
    triple = "x86_64-pc-windows-msvc" if args.abi == "msvc" else "x86_64-pc-windows-gnu"
    native_triple = "x86_64-pc-windows-msvc" if args.abi == "msvc" else "x86_64-w64-windows-gnu"
    flags = [flag for flag in os.environ.get("CARGO_ENCODED_RUSTFLAGS", "").split("\x1f") if flag]
    flags.append("-Clinker-plugin-lto")
    if args.abi == "msvc":
        flags += [f"-Clinker={LLVM / 'lld-link.exe'}", "-Clink-arg=/lldsavetemps"]
    else:
        flags += [f"-Clinker={LLVM / 'clang.exe'}", "-Clink-arg=--target=" + native_triple,
                  "-Clink-arg=--sysroot=C:/msys64/ucrt64", "-Clink-arg=-fuse-ld=lld",
                  "-Clink-arg=-Wl,--Xlink=/lldsavetemps"]
    env = dict(os.environ, PVZP_SDK=str(sdk), CARGO_ENCODED_RUSTFLAGS="\x1f".join(flags))
    command = ["cargo", "build", "--manifest-path", str(manifest), "--lib", "--release",
               "--locked", "--offline", "--target", triple, "--target-dir", str(build_dir)]
    log = build_dir / "build.log"
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run(command, env=env, stdout=output, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"runner build failed with exit code {result.returncode}; see {log}")
    dll = build_dir / triple / "release" / (library["name"] + ".dll")
    dll_record = digest(dll)

    def read_ir(path):
        return subprocess.check_output([str(LLVM / "clang.exe"), "--target=" + native_triple,
            "-S", "-emit-llvm", "-Xclang", "-disable-llvm-passes", "-Wno-override-module",
            str(path), "-o", "-"], text=True, encoding="utf-8")

    for before_path in sorted((dll.parent / "deps").glob("*rlibpvzp_rs-*0.preopt.bc")):
        after_path = Path(str(before_path).replace(".0.preopt.bc", ".4.opt.bc"))
        verification = verify_ir(read_ir(before_path), read_ir(after_path))
        if verification is None:
            continue
        report = {"build_dir": str(build_dir), "command": command, "rustflags": flags,
                  "manifest": digest(manifest), "sdk_config": digest(sdk / "build.txt"),
                  "build_log": digest(log), "dll": dll_record, "rust_module_before": digest(before_path),
                  "rust_module_after": digest(after_path), **verification}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"Fresh final DLL cross-language ThinLTO PASS: {args.output}")
        return
    raise RuntimeError(f"No representative Rust module in this final link: {build_dir}")


if __name__ == "__main__":
    main()
