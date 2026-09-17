"""Build and run native capability/lifecycle checks against the matching Portable SDK."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

p = argparse.ArgumentParser()
for name in ["sdk", "game", "resources", "profile", "output"]:
    p.add_argument("--" + name, type=Path, required=True)
p.add_argument("--abi", choices=["msvc", "gnu"], required=True)
args = p.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
crate = Path(__file__).resolve().parents[1]
root = crate.parents[3]
portable = root.parent / "PvZ-Portable"
llvm = Path("C:/Program Files/LLVM/bin")
includes = [crate / "cpp", args.sdk / "src", args.sdk / "src/SexyAppFramework",
            args.sdk / "src/SexyAppFramework/sound/SDL-Mixer-X/include"]
includes.extend(Path(value) for value in (args.sdk / "sdl-include.txt").read_text().split(";") if value)
env = dict(os.environ, PATH="C:/msys64/ucrt64/bin;" + os.environ["PATH"])
objects = []
for source in [crate / "cpp/bridge.cpp", crate / "tests/native_plugin.cpp"]:
    obj = args.output / (source.stem + ".obj")
    if args.abi == "msvc":
        command = [str(llvm / "clang-cl.exe"), "/std:c++20", "/utf-8", "/MD", "/EHsc", "/c", "/O2", "/Fo" + str(obj)]
    else:
        command = [str(llvm / "clang++.exe"), "--target=x86_64-w64-windows-gnu", "--sysroot=C:/msys64/ucrt64",
                   "-std=c++20", "-c", "-O2", "-o", str(obj)]
    command.extend("-I" + str(path) for path in includes)
    command.append(str(source))
    with (args.output / (source.stem + ".build.log")).open("w") as log:
        subprocess.run(command, env=env, check=True, stdout=log, stderr=log)
    objects.append(str(obj))
dll = args.output / "native-checks.dll"
if args.abi == "msvc":
    command = [str(llvm / "clang-cl.exe"), "/LD", *objects, "/Fe" + str(dll), "/link",
               "/LIBPATH:" + str(args.sdk / "lib"), "pvz-portable.lib"]
else:
    command = [str(llvm / "clang++.exe"), "--target=x86_64-w64-windows-gnu", "--sysroot=C:/msys64/ucrt64",
               "-shared", *objects, "-fuse-ld=lld", "-L" + str(args.sdk / "lib"), "-lpvz-portable", "-o", str(dll)]
with (args.output / "link.log").open("w") as log:
    subprocess.run(command, env=env, check=True, stdout=log, stderr=log)
subprocess.run([sys.executable, str(portable / "tests/plugin/run_smoke.py"), "--game", str(args.game),
                "--plugin", str(dll), "--resources", str(args.resources), "--profile", str(args.profile),
                "--output", str(args.output / "result.json"), "--expect",
                "attach,initialize,validated,first-update,profile-input,audio,pause,dance,cob-matrix,event-suppression,sink-lifecycle,garden-hidden,passed,shutdown,unload"],
               env=env, check=True)
print("native capabilities and lifecycle PASS", flush=True)
