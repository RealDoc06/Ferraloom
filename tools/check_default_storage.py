#!/usr/bin/env python3
"""Check one default-LZ4 Ferraloom chunk loads unchanged in the pinned Java server.

Fixtures and reports stay under .cache; both servers bind only to this host's
loopback. This checks one saved overworld marker, not complete storage parity.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import zipfile

from oracle import ROOT, TARGET, acquire, java_command
from parity.suite import Server, check_exit, check_predicate, data_version, pin_binary, wait_loaded
from parity.worlds import MAX_NBT_BYTES, SECTOR_BYTES, decode_nbt


ZLIB_FOOTER = '[world.chunk]\ntype = "anvil"\n[world.chunk.compression]\nalgorithm = "ZLib"\nlevel = 6\n'
MARKER = "if block 3 200 11 amethyst_block"
WORLD_ERRORS = re.compile(
    r"\bERROR\b|Failed to parse saved data|Couldn't load chunk|"
    r"Failed to (?:load|read|write|save|parse).*?(?:chunk|region|world|data)",
    re.IGNORECASE,
)


def digest(path: Path, algorithm: str = "sha256") -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, algorithm).hexdigest()


def default_storage_config(server: Server) -> None:
    """Remove exactly the shared fixture's storage override, leaving engine defaults."""
    server.configure()
    path = server.directory / "pumpkin.toml"
    config = path.read_text()
    if not config.endswith(ZLIB_FOOTER) or config.count(ZLIB_FOOTER) != 1:
        raise ValueError("Shared Server.configure storage footer changed; review this check")
    config = config.removesuffix(ZLIB_FOOTER)
    if "[world.chunk" in config:
        raise ValueError("Fixture still overrides default chunk storage")
    path.write_text(config)


def chunk_record(world: Path) -> tuple[Path, int, bytes]:
    """Read the actual chunk (0, 0) Anvil compression byte and bounded payload."""
    directories = [path for path in (
        world / "region", world / "dimensions/minecraft/overworld/region",
    ) if path.is_dir()]
    if len(directories) != 1:
        raise ValueError("Expected exactly one overworld region directory")
    region = directories[0] / "r.0.0.mca"
    with region.open("rb") as handle:
        header = handle.read(SECTOR_BYTES * 2)
        if len(header) != SECTOR_BYTES * 2:
            raise ValueError("Truncated Anvil header")
        location = int.from_bytes(header[:4], "big")
        sector, count = location >> 8, location & 255
        if sector < 2 or count == 0 or (sector + count) * SECTOR_BYTES > region.stat().st_size:
            raise ValueError("Invalid chunk (0, 0) sector allocation")
        handle.seek(sector * SECTOR_BYTES)
        length = int.from_bytes(handle.read(4), "big")
        if not 2 <= length <= count * SECTOR_BYTES - 4:
            raise ValueError("Invalid Anvil record length")
        record = handle.read(length)
        if len(record) != length:
            raise ValueError("Truncated Anvil record")
    return region, record[0], record[1:]


def decode_lz4(java: str, official: Path, payload: bytes, directory: Path) -> tuple[dict, dict]:
    """Use the reference distribution's checksum-validating LZ4 decoder, not a new codec."""
    directory.mkdir()
    with zipfile.ZipFile(official) as archive:
        libraries = [line.split("\t") for line in archive.read("META-INF/libraries.list").decode().splitlines()]
        matching = [entry for entry in libraries if len(entry) == 3 and ":lz4-java:" in entry[1]]
        if len(matching) != 1:
            raise ValueError("Expected one bundled official LZ4 library")
        expected, coordinate, relative = matching[0]
        library_bytes = archive.read("META-INF/libraries/" + relative)
    actual = hashlib.sha256(library_bytes).hexdigest()
    if actual != expected:
        raise ValueError("Official bundled LZ4 library SHA-256 mismatch")
    library = directory / "lz4.jar"
    library.write_bytes(library_bytes)
    compressed = directory / "chunk.lz4"
    compressed.write_bytes(payload)
    helper = directory / "DecodeLz4.java"
    helper.write_text('''import java.io.*;
import java.nio.file.*;
import net.jpountz.lz4.LZ4BlockInputStream;

class DecodeLz4 {
    public static void main(String[] args) throws Exception {
        try (var input = new LZ4BlockInputStream(Files.newInputStream(Path.of(args[0])))) {
            byte[] nbt = input.readNBytes(16 * 1024 * 1024 + 1);
            if (nbt.length > 16 * 1024 * 1024) throw new IOException("NBT exceeds byte limit");
            System.out.write(nbt);
        }
    }
}
''')
    decoded = subprocess.run(
        java_command(java) + ["--class-path", str(library), str(helper), str(compressed)],
        cwd=directory, capture_output=True, timeout=30,
    )
    if decoded.returncode != 0:
        raise RuntimeError(f"Official LZ4 decoder failed: {decoded.stderr.decode(errors='replace')[-4000:]}")
    if len(decoded.stdout) > MAX_NBT_BYTES:
        raise ValueError("Decompressed chunk exceeds NBT limit")
    return decode_nbt(decoded.stdout), {"coordinate": coordinate, "sha256": actual}


def stop_clean(server: Server, report: dict) -> None:
    code = server.stop()
    report[server.kind + "_shutdown"] = {"exit_code": code, "forced": server.forced_stop}
    check_exit(code, server.forced_stop)


def require_clean_log(server: Server) -> None:
    errors = [line for line in server.log_path.read_text().splitlines() if WORLD_ERRORS.search(line)]
    if errors:
        raise AssertionError(f"{server.kind} reported load/save errors: {errors[:5]}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accept-eula", action="store_true", help="Explicitly accept https://aka.ms/MinecraftEULA for this isolated run")
    parser.add_argument("--java", required=True, help="Java 25 executable for the pinned official server")
    parser.add_argument("--rust-binary", type=Path, required=True)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, default=ROOT / ".cache/default-storage.json")
    args = parser.parse_args()
    if not args.accept_eula:
        parser.error("Reference execution requires explicit --accept-eula")
    if args.workers < 1 or args.startup_timeout <= 0:
        parser.error("Workers and startup timeout must be positive")
    if not args.rust_binary.is_file() or not os.access(args.rust_binary, os.X_OK):
        parser.error(f"Rust server is not an executable file: {args.rust_binary}")
    cache = (ROOT / ".cache").resolve()
    args.output = args.output.resolve()
    if not args.output.is_relative_to(cache):
        parser.error("Keep this diagnostic's report under the repository .cache directory")
    runs = cache / "runs"
    runs.mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix="default-storage-", dir=runs))
    report = {
        "schema_version": 1, "created_at": datetime.now(timezone.utc).isoformat(),
        "result": "failed", "target_version": TARGET["version"], "protocol": TARGET["protocol"],
        "scope": "One default-LZ4 Rust overworld chunk and marker loaded by vanilla, without conversion. No other storage, entities, dimensions, crash durability or full parity claim.",
        "fixture": str(run_dir), "transport": "isolated offline 127.0.0.1", "workers": args.workers,
        "tool_sha256": digest(Path(__file__)), "observations": {},
    }
    servers: list[Server] = []
    began = time.monotonic()
    try:
        official = acquire()  # Enforces the pinned official artifact's SHA-1 and size.
        report["server_sha1"] = digest(official, "sha1")
        report["server_sha256"] = digest(official)
        source_digest = digest(args.rust_binary)
        args.rust_binary = pin_binary(args.rust_binary, run_dir)
        report["rust_binary_sha256"] = digest(args.rust_binary)
        if report["rust_binary_sha256"] != source_digest:
            raise ValueError("Rust binary changed while pinning the executable")
        observations = report["observations"]
        rust = Server("rust", run_dir / "rust", args)
        servers.append(rust)
        default_storage_config(rust)
        report["rust_config_sha256"] = digest(rust.directory / "pumpkin.toml")
        print(f"Default-LZ4 source on this host's loopback: {rust.directory}", flush=True)
        rust.start()
        rust.command("forceload add 0 0")
        wait_loaded(rust)
        rust.command("setblock 3 200 11 amethyst_block")
        check_predicate(rust, MARKER)
        observations["rust_marker"] = MARKER
        rust.save_flush()
        stop_clean(rust, observations)
        require_clean_log(rust)

        source_world = rust.directory / "world"
        region, compression, payload = chunk_record(source_world)
        observations["source_compression_byte"] = compression
        if compression != 4:
            raise AssertionError(f"Default storage is not LZ4 (Anvil byte 4): {compression}")
        chunk, decoder = decode_lz4(args.java, official, payload, run_dir / "decoder")
        report["lz4_decoder"] = decoder
        observations["source_chunk_data_version"] = chunk.get("DataVersion")
        observations["source_level_data_version"] = data_version(source_world)
        observations["source_region_sha256"] = digest(region)
        if (chunk.get("xPos"), chunk.get("zPos")) != (0, 0):
            raise AssertionError("Source region entry contains the wrong chunk")
        if observations["source_chunk_data_version"] != 5023 or observations["source_level_data_version"] != 5023:
            raise AssertionError("Source chunk/level DataVersion differs from the pinned 26.3 format 5023")

        vanilla = Server("vanilla", run_dir / "vanilla", args)
        servers.append(vanilla)
        vanilla.configure()
        shutil.copytree(source_world, vanilla.directory / "world", dirs_exist_ok=True)
        observations["copied_region_sha256"] = digest(vanilla.directory / "world" / region.relative_to(source_world))
        if observations["copied_region_sha256"] != observations["source_region_sha256"]:
            raise AssertionError("Source Anvil region changed while copying")
        print(f"Vanilla migration destination on this host's loopback: {vanilla.directory}", flush=True)
        vanilla.start()
        vanilla.command("forceload add 0 0")
        wait_loaded(vanilla)
        check_predicate(vanilla, MARKER)
        observations["vanilla_marker"] = MARKER
        vanilla.save_flush()
        stop_clean(vanilla, observations)
        require_clean_log(vanilla)
        observations["destination_load_save_errors"] = 0
        observations["destination_level_data_version"] = data_version(vanilla.directory / "world")
        if observations["destination_level_data_version"] != 5023:
            raise AssertionError("Vanilla migration changed the target world DataVersion")
        if digest(args.rust_binary) != report["rust_binary_sha256"] or digest(acquire()) != report["server_sha256"]:
            raise ValueError("A pinned executable changed during the check")
        report["result"] = "passed"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        for server in reversed(servers):
            try:
                if server.process is not None:
                    stop_clean(server, report["observations"])
            except Exception as error:
                report["result"] = "failed"
                report.setdefault("cleanup_errors", []).append(f"{server.kind}: {type(error).__name__}: {error}")
        report["duration_seconds"] = round(time.monotonic() - began, 3)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2).replace(str(ROOT), "$REPO") + "\n")
    print(f"Default storage {report['result']}; report: {args.output}; fixtures: {run_dir}")
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
