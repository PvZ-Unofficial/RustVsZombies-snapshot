"""Update Portable's checked-in export lists from this bridge's real references.

This developer tool is not part of Portable's standalone build.
"""
from pathlib import Path
import subprocess
import ctypes

root = Path(__file__).resolve().parents[4]
portable = root.parent / "PvZ-Portable"
llvm = Path("C:/Program Files/LLVM/bin")
output = root / "target/pvzp-sdk-exports"
output.mkdir(parents=True, exist_ok=True)
includes = [portable / "src", portable / "src/SexyAppFramework",
            portable / "src/SexyAppFramework/sound/SDL-Mixer-X/include"]
for abi in ["msvc", "gnu"]:
    obj = output / f"bridge-{abi}.obj"
    if abi == "msvc":
        command = [str(llvm / "clang-cl.exe"), "/c", "/std:c++20", "/utf-8", "/MD", "/EHsc", "/O2", "/Fo" + str(obj)]
        sdl = root / "target/rsvz/vcpkg/msvc/x64-windows/include/SDL2"
    else:
        command = [str(llvm / "clang++.exe"), "--target=x86_64-w64-windows-gnu", "--sysroot=C:/msys64/ucrt64",
                   "-std=c++20", "-c", "-O2", "-o", str(obj)]
        sdl = Path("C:/msys64/ucrt64/include/SDL2")
    command.extend("-I" + str(path) for path in [*includes, sdl])
    command.append(str(Path(__file__).parent / "cpp/bridge.cpp"))
    result = subprocess.run(command, capture_output=True, text=True, errors="replace")
    (output / f"bridge-{abi}-compile.log").write_text(result.stdout + result.stderr, encoding="utf-8")
    result.check_returncode()
    nm = str(llvm / "llvm-nm.exe")
    raw = subprocess.check_output([nm, "--undefined-only", str(obj)], text=True).splitlines()
    raw_symbols = [line.split()[-1].removeprefix("__imp_") for line in raw]
    if abi == "gnu":
        names = subprocess.check_output([str(llvm / "llvm-cxxfilt.exe"), *raw_symbols], text=True).splitlines()
        decoded = dict(zip(raw_symbols, names))
    else:
        candidates = [symbol for symbol in raw_symbols if symbol.startswith("?")]
        decoded = {}
        undecorate = ctypes.WinDLL("dbghelp").UnDecorateSymbolName
        undecorate.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint32, ctypes.c_uint32]
        undecorate.restype = ctypes.c_uint32
        for symbol in candidates:
            buffer = ctypes.create_string_buffer(8192)
            if undecorate(symbol.encode(), buffer, len(buffer), 0):
                decoded[symbol] = buffer.value.decode()
    symbols = []
    for symbol in raw_symbols:
        name = decoded.get(symbol, "")
        if "(" not in name or any(part in name for part in ["std::", "__cxxabiv1", "__gnu_cxx", "operator ", "type_info::"]):
            continue
        if symbol.startswith(("?", "_Z")):
            symbols.append(symbol)
    symbols = sorted(set(symbols))
    (portable / f"CMake/native-{abi}.def").write_text("EXPORTS\n" + "".join("    " + name + "\n" for name in symbols))
    print(f"{abi}: {len(symbols)} referenced native functions; imported data use their declarations")
