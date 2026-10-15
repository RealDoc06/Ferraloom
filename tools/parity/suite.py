#!/usr/bin/env python3
"""Run identical scenarios against isolated official and Ferraloom servers."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import gzip
import json
import os
from pathlib import Path
import queue
import re
import signal
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from oracle import ROOT, TARGET, CACHE, acquire, java_command, inventory, compare_registry
from parity import protocol
from parity.bot import run_client_scenarios
from parity.worlds import decode_nbt, compare_terrain

ANSI = re.compile(r"\x1b\[[0-9;]*m")


def harness_manifest() -> dict[str, str]:
    inputs = sorted((ROOT / "tools/parity").glob("*.py")) + sorted((ROOT / "tools/parity").glob("*.ts"))
    inputs += [ROOT / "tools/oracle.py", ROOT / "compatibility/target.json", ROOT / "compatibility/scenario-map.json"]
    return {str(path.relative_to(ROOT)): hashlib.file_digest(path.open("rb"), "sha256").hexdigest() for path in inputs}


def git_state() -> dict:
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True)
    status = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True)
    return {"commit": commit.stdout.strip() if commit.returncode == 0 else None, "dirty": bool(status.stdout.strip())}


def data_version(world: Path) -> int:
    root = decode_nbt(gzip.decompress((world / "level.dat").read_bytes()))
    return root["Data"]["DataVersion"]


def is_server_say(line: str, token: str) -> bool:
    return bool(re.search(r"\[Server\]\s+" + re.escape(token) + r"$", line))


@dataclass(frozen=True)
class Scenario:
    id: str
    features: tuple[str, ...]
    commands: tuple[str, ...]
    predicate: str
    settle: float = 0.15
    precondition: str | None = None
    force_dimension: str | None = None


SCENARIOS = [
    Scenario("block.setblock", ("blocks.placement",), ("setblock 0 200 0 stone",), "if block 0 200 0 stone"),
    Scenario("block.state", ("blocks.registry",), ("setblock 0 200 0 oak_stairs[facing=east,half=top]",), "if block 0 200 0 oak_stairs[facing=east,half=top]"),
    Scenario("block.fill", ("players.commands",), ("fill 0 200 0 2 201 2 glass",), " ".join(f"if block {x} {y} {z} glass" for x in range(3) for y in range(200, 202) for z in range(3))),
    Scenario("block.clone", ("players.commands",), ("setblock 0 200 0 diamond_block", "clone 0 200 0 0 200 0 5 200 5"), "if block 5 200 5 diamond_block"),
    Scenario("block.water_flow", ("blocks.fluids",), ("fill 0 200 0 3 204 3 air", "setblock 1 203 1 water"), "if block 1 202 1 water", 1.5),
    Scenario("block.gravity", ("blocks.falling",), ("fill 0 200 0 3 204 3 air", "setblock 1 203 1 sand"), "if block 1 200 1 sand", 1.5),
    Scenario("redstone.lamp", ("blocks.redstone_power",), ("setblock 8 200 0 redstone_lamp", "setblock 9 200 0 redstone_block"), "if block 8 200 0 redstone_lamp[lit=true]", 0.5),
    Scenario("redstone.wire", ("blocks.redstone_power",), ("setblock 8 200 3 redstone_wire", "setblock 9 200 3 redstone_block"), "if block 8 200 3 redstone_wire[power=15]", 0.5),
    Scenario("redstone.piston_push", ("blocks.pistons",), ("fill 0 200 8 4 202 10 air", "setblock 1 200 9 stone", "setblock 0 200 9 piston[facing=east]", "setblock -1 200 9 redstone_block"), "if block 2 200 9 stone", 1.0),
    Scenario("redstone.daylight", ("blocks.redstone_power",), ("time set 6000", "weather clear", "setblock 12 200 0 daylight_detector"), "if block 12 200 0 daylight_detector[power=15]", 1.5),
    Scenario("scoreboard.arithmetic", ("players.scoreboards",), ("scoreboard objectives add parity dummy", "scoreboard players set A parity 7", "scoreboard players set B parity 5", "scoreboard players operation A parity += B parity"), "if score A parity matches 12"),
    Scenario("data.storage", ("players.commands",), ('data modify storage ferraloom:probe answer set value 42',), "if data storage ferraloom:probe {answer:42}"),
    Scenario("entity.summon", ("entities.spawning",), ('summon pig 8 201 8 {Tags:["ferraloom_probe"],NoAI:1b}',), "if entity @e[type=pig,tag=ferraloom_probe,limit=1]"),
    Scenario("entity.damage", ("entities.combat",), ("damage @e[type=pig,tag=ferraloom_probe,limit=1] 5 generic",), "if entity @e[type=pig,tag=ferraloom_probe,nbt={Health:5.0f},limit=1]"),
    Scenario("entity.kill", ("entities.lifecycle",), ("kill @e[tag=ferraloom_probe]",), "unless entity @e[tag=ferraloom_probe]", 1.5, "if entity @e[tag=ferraloom_probe]"),
    Scenario("datapack.function", ("world.functions",), ("function ferraloom:probe",), "if score Datapack parity matches 37"),
    Scenario("dimension.nether", ("world.nether",), ("execute in minecraft:the_nether run setblock 0 100 0 gold_block",), "in minecraft:the_nether if block 0 100 0 gold_block", 0.5, force_dimension="minecraft:the_nether"),
    Scenario("dimension.end", ("world.end",), ("execute in minecraft:the_end run setblock 0 100 0 emerald_block",), "in minecraft:the_end if block 0 100 0 emerald_block", 0.5, force_dimension="minecraft:the_end"),
    Scenario("persistence.write", ("persistence.chunk_data",), ("setblock 14 200 14 diamond_block", "save-all flush"), "if block 14 200 14 diamond_block", 0.5),
]


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def datapack(directory: Path) -> None:
    pack = directory / "world/datapacks/ferraloom-parity"
    functions = pack / "data/ferraloom/function"
    functions.mkdir(parents=True)
    (pack / "pack.mcmeta").write_text(json.dumps({"pack": {"description": "Ferraloom compatibility fixture", "pack_format": 121, "min_format": 121, "max_format": 121}}))
    (functions / "probe.mcfunction").write_text("scoreboard players set Datapack parity 37\n")


class Server:
    def __init__(self, kind: str, directory: Path, args: argparse.Namespace):
        self.kind, self.directory, self.args = kind, directory, args
        self.port = free_port()
        self.process: subprocess.Popen[str] | None = None
        self.lines: queue.Queue[str] = queue.Queue()
        self.log_path = directory / "console.log"
        self.forced_stop = False
        self.reader: threading.Thread | None = None

    def configure(self) -> None:
        self.directory.mkdir(parents=True)
        datapack(self.directory)
        if self.kind == "vanilla":
            (self.directory / "eula.txt").write_text("# Explicit local test acceptance supplied via --accept-eula\neula=true\n")
            (self.directory / "server.properties").write_text(
                f"server-ip=127.0.0.1\nserver-port={self.port}\nonline-mode=false\nenforce-secure-profile=false\n"
                "level-seed=12345678\nview-distance=2\nsimulation-distance=2\nmax-players=20\n"
                "spawn-protection=0\nnetwork-compression-threshold=256\nmax-tick-time=60000\n"
                "white-list=false\nallow-flight=true\npause-when-empty-seconds=-1\n"
                "motd=Ferraloom compatibility reference\nenable-rcon=false\n")
        else:
            (self.directory / "pumpkin.toml").write_text(
                'seed = "12345678"\nspawn_protection = 0\n'
                f'[networking.java]\naddress = "127.0.0.1:{self.port}"\nonline_mode = false\nencryption = false\n'
                'view_distance = 2\nsimulation_distance = 2\nmax_players = 20\n'
                '[networking.bedrock]\nenabled = false\n[networking.lan_broadcast]\nenabled = false\n'
                '[commands]\nuse_tty = false\n[logging]\ncolor = false\nfile = ""\ntimestamp = false\n'
                '[telemetry]\nenabled = false\n[plugins]\nenabled = false\n')
            with (self.directory / "pumpkin.toml").open("a") as config:
                config.write('[world.chunk]\ntype = "anvil"\n[world.chunk.compression]\nalgorithm = "ZLib"\nlevel = 6\n')

    def start(self) -> None:
        if self.kind == "vanilla":
            command = java_command(self.args.java) + ["-jar", str(acquire()), "nogui"]
        else:
            command = [str(self.args.rust_binary.resolve()), "--data-dir", str(self.directory),
                       "--bind", f"127.0.0.1:{self.port}", "--offline", "--workers", str(self.args.workers)]
        self.lines = queue.Queue()
        self.process = subprocess.Popen(command, cwd=self.directory, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                        bufsize=1, start_new_session=True)
        process = self.process
        lines = self.lines

        def read() -> None:
            assert process.stdout is not None
            with self.log_path.open("a") as log:
                for raw in process.stdout:
                    line = ANSI.sub("", raw.rstrip())
                    log.write(line + "\n")
                    log.flush()
                    lines.put(line)

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        deadline = time.monotonic() + self.args.startup_timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"{self.kind} exited {process.returncode}; see {self.log_path}")
            try:
                protocol.status_fields(self.port)
                self.command("list")
                return
            except (OSError, EOFError, ValueError, AssertionError):
                time.sleep(0.2)
        raise TimeoutError(f"{self.kind} startup exceeded {self.args.startup_timeout}s; see {self.log_path}")

    def command(self, command: str, timeout: float = 15) -> list[str]:
        assert self.process is not None and self.process.stdin is not None
        token = f"FERRALOOM_END_{uuid.uuid4().hex}"
        self.process.stdin.write(command + "\nsay " + token + "\n")
        self.process.stdin.flush()
        lines: list[str] = []
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"Server exited {self.process.returncode} while executing {command}")
            try:
                line = self.lines.get(timeout=min(0.25, max(0.001, deadline - time.monotonic())))
            except queue.Empty:
                continue
            lines.append(line)
            if is_server_say(line, token):
                return lines
        raise TimeoutError(f"Command timed out: {command}")

    def stop(self) -> int | None:
        process = self.process
        if process is None:
            return None
        if process.poll() is None:
            try:
                assert process.stdin is not None
                process.stdin.write("stop\n")
                process.stdin.flush()
                process.wait(timeout=45)
            except (OSError, subprocess.TimeoutExpired):
                self.forced_stop = True
                self.signal(signal.SIGTERM)
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    self.signal(signal.SIGKILL)
                    process.wait(timeout=5)
        if self.reader is not None:
            self.reader.join(timeout=2)
        return process.returncode

    def signal(self, signum: int) -> None:
        if self.process is not None:
            try:
                os.killpg(self.process.pid, signum)
            except ProcessLookupError:
                pass


def recorded(test_id: str, features: tuple[str, ...], action) -> dict:
    started = time.monotonic()
    try:
        detail = action()
        result = {"result": "failed" if isinstance(detail, dict) and detail.get("result") == "failed" else "passed", "detail": detail}
    except Exception as error:
        result = {"result": "failed", "error": f"{type(error).__name__}: {error}"}
    result.update({"id": test_id, "features": list(features), "duration_seconds": round(time.monotonic() - started, 3)})
    print(f"  {test_id}: {result['result']}", flush=True)
    return result


def run_scenario(server: Server, scenario: Scenario) -> dict:
    if scenario.force_dimension is not None:
        server.command(f"execute in {scenario.force_dimension} run forceload add 0 0")
        wait_loaded(server, scenario.force_dimension)
    if scenario.precondition is not None:
        check_predicate(server, scenario.precondition)
    for command in scenario.commands:
        server.command(command)
    time.sleep(scenario.settle)
    check_predicate(server, scenario.predicate)
    return {"predicate": scenario.predicate, "commands": list(scenario.commands)}


def check_predicate(server: Server, predicate: str) -> None:
    token = "FERRALOOM_PASS_" + uuid.uuid4().hex
    output = server.command(f"execute {predicate} run say {token}")
    if not any(is_server_say(line, token) for line in output):
        raise AssertionError(f"Predicate failed: {predicate}; output: {output[-6:]}")


def wait_loaded(server: Server, dimension: str = "minecraft:overworld", *, x: int = 0, z: int = 0) -> None:
    deadline = time.monotonic() + 60
    while True:
        try:
            check_predicate(server, f"in {dimension} if loaded {x} 100 {z}")
            return
        except AssertionError:
            if time.monotonic() >= deadline:
                raise AssertionError(f"Forced chunk never loaded: {dimension} {x} {z}") from None
            time.sleep(0.2)


def run(server: Server, quick: bool) -> list[dict]:
    results: list[dict] = []
    try:
        server.configure()
        if not quick:
            results.append(recorded("blocks.registry_tables", ("blocks.registry",), lambda: registry_tables(server)))
        server.start()
        if server.args.rust_binary.is_file():
            results.append(recorded("lifecycle.world_lock", ("persistence.level_data",), lambda: world_lock(server)))
        for name, feature, action in [
            ("network.status_fields", "network.status", protocol.status_fields),
            ("network.status_other_version", "network.status", protocol.status_other_version),
            ("network.incompatible_login", "network.handshake", protocol.incompatible_login),
            ("network.ping_before_status", "network.status", protocol.ping),
            ("network.duplicate_status", "network.status", protocol.duplicate_status),
            ("network.fragmented", "network.framing", protocol.fragmented_status),
            ("network.concurrent_status", "network.framing", protocol.concurrent_status),
            ("network.malformed_length", "network.framing", protocol.malformed_length),
        ]:
            results.append(recorded(name, (feature,), lambda action=action: action(server.port)))
        if not quick:
            server.command("forceload add -16 -16 31 31")
            wait_loaded(server)
            wait_loaded(server, x=-1, z=9)
            server.command("fill -2 199 -2 15 199 15 stone")
            results.extend(run_client_scenarios(server))
            for scenario in SCENARIOS:
                results.append(recorded(scenario.id, scenario.features, lambda scenario=scenario: run_scenario(server, scenario)))
            server.command("save-off")
            unflushed = Scenario("persistence.unflushed_write", ("persistence.chunk_data",), ("setblock 13 200 14 emerald_block",), "if block 13 200 14 emerald_block")
            results.append(recorded(unflushed.id, unflushed.features, lambda: run_scenario(server, unflushed)))
            exit_code = server.stop()
            results.append(recorded("lifecycle.clean_shutdown", ("lifecycle.shutdown",), lambda: check_exit(exit_code, server.forced_stop)))
            server.start()
            server.command("forceload add 0 0")
            wait_loaded(server)
            persisted = Scenario("persistence.restart", ("persistence.chunk_data",), (), "if block 14 200 14 diamond_block")
            results.append(recorded(persisted.id, persisted.features, lambda: run_scenario(server, persisted)))
            unflushed_read = Scenario("persistence.shutdown_restart", ("persistence.chunk_data",), (), "if block 13 200 14 emerald_block")
            results.append(recorded(unflushed_read.id, unflushed_read.features, lambda: run_scenario(server, unflushed_read)))
            signal_write = Scenario("persistence.signal_write", ("persistence.chunk_data",), ("setblock 12 200 14 lapis_block",), "if block 12 200 14 lapis_block")
            results.append(recorded(signal_write.id, signal_write.features, lambda: run_scenario(server, signal_write)))
            server.signal(signal.SIGTERM)
            protocol.require(server.process is not None, "Missing server process")
            signal_code = server.process.wait(timeout=45)
            results.append(recorded("lifecycle.sigterm_exit", ("lifecycle.shutdown",), lambda: check_signal_exit(server.kind, signal_code, server.forced_stop)))
            server.stop()
            server.start()
            server.command("forceload add 0 0")
            wait_loaded(server)
            signal_read = Scenario("persistence.signal_restart", ("persistence.chunk_data",), (), "if block 12 200 14 lapis_block")
            results.append(recorded(signal_read.id, signal_read.features, lambda: run_scenario(server, signal_read)))
    except Exception as error:
        results.append({"id": "harness.startup_or_setup", "features": [], "result": "failed", "error": f"{type(error).__name__}: {error}"})
    finally:
        server.stop()
    return results


def world_lock(server: Server) -> dict:
    """Ferraloom must respect both vanilla and its own live world record locks."""
    contender = subprocess.run([str(server.args.rust_binary.resolve()), "--data-dir", str(server.directory),
                               "--bind", f"127.0.0.1:{free_port()}", "--offline", "--workers", "1"],
                              cwd=server.directory, capture_output=True, text=True, timeout=20)
    output = contender.stdout + contender.stderr
    protocol.require(contender.returncode != 0 and "already locked" in output, f"Concurrent server did not reject the held world: {contender.returncode}; {output[-600:]}")
    return {"holder": server.kind, "contender": "rust", "world_lock_rejected": True}


def registry_tables(server: Server) -> dict:
    reference = json.loads((CACHE / "generated/reports/blocks.json").read_text())
    facts = {"blocks": len(reference), "states": sum(len(block["states"]) for block in reference.values()),
             "scope": "IDs, defaults and all property tuples; mechanics excluded"}
    if server.kind == "rust":
        passed = compare_registry(server.directory / "rust-registry.json", server.directory / "registry-comparison.json", server.args.rust_binary)
        protocol.require(passed, "Runtime registry differs from official block tables")
    return facts


def check_exit(code: int | None, forced: bool) -> dict:
    protocol.require(code == 0 and not forced, f"Unclean shutdown: exit={code}, forced={forced}")
    return {"exit_code": code}


def check_signal_exit(kind: str, code: int | None, forced: bool) -> dict:
    # HotSpot completes its shutdown hooks and reports 128 + SIGTERM on Unix.
    accepted = (0, 128 + signal.SIGTERM) if kind == "vanilla" else (0,)
    protocol.require(code in accepted and not forced, f"Unclean SIGTERM shutdown: exit={code}, forced={forced}")
    return {"exit_code": code, "signal": "SIGTERM", "durability": "checked separately by persistence.signal_restart"}


def cross_world(source: str, destination: str, run_dir: Path, args: argparse.Namespace, source_cases: list[dict]) -> dict:
    """Load the other implementation's saved overworld without format conversion."""
    test_id = f"persistence.{source}_to_{destination}"
    if not any(case["id"] == "persistence.write" and case["result"] == "passed" for case in source_cases):
        return {"id": test_id, "features": ["persistence.chunk_data", "persistence.level_data"],
                "result": "skipped", "reason": "Source persistence.write did not establish the saved marker"}
    server = Server(destination, run_dir / f"migration-{source}-to-{destination}", args)

    def check() -> dict:
        try:
            source_world = run_dir / source / "world"
            protocol.require((source_world / "level.dat").is_file(), "Source did not save level.dat")
            source_version = data_version(source_world)
            protocol.require(source_version == 5023, f"Source world is not the target format: {source_version}")
            server.configure()
            shutil.copytree(source_world, server.directory / "world", dirs_exist_ok=True)
            server.start()
            server.command("forceload add 0 0")
            wait_loaded(server)
            predicate = "if block 14 200 14 diamond_block"
            check_predicate(server, predicate)
            code = server.stop()
            check_exit(code, server.forced_stop)
            destination_version = data_version(server.directory / "world")
            protocol.require(destination_version == source_version, "Loading rewrote the data version")
            return {"source": source, "destination": destination, "predicate": predicate, "source_data_version": source_version,
                    "destination_data_version": destination_version, "external_conversion": False, "dimension": "overworld"}
        finally:
            server.stop()

    return recorded(test_id, ("persistence.chunk_data", "persistence.level_data"), check)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java", default=os.environ.get("JAVA", "java"))
    parser.add_argument("--rust-binary", type=Path, default=ROOT / "target/debug/ferraloom")
    parser.add_argument("--accept-eula", action="store_true", help="Accept Minecraft EULA for these isolated local reference runs")
    parser.add_argument("--server", choices=["both", "vanilla", "rust"], default="both")
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--quick", action="store_true", help="Run network scenarios only")
    parser.add_argument("--record", action="store_true", help="Record known gaps without failing the process; report still marks every failure")
    parser.add_argument("--output", type=Path, default=ROOT / "compatibility/results/latest.json")
    args = parser.parse_args()
    if args.server in ("both", "vanilla") and not args.accept_eula:
        parser.error("Vanilla execution requires explicit --accept-eula (https://aka.ms/MinecraftEULA)")
    if args.workers < 1:
        parser.error("--workers must be at least 1")
    if args.server in ("both", "rust") and not args.rust_binary.is_file():
        parser.error(f"Build Ferraloom first: missing {args.rust_binary}")
    (ROOT / ".cache/runs").mkdir(parents=True, exist_ok=True)
    if not args.quick:
        inventory(args.java, ROOT / "compatibility/vanilla-inventory.json")
    run_dir = Path(tempfile.mkdtemp(prefix="parity-", dir=ROOT / ".cache/runs"))
    report = {"schema_version": 1, "target_version": TARGET["version"], "protocol": TARGET["protocol"],
              "server_sha1": TARGET["server_sha1"], "rust_upstream": TARGET["rust_upstream"],
              "created_at": datetime.now(timezone.utc).isoformat(),
              "environment": {"host": socket.gethostname(), "platform": sys.platform, "auth": "isolated offline loopback", "seed": "12345678", "workers": args.workers},
              "scope": "Selected scenarios only; passing does not establish full vanilla parity.", "servers": {}}
    report["harness_manifest"] = harness_manifest()
    report["test_suite_sha256"] = hashlib.sha256(json.dumps(report["harness_manifest"], sort_keys=True).encode()).hexdigest()
    report["source_state"] = git_state()
    if args.rust_binary.is_file():
        report["rust_binary_sha256"] = hashlib.file_digest(args.rust_binary.open("rb"), "sha256").hexdigest()
    for kind in (["vanilla", "rust"] if args.server == "both" else [args.server]):
        print(f"Running {kind} on this host's loopback; logs: {run_dir / kind / 'console.log'}", flush=True)
        report["servers"][kind] = run(Server(kind, run_dir / kind, args), args.quick)
    if args.server == "both" and not args.quick:
        report["cross_world"] = [cross_world(source, destination, run_dir, args, report["servers"][source])
                                 for source, destination in [("vanilla", "rust"), ("rust", "vanilla")]]
        report["terrain"] = recorded("worldgen.underground_chunk", ("world.terrain",),
                                     lambda: compare_terrain(run_dir / "vanilla/world", run_dir / "rust/world"))
    vanilla = {case["id"]: case for case in report["servers"].get("vanilla", [])}
    rust = {case["id"]: case for case in report["servers"].get("rust", [])}
    report["comparison"] = [{"id": key,
                             "vanilla": vanilla.get(key, {}).get("result", "not_run"),
                             "rust": rust.get(key, {}).get("result", "not_run"),
                             "result": "passed" if vanilla.get(key, {}).get("result") == rust.get(key, {}).get("result") == "passed" else "failed"}
                            for key in sorted(vanilla.keys() | rust.keys())]
    failed = [f"{kind}:{case['id']}" for kind, cases in report["servers"].items() for case in cases if case["result"] != "passed"]
    failed.extend(case["id"] for case in report.get("cross_world", []) if case["result"] != "passed")
    if report.get("terrain", {}).get("result") == "failed":
        failed.append("worldgen.underground_chunk")
    if report["harness_manifest"] != harness_manifest():
        failed.append("harness.changed_during_run")
    report["failed"] = failed
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2).replace(str(ROOT), "$REPO") + "\n")
    print(f"Report: {args.output}; {len(failed)} failed scenarios. Raw logs: {run_dir}")
    return 0 if args.record or not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
