"""Export committed sources and assemble a Windows Release, without vendoring into Git.

Requires a built target/release/rsvz.exe and explicit local tool distributions.
Downloads only pinned Ninja and LLVM's license; does not publish or modify inputs.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import urllib.request
import zipfile


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def export(root, destination, include=None):
    if git(root, "status", "--porcelain", "--untracked-files=no").strip():
        raise RuntimeError(f"Commit tracked changes before packaging: {root}")
    commit = git(root, "rev-parse", "HEAD").decode().strip()
    with zipfile.ZipFile(io.BytesIO(git(root, "archive", "--format=zip", commit))) as archive:
        for item in archive.infolist():
            relative = Path(item.filename)
            if item.is_dir() or (include and not include(relative)):
                continue
            if relative.is_absolute() or ".." in relative.parts:
                raise RuntimeError(f"Unsafe archive path: {relative}")
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(item))
    return commit


def download(url):
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def copy(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if source.is_dir():
        shutil.copytree(source, destination)
    else:
        shutil.copy2(source, destination)


def archive_tree(root, output):
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for directory, directories, files in os.walk(root):
            if Path(directory) == root:
                directories[:] = [d for d in directories if d not in {".git", ".cargo-home", "target", "results"}]
            directories.sort()
            for name in sorted(files):
                path = Path(directory) / name
                archive.write(path, Path("RustVsZombies") / path.relative_to(root))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pe-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New directory; never overwritten")
    parser.add_argument("--rust-toolchain", type=Path, required=True)
    parser.add_argument("--llvm-root", type=Path, required=True)
    parser.add_argument("--cmake-root", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    # Preserve the compiler and standard libraries as an installed distribution.
    for required in [root / "target/release/rsvz.exe",
                     args.rust_toolchain / "bin/rustc.exe",
                     args.rust_toolchain / "lib/rustlib/i686-pc-windows-msvc/lib",
                     args.rust_toolchain / "lib/rustlib/x86_64-pc-windows-msvc/lib"]:
        if not required.exists():
            raise RuntimeError(f"Missing input: {required}")
    args.output.mkdir(parents=True, exist_ok=False)
    source = args.output / "source"
    allowed = {".cargo", ".editorconfig", ".gitattributes", ".gitignore", "AGENTS.md",
               "Cargo.lock", "Cargo.toml", "LICENSE", "README.md", "crates", "dev-scripts",
               "docs", "extensions", "rust-toolchain.toml", "rustfmt.toml", "scripts", "tests"}
    rsvz_commit = export(root, source, lambda p: p.parts[0] in allowed
                         and p.parts[:2] != ("docs", "history"))
    pe_commit = export(args.pe_root, source / "vendor/pvz-emulator")
    # These entries keep the public snapshot's tools, local scripts and logs out of Git.
    with (source / ".gitignore").open("a", encoding="utf-8") as f:
        f.write("\n/tools/\n/vendor/pvz-emulator/\n/.cargo-home/\n/results/\n/user-scripts/\n")
    versions = {
        "rsvz_commit": rsvz_commit, "pe_commit": pe_commit,
        "rust": subprocess.check_output([str(args.rust_toolchain / "bin/rustc.exe"), "-vV"]).decode(),
        "llvm": subprocess.check_output([str(args.llvm_root / "bin/clang-cl.exe"), "--version"]).decode(),
        "cmake": subprocess.check_output([str(args.cmake_root / "bin/cmake.exe"), "--version"]).decode(),
        "ninja": "1.13.1",
        "requires": ["Microsoft MSVC x86/x64 C++ Build Tools", "Windows SDK", "network for Cargo dependencies"],
        "pe_source": "https://github.com/PvZ-Unofficial/PvZ-Emulator",
        "ninja_source": "https://github.com/ninja-build/ninja/releases/tag/v1.13.1",
        "llvm_source": "https://github.com/llvm/llvm-project/releases/tag/llvmorg-22.1.7",
    }
    (source / "SNAPSHOT.json").write_text(json.dumps(versions, ensure_ascii=False, indent=2), encoding="utf-8")
    archive_tree(source, args.output / "rsvz-source.zip")
    print("Source archive ready", flush=True)
    bundle = args.output / "bundle"
    shutil.copytree(source, bundle)
    tools = bundle / "tools"
    copy(root / "target/release/rsvz.exe", tools / "rsvz.exe")
    # Retain licenses; omit documentation pages and the unused GNU target only.
    shutil.copytree(args.rust_toolchain, tools / "rust",
                    ignore=shutil.ignore_patterns("html", "x86_64-pc-windows-gnu"))
    for name in ["clang-cl.exe", "clang.exe", "lld-link.exe", "libclang.dll", "llvm-ar.exe", "llvm-lib.exe"]:
        copy(args.llvm_root / "bin" / name, tools / "llvm/bin" / name)
    copy(args.llvm_root / "lib/clang", tools / "llvm/lib/clang")
    (tools / "llvm/LICENSE.txt").write_bytes(download(
        "https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-22.1.7/llvm/LICENSE.TXT"))
    for name in ["clang", "lld", "compiler-rt"]:
        (tools / "llvm" / f"LICENSE-{name}.txt").write_bytes(download(
            f"https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-22.1.7/{name}/LICENSE.TXT"))
    copy(args.cmake_root / "bin", tools / "cmake/bin")
    copy(args.cmake_root / "share", tools / "cmake/share")
    copy(args.cmake_root / "doc", tools / "cmake/doc")
    ninja = download("https://github.com/ninja-build/ninja/releases/download/v1.13.1/ninja-win.zip")
    (tools / "ninja").mkdir()
    with zipfile.ZipFile(io.BytesIO(ninja)) as archive:
        (tools / "ninja/ninja.exe").write_bytes(archive.read("ninja.exe"))
    (tools / "ninja/LICENSE").write_bytes(download(
        "https://raw.githubusercontent.com/ninja-build/ninja/v1.13.1/COPYING"))
    print("Toolchain assembled; compressing", flush=True)
    archive_tree(bundle, args.output / "rsvz-windows-tools.zip")
    with (args.output / "SHA256SUMS").open("w", encoding="ascii") as f:
        for name in ["rsvz-source.zip", "rsvz-windows-tools.zip"]:
            with (args.output / name).open("rb") as artifact:
                digest = hashlib.file_digest(artifact, "sha256").hexdigest()
            f.write(f"{digest}  {name}\n")
    print(f"Release artifacts: {args.output}", flush=True)


if __name__ == "__main__":
    main()
