#!/usr/bin/env python3
"""Audit PE dynamic imports of installed M5 binaries (not static linkage).

Requires pefile==2024.8.26. Output is JSON-only for GitHub Actions evidence.
The output is NOT proof that DLLs can be resolved on a clean Windows system.
"""
import argparse
import json
from pathlib import Path

import pefile


def imported_names(entries):
    result = set()
    for entry in entries:
        if not entry.dll:
            raise ValueError("PE import descriptor without a DLL name")
        result.add(entry.dll.decode("ascii").lower())
    return sorted(result)


def audit_one(path):
    pe = pefile.PE(str(path), fast_load=True)
    try:
        if pe.FILE_HEADER.Machine != 0x8664:
            raise ValueError(f"{path.name}: expected Windows x64 PE machine 0x8664")
        pe.parse_data_directories(
            directories=[
                pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"],
                pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT"],
            ]
        )
        direct = imported_names(getattr(pe, "DIRECTORY_ENTRY_IMPORT", []))
        delayed = imported_names(getattr(pe, "DIRECTORY_ENTRY_DELAY_IMPORT", []))
        return {
            "file": path.name,
            "pe_machine": "0x8664",
            "direct_dll_imports": direct,
            "delay_load_dll_imports": delayed,
        }
    finally:
        pe.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs=3, type=Path)
    args = parser.parse_args()
    expected = {"hongguorust.exe", "libmpv-2.dll", "vulkan-1.dll"}
    found = {p.name.lower() for p in args.binaries}
    if found != expected:
        parser.error(f"expected exactly {sorted(expected)}, got {sorted(found)}")
    for path in args.binaries:
        if not path.is_file():
            parser.error(f"not a file: {path}")
    results = [audit_one(p) for p in args.binaries]
    print(json.dumps({
        "schema_version": 1,
        "scope": "PE import and delay-load tables only; excludes static linkage and runtime LoadLibrary",
        "clean_windows_dependency_resolution_verified": False,
        "binaries": results,
    }, sort_keys=True))


if __name__ == "__main__":
    main()
