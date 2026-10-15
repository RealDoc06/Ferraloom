#!/usr/bin/env python3
"""Acquire the pinned official reference and index its identifiers, never its code."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = json.loads((ROOT / "compatibility/target.json").read_text())
CACHE = ROOT / ".cache/vanilla" / TARGET["version"]


def acquire() -> Path:
    CACHE.mkdir(parents=True, exist_ok=True)
    jar = CACHE / "server.jar"
    if not jar.exists():
        temporary = jar.with_suffix(".download")
        urllib.request.urlretrieve(TARGET["server_url"], temporary)
        temporary.replace(jar)
    digest = hashlib.file_digest(jar.open("rb"), "sha1").hexdigest()
    if digest != TARGET["server_sha1"] or jar.stat().st_size != TARGET["server_size"]:
        raise ValueError(f"Official server checksum/size mismatch: {jar}")
    return jar


def java_command(java: str) -> list[str]:
    return [str(Path(java).resolve()) if os.sep in java else java, "-Xms128M", "-Xmx1024M"]


def inventory(java: str, output: Path) -> None:
    jar = acquire()
    reports = CACHE / "generated"
    subprocess.run(java_command(java) + ["-DbundlerMainClass=net.minecraft.data.Main", "-jar", str(jar),
                   "--reports", "--output", str(reports)], cwd=CACHE, check=True)
    with zipfile.ZipFile(jar) as outer:
        inner_path = CACHE / "minecraft.jar"
        inner_path.write_bytes(outer.read(f"META-INF/versions/{TARGET['version']}/server-{TARGET['version']}.jar"))
    resources: dict[str, list[str]] = {}
    with zipfile.ZipFile(inner_path) as inner:
        version = json.loads(inner.read("version.json"))
        if version["protocol_version"] != TARGET["protocol"]:
            raise ValueError("Protocol target differs from official artifact")
        for name in sorted(inner.namelist()):
            if not name.startswith("data/minecraft/") or name.endswith("/"):
                continue
            category, _, path = name.removeprefix("data/minecraft/").partition("/")
            resources.setdefault(category, []).append(path)
    registries = json.loads((reports / "reports/registries.json").read_text())
    registry_ids = {name: sorted(value["entries"]) for name, value in sorted(registries.items())}
    blocks = json.loads((reports / "reports/blocks.json").read_text())
    commands = json.loads((reports / "reports/commands.json").read_text())
    result = {
        "version": TARGET["version"], "protocol": TARGET["protocol"],
        "server_sha1": TARGET["server_sha1"],
        "scope": "Identifier inventory only. Presence does not prove behavioral compatibility.",
        "world_version": version["world_version"], "pack_version": version["pack_version"],
        "registry_counts": {key: len(value) for key, value in registry_ids.items()},
        "resource_counts": {key: len(value) for key, value in resources.items()},
        "block_state_count": sum(len(block["states"]) for block in blocks.values()),
        "command_roots": sorted(commands["children"]),
        "registries": registry_ids, "datapack_resources": resources,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Indexed {len(registry_ids)} registries, {len(blocks)} blocks, "
          f"{result['block_state_count']} states, {sum(map(len, resources.values()))} datapack resources")


def compare_registry(rust_path: Path, output: Path, rust_binary: Path | None = None) -> bool:
    """Compare every runtime block/state ID, property tuple and default to vanilla."""
    acquire()
    binary_digest = None
    if rust_binary is not None:
        binary_digest = hashlib.file_digest(rust_binary.open("rb"), "sha256").hexdigest()
        subprocess.run([str(rust_binary.resolve()), "--export-registry", str(rust_path.resolve())], check=True)
        if binary_digest != hashlib.file_digest(rust_binary.open("rb"), "sha256").hexdigest():
            raise ValueError("Rust binary changed during registry export; use a stable copy")
    reports = CACHE / "generated/reports"
    reference = json.loads((reports / "blocks.json").read_text())
    block_ids = json.loads((reports / "registries.json").read_text())["minecraft:block"]["entries"]
    rust = json.loads(rust_path.read_text())
    if rust.get("version") != TARGET["version"] or rust.get("protocol") != TARGET["protocol"]:
        raise ValueError("Rust registry export targets a different Minecraft version")
    blocks = rust["blocks"]
    mismatches = []
    missing, extra = sorted(reference.keys() - blocks.keys()), sorted(blocks.keys() - reference.keys())
    for name in sorted(reference.keys() & blocks.keys()):
        expected, actual = reference[name], blocks[name]
        if actual["id"] != block_ids[name]["protocol_id"]:
            mismatches.append({"block": name, "field": "block_id", "vanilla": block_ids[name]["protocol_id"], "rust": actual["id"]})
        normalize = lambda states: sorted((state["id"], bool(state.get("default", False)), tuple(sorted(state.get("properties", {}).items()))) for state in states)
        if normalize(expected["states"]) != normalize(actual["states"]):
            mismatches.append({"block": name, "field": "state_ids_defaults_or_properties", "vanilla_count": len(expected["states"]), "rust_count": len(actual["states"])})
    passed = not (missing or extra or mismatches)
    result = {
        "target_version": TARGET["version"], "protocol": TARGET["protocol"], "server_sha1": TARGET["server_sha1"],
        "rust_registry_sha256": hashlib.file_digest(rust_path.open("rb"), "sha256").hexdigest(),
        "rust_binary_sha256": binary_digest,
        "official_block_report_sha256": hashlib.file_digest((reports / "blocks.json").open("rb"), "sha256").hexdigest(),
        "result": "passed" if passed else "failed",
        "scope": "Complete block IDs, state IDs, property tuples and defaults. Does not test block behavior.",
        "vanilla_blocks": len(reference), "rust_blocks": len(blocks),
        "vanilla_states": sum(len(block["states"]) for block in reference.values()),
        "rust_states": sum(len(block["states"]) for block in blocks.values()),
        "missing_blocks": missing, "extra_blocks": extra, "mismatches": mismatches,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Registry {result['result']}: {result['vanilla_blocks']} blocks, {result['vanilla_states']} states; {len(missing)} missing, {len(extra)} extra, {len(mismatches)} mismatches")
    return passed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["fetch", "inventory", "compare-registry"])
    parser.add_argument("--java", default=os.environ.get("JAVA", "java"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--rust-registry", type=Path, default=ROOT / ".cache/rust-registry.json")
    parser.add_argument("--rust-binary", type=Path, help="Run this executable's registry exporter and record its SHA-256")
    args = parser.parse_args()
    if args.action == "fetch":
        print(acquire())
    elif args.action == "inventory":
        inventory(args.java, args.output or ROOT / "compatibility/vanilla-inventory.json")
    elif not compare_registry(args.rust_registry, args.output or ROOT / "compatibility/results/registry.json", args.rust_binary):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
