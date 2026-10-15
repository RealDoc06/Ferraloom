#!/usr/bin/env python3
"""Compare isolated official Java furnace saves and Rust continuations.

Creates a fresh 237-tick Java oracle, then fresh Java/Rust main and lava copies.
Reports timer/speed/fuel remainder fixtures, not full furnace, inventory, XP,
name, lock or Minecraft parity. Does not publish compatibility ledger results.
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
import signal
import socket
import subprocess
import sys
import tempfile
import time
import traceback

sys.dont_write_bytecode = True
from oracle import ROOT, TARGET, acquire
from parity import furnaces, suite

WORLD_ERRORS = re.compile(r"\bERROR\b|Failed to parse saved data|Couldn't load chunk|Failed to (?:load|read|write|save|parse).*?(?:chunk|region|world|data)", re.IGNORECASE)


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def canonical_hash(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def world_manifest(world: Path) -> dict:
    manifest = {}
    for path in sorted(world.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Fixture contains an unexpected symlink: {path}")
        if path.is_file():
            manifest[str(path.relative_to(world))] = {"bytes": path.stat().st_size, "sha256": furnaces.digest(path)}
    if not manifest:
        raise ValueError("Fixture world is empty")
    return manifest


class SourceMutationError(RuntimeError):
    pass


def verify_source(report: dict, source: dict, stage: str) -> dict:
    manifest = world_manifest(Path(source["world_path"]))
    record = {"stage": stage, "manifest_sha256": canonical_hash(manifest), "file_count": len(manifest)}
    record["passed"] = record["manifest_sha256"] == source["world_manifest_sha256"]
    report["source_integrity_checks"].append(record)
    if not record["passed"]:
        raise SourceMutationError(f"Java source world changed at {stage}")
    return manifest


def source_state() -> dict:
    state = suite.git_state()
    # Record hashes, never patch text or an inventory of ignored local files.
    patch = subprocess.run(["git", "diff", "--binary", "HEAD"], cwd=ROOT, capture_output=True, check=True).stdout
    status = subprocess.run(["git", "status", "--porcelain=v1", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    state.update(tracked_patch_sha256=hashlib.sha256(patch).hexdigest(), status_sha256=hashlib.sha256(status).hexdigest())
    paths = subprocess.run(["git", "ls-files", "--others", "--exclude-standard", "-z", "--", "src", "vendor/pumpkin/crates", "tools/check_furnaces.py", "tools/parity/furnaces.py"],
                           cwd=ROOT, capture_output=True, check=True).stdout.split(b"\0")
    state["untracked_source_sha256"] = {
        os.fsdecode(path): furnaces.digest(ROOT / os.fsdecode(path)) for path in paths
        if path and (os.fsdecode(path).endswith(".rs") or os.fsdecode(path) in ("tools/check_furnaces.py", "tools/parity/furnaces.py"))
    }
    return state


def harness_manifest() -> dict:
    return {**suite.harness_manifest(), "tools/check_furnaces.py": furnaces.digest(Path(__file__).resolve())}


class LoggedServer(suite.Server):
    def command(self, command: str, timeout: float = 15) -> list[str]:
        record = {"command": command, "started": now()}
        try:
            output = super().command(command, timeout)
            record["output"] = output
            return output
        except Exception as error:
            record["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            record["finished"] = now()
            with (self.directory / "commands.jsonl").open("a") as log:
                log.write(json.dumps(record) + "\n")

    def checked(self, command: str, timeout: float = 15) -> list[str]:
        output = self.command(command, timeout)
        if any(re.search(r"Unknown|Incorrect argument|Cannot|No (?:block|entity)|Failed to", line) for line in output):
            raise RuntimeError(f"Fixture command failed: {command}: {output}")
        return output


def configure(server: LoggedServer, record: dict) -> None:
    server.configure()
    record["configs"] = {name: {"sha256": furnaces.digest(server.directory / name), "text": (server.directory / name).read_text()}
                         for name in ("eula.txt", "server.properties", "pumpkin.toml") if (server.directory / name).is_file()}


def stop_server(server: LoggedServer, record: dict) -> None:
    record["stop_started"] = now()
    try:
        code = server.stop()
    except Exception as error:
        record["shutdown_error"] = f"{type(error).__name__}: {error}"
        server.forced_stop = True
        if server.process is not None and server.process.poll() is None:
            try:
                server.signal(signal.SIGKILL)
                server.process.wait(timeout=10)
            except Exception as cleanup:
                record["cleanup_error"] = f"{type(cleanup).__name__}: {cleanup}"
        code = server.process.returncode if server.process else None
    record["finished"] = now()
    record["exit"] = {"code": code, "forced_stop": server.forced_stop,
                      "process_reaped": server.process is not None and server.process.poll() is not None}
    record["exit"]["passed"] = code == 0 and not server.forced_stop and record["exit"]["process_reaped"]
    record["stop_command"] = "stop"
    if server.process is not None:
        record.update(pid=server.process.pid, argv=server.process.args)
    record.update(console_path=str(server.log_path), commands_path=str(server.directory / "commands.jsonl"))
    if server.log_path.exists():
        lines = server.log_path.read_text().splitlines()
        record.update(console_sha256=furnaces.digest(server.log_path), shutdown_tail=lines[-18:],
                      load_save_errors=[line for line in lines if WORLD_ERRORS.search(line)])


def freeze_and_load(server: LoggedServer, record: dict) -> None:
    output = server.command("tick freeze")
    if not any(line.endswith("The game is frozen") for line in output):
        raise RuntimeError(f"Freeze was not acknowledged: {output}")
    before = suite.game_time(server)
    server.checked("forceload add 4096 0", timeout=60)
    suite.wait_loaded(server, x=4096, z=0)
    furnaces.require_frozen(server)
    after = suite.game_time(server)
    record["fixture_loading_game_time"] = {"before": before, "after": after}
    if after != before:
        raise RuntimeError("Game advanced while loading the frozen fixture")


def unforce_and_save(server: LoggedServer, record: dict) -> None:
    before = suite.game_time(server)
    server.checked("forceload remove 4096 0")
    record["final_save_game_time"] = furnaces.save_frozen(server)
    if record["final_save_game_time"]["after"] != before:
        raise RuntimeError("Game advanced while unforcing the frozen fixture")


def generate_reference(run: Path, args: argparse.Namespace) -> dict:
    server = LoggedServer("vanilla", run / "java-source", args)
    source = {"runtime": "vanilla", "directory": str(server.directory), "world_path": str(server.directory / "world"),
              "loopback_port": server.port, "started": now(), "result": "failed"}
    try:
        configure(server, source)
        server.start()
        freeze_and_load(server, source)
        for name, item in (("furnace", "raw_iron"), ("blast_furnace", "raw_iron"), ("smoker", "porkchop")):
            position = " ".join(map(str, furnaces.POSITIONS[name]))
            for command in (f"setblock {position} minecraft:{name}", f"item replace block {position} container.0 with minecraft:{item} 16",
                            f"item replace block {position} container.1 with minecraft:coal 1"):
                server.checked(command)
        source["timing"] = furnaces.step_exact(server, 237)
        if source["timing"]["before"] != source["fixture_loading_game_time"]["after"]:
            raise RuntimeError("Game advanced while seeding the frozen cooking fixtures")
        position = " ".join(map(str, furnaces.POSITIONS["wet_sponge_lava_unstarted"]))
        for command in (f"setblock {position} minecraft:furnace", f"item replace block {position} container.0 with minecraft:wet_sponge 16",
                        f"item replace block {position} container.1 with minecraft:lava_bucket 1"):
            server.checked(command)
        if suite.game_time(server) != source["timing"]["after"]:
            raise RuntimeError("Game advanced while seeding the unstarted lava fixture")
        unforce_and_save(server, source)
    except Exception as error:
        source.update(error=f"{type(error).__name__}: {error}", traceback=traceback.format_exc())
    finally:
        stop_server(server, source)
    try:
        source["saved"] = furnaces.read_saved(Path(source["world_path"]))
        source["verification"] = furnaces.compare_snapshot(source["saved"], furnaces.expected_reference())
        if source["saved"]["chunk_data_version"] != 5023:
            raise ValueError("Fixture DataVersion differs from official 26.3 format 5023")
        manifest = world_manifest(Path(source["world_path"]))
        manifest_path = run / "source-world-manifest.json"
        write_json(manifest_path, manifest)
        source.update(world_manifest_path=str(manifest_path), world_manifest_sha256=canonical_hash(manifest))
        if "error" not in source and source["exit"]["passed"] and not source.get("load_save_errors") and source["verification"]["result"] == "passed":
            source["result"] = "passed"
    except Exception as error:
        source["inspection_error"] = f"{type(error).__name__}: {error}"
    write_json(server.directory / "source-result.json", source)
    return source


def run_case(kind: str, mode: str, run: Path, args: argparse.Namespace, source: dict, report: dict) -> dict:
    server = LoggedServer(kind, run / f"{kind}-{mode}", args)
    case = {"runtime": kind, "mode": mode, "directory": str(server.directory), "loopback_port": server.port,
            "started": now(), "phases": [], "result": "failed"}
    try:
        before = verify_source(report, source, f"before clone {kind}-{mode}")
        try:
            configure(server, case)
            shutil.copytree(Path(source["world_path"]), server.directory / "world", dirs_exist_ok=True)
            copied = world_manifest(server.directory / "world")
            case["copied_world_manifest_sha256"] = canonical_hash(copied)
            if copied != before:
                raise SourceMutationError("Clone differs from verified Java source before startup")
        finally:
            verify_source(report, source, f"after clone {kind}-{mode}")
        server.start()
        freeze_and_load(server, case)
        schedule = (("zero-step", 0, 0), ("after-63", 63, 63), ("after-163", 100, 163)) if mode == "main" else (("lava-after-1", 1, 1), ("lava-after-200", 199, 200))
        for name, advance, elapsed in schedule:
            phase = {"name": name, "requested_advance": advance, "cumulative_fixture_ticks": elapsed, "started": now(), "result": "failed"}
            case["phases"].append(phase)
            print(f"{kind}-{mode}: {name}", flush=True)
            try:
                phase["timing"] = furnaces.step_exact(server, advance)
                phase["expected"] = furnaces.expected_continuation(source["saved"], mode, elapsed)
                phase["save_game_time"] = furnaces.save_frozen(server)
                saved = furnaces.read_saved(server.directory / "world")
                snapshot = server.directory / f"{name}-snapshot.json"
                write_json(snapshot, saved)
                phase.update(snapshot_path=str(snapshot), snapshot_sha256=furnaces.digest(snapshot))
                phase.update(furnaces.compare_snapshot(saved, phase["expected"]))
            except Exception as error:
                phase.update(error=f"{type(error).__name__}: {error}", traceback=traceback.format_exc())
            phase["finished"] = now()
            write_json(server.directory / f"{name}-result.json", phase)
            # Preserve later observations after semantic/inspection failures.
            # A dead process cannot provide meaningful continuation evidence.
            if server.process is None or server.process.poll() is not None:
                break
        unforce_and_save(server, case)
    except SourceMutationError as error:
        case["source_integrity_error"] = str(error)
    except Exception as error:
        case.update(error=f"{type(error).__name__}: {error}", traceback=traceback.format_exc())
    finally:
        stop_server(server, case)
        try:
            verify_source(report, source, f"after execution {kind}-{mode}")
        except Exception as error:
            case["source_integrity_error"] = f"{type(error).__name__}: {error}"
        if case["exit"]["passed"] and "error" not in case and "source_integrity_error" not in case and not case.get("load_save_errors") and case["phases"] and all(phase["result"] == "passed" for phase in case["phases"]):
            case["result"] = "passed"
        write_json(server.directory / "case-result.json", case)
    return case


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accept-eula", action="store_true", help="Explicitly accept https://aka.ms/MinecraftEULA for these isolated Java runs")
    parser.add_argument("--java", required=True, help="Java 25 executable, as a path or PATH command")
    parser.add_argument("--rust-binary", type=Path, required=True, help="Already built Ferraloom executable; copied before any server runs")
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, default=ROOT / ".cache/furnaces.json")
    args = parser.parse_args()
    if not args.accept_eula:
        parser.error("Reference execution requires explicit --accept-eula")
    executable = shutil.which(args.java)
    if executable is None or not args.rust_binary.is_file() or not os.access(args.rust_binary, os.X_OK):
        parser.error("--java and --rust-binary must name executable files")
    args.java, args.rust_binary = str(Path(executable).resolve()), args.rust_binary.resolve()
    if args.workers < 1 or args.startup_timeout <= 0:
        parser.error("Workers and startup timeout must be positive")
    cache = ROOT.resolve() / ".cache"
    args.output = args.output.resolve()
    runs = cache / "runs"
    if cache.resolve() != cache or not args.output.is_relative_to(cache) or not runs.resolve().is_relative_to(cache):
        parser.error("Report and fixtures must remain inside the repository's real .cache directory")
    if args.output == cache or args.output.is_dir():
        parser.error("--output must be a report file under .cache")
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="furnaces-", dir=runs))
    began = time.monotonic()
    report = {"schema_version": 1, "kind": "furnace_continuation_diagnostic", "started": now(), "host": socket.gethostname(),
              "run_directory": str(run), "transport": "fresh sequential offline 127.0.0.1 processes", "target": TARGET,
              "scope": "Timer/speed/fuel remainder fixtures only, not full furnace/inventory/XP/name/lock/Minecraft parity.",
              "comparison": "Semantic values and canonical numeric NBT types are separate. Java accepts NumericTag widths such as Short; a different canonical type alone is not a semantic failure.",
              "publishes_compatibility_results": False, "source_integrity_checks": [], "cases": [], "result": "failed"}
    print(f"Furnace report: {args.output}; isolated fixtures: {run}", flush=True)
    try:
        report.update(source_code=source_state(), harness_manifest=harness_manifest())
        original = args.rust_binary
        before = furnaces.digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        pinned, after = furnaces.digest(args.rust_binary), furnaces.digest(original)
        report["rust_binary"] = {"source_path": str(original), "source_sha256_before": before, "source_sha256_after": after,
                                 "pinned_path": str(args.rust_binary), "pinned_sha256": pinned, "bytes": args.rust_binary.stat().st_size}
        if before != pinned or after != pinned:
            raise ValueError("Rust binary changed while pinning; no server was started")
        official = acquire()
        report["official_jar"] = {"path": str(official), "sha1": furnaces.digest(official, "sha1"), "sha256": furnaces.digest(official), "bytes": official.stat().st_size}
        version = subprocess.run([args.java, "-version"], capture_output=True, text=True, timeout=15, check=True)
        report["java"] = {"path": args.java, "sha256": furnaces.digest(Path(args.java)), "version": version.stdout + version.stderr}
        source = generate_reference(run, args)
        report["source_fixture"] = source
        if source["result"] != "passed":
            raise ValueError("Fresh Java source failed validation; refusing to use it as the oracle")
        verify_source(report, source, "initial verified source")
        for kind, mode in (("vanilla", "main"), ("rust", "main"), ("vanilla", "lava"), ("rust", "lava")):
            case = run_case(kind, mode, run, args, source, report)
            report["cases"].append(case)
            write_json(run / "report.json", report)
            if case.get("source_integrity_error") or not case["exit"]["process_reaped"]:
                raise ValueError("Refusing further servers after fixture mutation or an unreaped process")
        verify_source(report, source, "final verified source")
        if furnaces.digest(args.rust_binary) != pinned or furnaces.digest(acquire()) != report["official_jar"]["sha256"]:
            raise ValueError("Pinned binary or official reference changed during the run")
        if harness_manifest() != report["harness_manifest"]:
            raise ValueError("Harness changed during the run")
        if all(case["result"] == "passed" for case in report["cases"]) and len(report["cases"]) == 4:
            report["result"] = "passed"
    except Exception as error:
        report.update(error=f"{type(error).__name__}: {error}", traceback=traceback.format_exc())
    finally:
        report.update(finished=now(), duration_seconds=round(time.monotonic() - began, 3))
        write_json(run / "report.json", report)
        write_json(args.output, report)
    print(f"Furnace diagnostic {report['result']}; report: {args.output}", flush=True)
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
