"""Serial native smoke tests. Rust tooling owns game processes, wire validation and cleanup."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import subprocess
import time


def fingerprint(directory):
    """Compare template file contents, not access times changed by reading."""
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            with path.open("rb") as stream:
                result[str(path.relative_to(directory))] = hashlib.file_digest(stream, "sha256").hexdigest()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["1051", "portable", "all"], default="all")
    parser.add_argument("--game-1051", type=Path)
    parser.add_argument("--resources", type=Path)
    parser.add_argument("--profile-1051", type=Path, help="1051 userdata directory containing users.dat")
    parser.add_argument("--profile-portable", type=Path, help="Portable savedir containing userdata/users.dat")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--check-timeout", action="store_true", help="also run the never-stop fixture on each backend")
    parser.add_argument("--timeout-secs", type=int, default=60)
    args = parser.parse_args()
    backends = ["1051", "portable"] if args.backend == "all" else [args.backend]
    for backend in backends:
        required = ["game_1051", "profile_1051"] if backend == "1051" else ["resources", "profile_portable"]
        for name in required:
            value = getattr(args, name)
            if value is None or not value.exists():
                parser.error(f"--{name.replace('_', '-')} must name an existing path")
            setattr(args, name, value.resolve())
    if args.timeout_secs < 1:
        parser.error("--timeout-secs must be positive")
    root = Path(__file__).resolve().parents[1]
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    output = (args.output_dir or root / "target/live-tests" / stamp).resolve()
    output.mkdir(parents=True, exist_ok=False)
    templates = [getattr(args, "profile_" + b) for b in backends]
    before = {str(p): fingerprint(p) for p in templates}
    (output / "templates-before.json").write_text(json.dumps(before, indent=2), encoding="utf-8")
    results = []
    jobs = []
    try:
        for backend in backends:
            for timeout_case in ([False, True] if args.check_timeout else [False]):
                case = backend + ("-timeout" if timeout_case else "-smoke")
                directory = output / case
                directory.mkdir()
                command = ["cargo", "run", "-p", "rsvz-cli", "--"]
                if backend == "1051":
                    command += ["run-1051", "--game", str(args.game_1051),
                                "--profile-template", str(args.profile_1051)]
                else:
                    command += ["run-portable", "--portable-toolchain", "msvc", "--close-game-on-finish",
                                "--profile-template", str(args.profile_portable)]
                command += ["--dev-script", "live_smoke", "--timeout-secs", str(args.timeout_secs),
                            "--report", str(directory / "report.json"), "--output", str(directory / "artifact.json")]
                if timeout_case:
                    command += ["--script-feature", "never-stop"]
                if backend == "portable":
                    command += ["--", "-resdir", str(args.resources)]
                jobs.append((case, timeout_case, directory, command))
        # Finish every selected build-only gate before starting the first real game.
        for case, _, directory, command in jobs:
            build = command[:6] + ["--build-only"] + command[6:]
            print(f"Building {case}", flush=True)
            with (directory / "build.log").open("w", encoding="utf-8") as log:
                status = subprocess.run(build, cwd=root, stdout=log, stderr=subprocess.STDOUT).returncode
            if status != 0:
                results.append(dict(case=case + "-build", passed=False, exit_code=status, command=build))
                return 1
        for case, timeout_case, directory, command in jobs:
            print(f"Running {case}; log: {directory / 'command.log'}", flush=True)
            started = time.monotonic()
            with (directory / "command.log").open("w", encoding="utf-8") as log:
                status = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT).returncode
            log_text = (directory / "command.log").read_text(encoding="utf-8", errors="replace")
            # No host wire parsing here. Lifecycle assertions belong to the Rust command.
            passed = status == 0 if not timeout_case else status != 0 and "live run timed out" in log_text
            passed = passed and "process reaped:" in log_text
            if not timeout_case:
                passed = passed and "game alive after unload" in log_text and (directory / "artifact.json").is_file()
            results.append(dict(case=case, passed=passed, exit_code=status,
                                elapsed_seconds=time.monotonic() - started, command=command))
            print(f"{case}: {'PASS' if passed else 'FAIL'}", flush=True)
    finally:
        after = {str(p): fingerprint(p) for p in templates}
        unchanged = before == after
        (output / "templates-after.json").write_text(json.dumps(after, indent=2), encoding="utf-8")
        summary = dict(results=results, templates_unchanged=unchanged)
        (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
        print(f"Results: {output / 'summary.json'}", flush=True)
    return 0 if unchanged and results and all(r["passed"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
