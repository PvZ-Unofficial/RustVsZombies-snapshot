"""Check actual target-filtered Cargo normal/build graphs (stdlib only)."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[4]
BACKENDS = {"rsvz-pvz-emulator-backend", "rsvz-pvz1051-injected", "rsvz-pvz-portable-backend"}
NO_BACKEND = "rsvz-no-backend"
FOUNDATIONS = {"rsvz-model", "rsvz-backend-api", "rsvz-profiling"}


def check(manifest, target, features, selected):
    command = ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
               "--manifest-path", str(manifest), "--filter-platform", target]
    if features is not None:
        command += ["--no-default-features", "--features", features]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(result.stderr)
    metadata = json.loads(result.stdout)
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    packages = {package["id"]: package for package in metadata["packages"]}

    def dependencies(package_id):
        return [dep["pkg"] for dep in nodes[package_id]["deps"]
                if any(kind["kind"] != "dev" for kind in dep["dep_kinds"])]

    root_id = metadata["resolve"]["root"]
    assert root_id is not None, "pass a package manifest, not a virtual workspace"
    reachable, pending = set(), [root_id]
    while pending:
        package_id = pending.pop()
        if package_id not in reachable:
            reachable.add(package_id)
            pending.extend(dependencies(package_id))

    for package_id in reachable:
        package = packages[package_id]
        name = package["name"]
        path = Path(package["manifest_path"]).resolve()
        assert name != "rsvz-runtime", name
        assert not path.is_relative_to(ROOT / "crates/tools"), name
        assert not (path.is_relative_to(ROOT / "crates/backends") and "tooling" in path.parts), name
        if name == "rsvz-profiling":
            assert not dependencies(package_id), "profiling must remain std-only"
        for dep_id in dependencies(package_id):
            dep = packages[dep_id]
            dep_path = Path(dep["manifest_path"]).resolve()
            if name in FOUNDATIONS:
                allowed = {"rsvz-model"} if name == "rsvz-backend-api" else set()
                if dep_path.is_relative_to(ROOT):
                    assert dep["name"] in allowed, (name, dep["name"])
            if path.is_relative_to(ROOT / "crates/backends"):
                if dep_path.is_relative_to(ROOT) and not dep_path.is_relative_to(ROOT / "crates/backends"):
                    assert dep["name"] in FOUNDATIONS, (name, dep["name"])
            if name in {"rsvz-game", "rsvz-schedule"}:
                allowed = FOUNDATIONS | {"rsvz-current"}
                if name == "rsvz-game":
                    allowed.add("rsvz-schedule")
                if dep_path.is_relative_to(ROOT):
                    assert dep["name"] in allowed, (name, dep["name"])
            if name in {"rsvz", "rsvz-current"} and dep_path.is_relative_to(ROOT):
                allowed = BACKENDS | {NO_BACKEND} if name == "rsvz-current" else FOUNDATIONS | {
                    "rsvz-game", "rsvz-schedule", "rsvz-current", "rsvz-macros"
                }
                assert dep["name"] in allowed, (name, dep["name"])
    actual = {packages[package_id]["name"] for package_id in reachable} & BACKENDS
    if selected is None:
        assert len(actual) == 1, actual
    else:
        assert actual == selected, (actual, selected)
    names = {packages[package_id]["name"] for package_id in reachable}
    if selected == set():
        assert {"rsvz-current", NO_BACKEND} <= names, names
        assert not names & {"pe-rs", "pvzp-rs"}, names
    print(f"{packages[root_id]['name']} {target} [{features or 'manifest features'}] graph PASS")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="check one generated runner instead of the standard graphs")
    parser.add_argument("--target", help="required with --manifest")
    args = parser.parse_args()
    if args.manifest:
        if not args.target:
            parser.error("--manifest requires --target")
        check(args.manifest.resolve(), args.target, None, None)
        return
    if args.target:
        parser.error("--target requires --manifest")
    check(ROOT / "crates/scripting/api/tests/current-capabilities/Cargo.toml", "x86_64-pc-windows-msvc", "", set())
    for selector, target, backend, directory in [
        ("pvz-emulator", "x86_64-pc-windows-msvc", "rsvz-pvz-emulator-backend", "pvz_emulator/backend"),
        ("pvz-1-0-0-1051", "i686-pc-windows-msvc", "rsvz-pvz1051-injected", "pvz_1_0_0_1051/injected"),
        ("pvz-portable", "x86_64-pc-windows-msvc", "rsvz-pvz-portable-backend", "pvz_portable/backend"),
        ("pvz-portable", "x86_64-pc-windows-gnu", "rsvz-pvz-portable-backend", "pvz_portable/backend"),
    ]:
        check(ROOT / "crates/scripting/api/Cargo.toml", target, selector, {backend})
        manifest = ROOT / "crates/backends" / directory / "Cargo.toml"
        check(manifest, target, "host" if selector == "pvz-emulator" else "", {backend})
        if selector == "pvz-1-0-0-1051":
            check(manifest, target, "fast-forward-profiler", {backend})
    # metadata unifies the PE dev-dependency's features into Witness's normal
    # rsvz node. tree's edge selection resolves the actual tooling-only graph.
    result = subprocess.run([
        "cargo", "tree", "--locked", "--offline", "--manifest-path",
        str(ROOT / "extensions/rsvz-witness/Cargo.toml"), "--no-default-features",
        "--features", "tooling", "--edges", "normal,build", "--prefix", "none",
        "--target", "x86_64-pc-windows-msvc",
    ], cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(result.stderr)
    names = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
    assert {"rsvz-witness", "rsvz", "rsvz-current", NO_BACKEND} <= names, names
    assert not names & (BACKENDS | {"rsvz-runtime", "pe-rs", "pvzp-rs"}), names
    print("Witness tooling-only normal/build graph PASS")


if __name__ == "__main__":
    main()
