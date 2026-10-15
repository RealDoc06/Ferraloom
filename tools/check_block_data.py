#!/usr/bin/env python3
"""Observe selected live container changes through actual protocol-777 menus.

Runs fresh, sequential Java/Rust loopback fixtures and a clean restart of each.
Only plain item stacks and single barrel/chest/trapped-chest menus are covered.
No rich components, general click correctness, automation or full parity claim.
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
import socket
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from oracle import CACHE, ROOT, TARGET, acquire
from parity import suite
from parity.bot import Client, NAME
from parity.furnaces import digest, require_frozen, step_exact
from parity.protocol import require

CONTAINERS = {"barrel": (4097, 200, 1), "chest": (4101, 200, 1), "trapped_chest": (4105, 200, 1)}
LOOT_POSITIONS = {"chest": (4109, 200, 1), "trapped_chest": (4109, 200, 5)}
ITEMS = {"diamond": 1012, "emerald": 1013, "gold_ingot": 1022}
CASES = {
    **{f"block_data.{name}_menu": "network.inventory_stream" for name in CONTAINERS},
    **{f"block_data.{name}_swap": "network.inventory_stream" for name in CONTAINERS},
    **{f"block_data.{name}_restart": "persistence.block_entities" for name in CONTAINERS},
    **{f"block_data.{name}_cleared_loot_menu": "blocks.containers" for name in LOOT_POSITIONS},
}
WORLD_ERRORS = re.compile(r"\bERROR\b|Failed to parse saved data|Couldn't load chunk|Failed to (?:load|read|write|save|parse).*?(?:chunk|region|world|data)", re.IGNORECASE)


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def manifest() -> dict:
    return {**suite.harness_manifest(), **{path: digest(ROOT / path) for path in (
        "tools/check_block_data.py", "tests/block_data_protocol.test.ts",
    )}}


class LoggedServer(suite.Server):
    def command(self, command: str, timeout: float = 15) -> list[str]:
        entry = {"command": command, "started": now()}
        try:
            output = super().command(command, timeout)
            entry["output"] = output
            return output
        except Exception as error:
            entry["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            with (self.directory / "commands.jsonl").open("a") as log:
                log.write(json.dumps(entry) + "\n")


def stack(item: str, count: int) -> dict:
    return {"item_id": ITEMS[item], "count": count}


def contents(entries: dict[int, dict] | None = None) -> list:
    slots = [None] * 27
    for slot, value in (entries or {}).items():
        slots[slot] = value
    return slots


def pos_text(pos: tuple[int, int, int]) -> str:
    return " ".join(map(str, pos))


def teleport(server: LoggedServer, client: Client, pos: tuple[int, int, int]) -> None:
    x, y, z = pos
    target = {"x": x + 0.5, "y": y, "z": z + 2.5}
    before = client.state()["teleport_confirmations"]
    server.command(f"tp {NAME} {target['x']} {target['y']} {target['z']} 180 0")
    client.wait_state(lambda state: state["teleport_confirmations"] > before
                      and all(abs((state.get("server_position") or {}).get(axis, float("inf")) - value) < 0.01
                              for axis, value in target.items()))


def close_menu(client: Client) -> None:
    if client.state().get("menu") is not None:
        client.request("close_menu")


def open_menu(client: Client, pos: tuple[int, int, int], expected: list) -> dict:
    close_menu(client)
    before = client.state()["packets"]
    client.request("use_item_on", **dict(zip(("x", "y", "z"), pos)))

    def matches(state: dict) -> bool:
        menu = state.get("menu")
        return (isinstance(menu, dict) and menu["opened_sequence"] > before
                and menu["menu_type"] == 2 and len(menu["slots"]) == 63
                and menu["slots"][:27] == expected and menu["carried"] is None)

    state = client.wait_state(matches)
    menu = state["menu"]
    require(menu["container_id"] > 0, menu)
    packets = [packet for packet in state["menu_packets"]
               if packet["sequence"] > before and packet["container_id"] == menu["container_id"]]
    require(any(packet["kind"] == "open" for packet in packets)
            and any(packet["kind"] == "content" and len(packet["slots"]) == 63 for packet in packets),
            "Opening requires real OpenScreen and 63-slot SetContent packets")
    return {"menu": menu, "packets": packets}


def change_menu(server: LoggedServer, client: Client, command: str, expected: list,
                changed: tuple[int, ...]) -> dict:
    before = client.state()
    old_menu = before["menu"]
    require(isinstance(old_menu, dict), "Menu must remain open across /data")
    output = server.command(command)
    ticks = step_exact(server, 2)

    def matches(state: dict) -> bool:
        menu = state.get("menu")
        if not (isinstance(menu, dict) and menu["container_id"] == old_menu["container_id"]
                and menu["opened_sequence"] == old_menu["opened_sequence"]
                and menu["slots"][:27] == expected):
            return False
        packets = [packet for packet in state["menu_packets"]
                   if packet["sequence"] > before["packets"] and packet["container_id"] == menu["container_id"]]
        return all(any((packet["kind"] == "content" and packet["slots"][slot] == expected[slot])
                       or (packet["kind"] == "slot" and packet["slot"] == slot and packet["stack"] == expected[slot])
                       for packet in packets) for slot in changed)

    state = client.wait_state(matches)
    packets = [packet for packet in state["menu_packets"]
               if packet["sequence"] > before["packets"] and packet["container_id"] == old_menu["container_id"]]
    require(state["menu"]["state_id"] != old_menu["state_id"], "Inventory mutation did not advance the menu state ID")
    return {"command": command, "output": output, "ticks": ticks,
            "before_sequence": before["packets"], "opened_sequence": old_menu["opened_sequence"],
            "container_id": old_menu["container_id"], "packets": packets}


def fresh_container(server: LoggedServer, client: Client, name: str, pos: tuple[int, int, int]) -> None:
    close_menu(client)
    server.command(f"clear {NAME}")
    server.command(f"setblock {pos_text(pos)} air")
    server.command(f"setblock {pos_text(pos)} {name}")
    suite.check_predicate(server, f"if block {pos_text(pos)} {name}")
    teleport(server, client, pos)


def menu_case(server: LoggedServer, client: Client, name: str) -> dict:
    pos = CONTAINERS[name]
    fresh_container(server, client, name, pos)
    initial = open_menu(client, pos, contents())
    phases = []
    for item, count in (("diamond", 7), ("emerald", 3)):
        command = f'data modify block {pos_text(pos)} Items set value [{{Slot:0b,id:"minecraft:{item}",count:{count}}}]'
        phases.append(change_menu(server, client, command, contents({0: stack(item, count)}), (0,)))
    phases.append(change_menu(server, client, f"item replace block {pos_text(pos)} container.1 with minecraft:gold_ingot 5",
                              contents({0: stack("emerald", 3), 1: stack("gold_ingot", 5)}), (1,)))
    phases.append(change_menu(server, client, f"data remove block {pos_text(pos)} Items", contents(), (0, 1)))
    close_menu(client)
    return {"block": name, "position": list(pos), "initial": initial, "phases": phases}


def swap_reopen(client: Client, pos: tuple[int, int, int], expected: list, hotbar: dict | None) -> dict:
    sent = client.request("swap", slot=2, hotbar=0)
    observed = open_menu(client, pos, expected)
    require(observed["menu"]["slots"][54] == hotbar, "Fresh menu content has the wrong player hotbar stack")
    return {"action": sent, "observed": observed}


def swap_case(server: LoggedServer, client: Client, name: str) -> dict:
    pos = CONTAINERS[name]
    fresh_container(server, client, name, pos)
    server.command(f"item replace entity {NAME} hotbar.0 with minecraft:gold_ingot 5")
    initial = open_menu(client, pos, contents())
    require(initial["menu"]["slots"][54] == stack("gold_ingot", 5), "Missing actual player stack before SWAP")
    deposited = contents({2: stack("gold_ingot", 5)})
    deposit = swap_reopen(client, pos, deposited, None)
    extraction = swap_reopen(client, pos, contents(), stack("gold_ingot", 5))
    server.command("scoreboard players set Count blockdata -999")
    output = server.command(f"execute store result score Count blockdata run clear {NAME} minecraft:gold_ingot 0")
    suite.check_predicate(server, "if score Count blockdata matches 5")
    # clear ... 0 leaves the items available for the persisted deposit.
    persisted = swap_reopen(client, pos, deposited, None)
    close_menu(client)
    return {"initial": initial, "deposit": deposit, "extraction": extraction,
            "clear_test_output": output, "clear_result": 5, "persisted_deposit": persisted}


def loot_case(server: LoggedServer, client: Client, name: str) -> dict:
    pos = LOOT_POSITIONS[name]
    fresh_container(server, client, name, pos)
    outputs = [server.command(f'data merge block {pos_text(pos)} {{LootTable:"minecraft:empty",LootTableSeed:19L}}')]
    suite.check_predicate(server, f'if data block {pos_text(pos)} {{LootTable:"minecraft:empty"}}')
    outputs.append(server.command(f"data remove block {pos_text(pos)} LootTable"))
    outputs.append(server.command(f"item replace block {pos_text(pos)} container.0 with minecraft:diamond 7"))
    observed = open_menu(client, pos, contents({0: stack("diamond", 7)}))
    close_menu(client)
    return {"position": list(pos), "setup_output": outputs, "first_open": observed,
            "scope": "Pending empty loot removed before the first open; inserted items must survive."}


def run_side(kind: str, directory: Path, args: argparse.Namespace, report: dict) -> list[dict]:
    server = LoggedServer(kind, directory, args)
    results: dict[str, dict] = {}
    processes = report["processes"][kind] = []
    clients: list[Client] = []
    failures: list[str] = []

    def record(test_id: str, action) -> dict:
        entry = suite.recorded(test_id, (CASES[test_id],), action)
        results[test_id] = entry
        return entry

    def stop() -> None:
        code = server.stop()
        processes[-1].update(exit_code=code, forced_stop=server.forced_stop, stopped=now())
        suite.check_exit(code, server.forced_stop)
        errors = [line for line in server.log_path.read_text().splitlines() if WORLD_ERRORS.search(line)]
        require(not errors, f"Server reported errors: {errors[:5]}")

    def connect(label: str) -> Client:
        client = Client(server, block_data=True, log_name=label)
        clients.append(client)
        client.ready()
        return client

    def close_client(client: Client) -> None:
        client.close()
        code = client.process.returncode
        processes[-1].setdefault("clients", []).append({"pid": client.process.pid, "exit_code": code})
        require(code == 0, f"Protocol client exited {code}")

    try:
        server.configure()
        configs = {path.name: digest(path) for path in directory.iterdir() if path.name in ("eula.txt", "server.properties", "pumpkin.toml")}
        for boot in ("initial", "restart"):
            processes.append({"boot": boot, "started": now(), "configs_sha256": configs, "log": str(server.log_path)})
            server.start()
            processes[-1].update(pid=server.process.pid, argv=server.process.args)
            server.command("tick freeze")
            require_frozen(server)
            server.command("forceload add 4096 0")
            suite.wait_loaded(server, x=4096, z=0)
            if boot == "initial":
                server.command("fill 4096 199 0 4111 199 7 stone")
                server.command("difficulty peaceful")
                server.command("scoreboard objectives add blockdata dummy")
            client = connect("wire-" + boot)
            server.command(f"gamemode survival {NAME}")
            if boot == "initial":
                for name in CONTAINERS:
                    record(f"block_data.{name}_menu", lambda name=name: menu_case(server, client, name))
                    record(f"block_data.{name}_swap", lambda name=name: swap_case(server, client, name))
                for name in LOOT_POSITIONS:
                    record(f"block_data.{name}_cleared_loot_menu", lambda name=name: loot_case(server, client, name))
            else:
                for name, pos in CONTAINERS.items():
                    test_id = f"block_data.{name}_restart"
                    if results[f"block_data.{name}_swap"]["result"] != "passed":
                        results[test_id] = {"id": test_id, "features": [CASES[test_id]], "result": "skipped",
                                            "detail": {"reason": "The real SWAP deposit prerequisite failed."}}
                        continue

                    def restarted(pos=pos) -> dict:
                        teleport(server, client, pos)
                        observed = open_menu(client, pos, contents({2: stack("gold_ingot", 5)}))
                        close_menu(client)
                        return {"after_clean_restart": observed, "scope": "Real SWAP deposit survives save, stop, reload and menu reopen."}

                    record(test_id, restarted)
            close_client(client)
            server.save_flush()
            require_frozen(server)
            stop()
    except Exception as error:
        failures.append(f"{type(error).__name__}: {error}")
    finally:
        for client in clients:
            if not any(entry.get("pid") == client.process.pid for process in processes for entry in process.get("clients", [])):
                try:
                    close_client(client)
                except Exception as error:
                    failures.append(f"Client cleanup: {type(error).__name__}: {error}")
        if server.process is not None and "exit_code" not in processes[-1]:
            try:
                processes[-1].update(pid=server.process.pid, argv=server.process.args)
                stop()
            except Exception as error:
                failures.append(f"Server cleanup: {type(error).__name__}: {error}")
        for test_id, feature in CASES.items():
            if test_id not in results:
                results[test_id] = {"id": test_id, "features": [feature], "result": "failed", "error": "; ".join(failures) or "Case was not reached"}
            if failures:
                results[test_id].update(result="failed", runtime_errors=failures)
        report.setdefault("runtime_errors", {})[kind] = failures
    return [results[test_id] for test_id in CASES]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accept-eula", action="store_true")
    parser.add_argument("--java", required=True)
    parser.add_argument("--rust-binary", type=Path, required=True)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, default=ROOT / ".cache/block-data.json")
    args = parser.parse_args()
    if not args.accept_eula:
        parser.error("Explicit --accept-eula is required for https://aka.ms/MinecraftEULA")
    if args.workers < 1 or args.startup_timeout <= 0:
        parser.error("Workers and startup timeout must be positive")
    if not args.rust_binary.is_file() or not os.access(args.rust_binary, os.X_OK):
        parser.error("--rust-binary must be an existing executable")
    args.output = args.output.resolve()
    if not args.output.is_relative_to((ROOT / ".cache").resolve()):
        parser.error("Reports must stay under this repository's ignored .cache directory")
    runs = ROOT / ".cache/runs"
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="block-data-", dir=runs))
    report = {"schema_version": 1, "kind": "live_block_data", "created_at": now(), "target_version": TARGET["version"],
              "protocol": TARGET["protocol"], "fixture": str(run), "result": "failed", "servers": {}, "processes": {},
              "environment": {"host": socket.gethostname(), "platform": sys.platform, "auth": "isolated offline 127.0.0.1", "seed": "12345678", "workers": args.workers},
              "scope": __doc__, "failed": []}
    began = time.monotonic()
    try:
        report["source_state"] = suite.git_state()
        require(report["source_state"]["dirty"] is False, "Commit source changes before this paired acceptance run")
        report["harness_manifest"] = manifest()
        report["test_suite_sha256"] = hashlib.sha256(json.dumps(report["harness_manifest"], sort_keys=True).encode()).hexdigest()
        require((CACHE / "server.jar").is_file(), "Pinned official jar must already be cached; this check does not download it")
        official = acquire()
        report.update(server_sha1=digest(official, "sha1"), server_sha256=digest(official))
        original = args.rust_binary.resolve()
        before = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        report["rust_binary_sha256"] = digest(args.rust_binary)
        report["rust_binary"] = {"source": str(original), "pinned": str(args.rust_binary), "bytes": args.rust_binary.stat().st_size}
        require(before == report["rust_binary_sha256"] == digest(original), "Rust executable changed while pinning")
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        version = subprocess.run([args.java, "-version"], capture_output=True, text=True, timeout=15, check=True)
        report["java"] = {"path": args.java, "sha256": digest(Path(args.java)), "version": version.stdout + version.stderr}
        for kind in ("vanilla", "rust"):
            print(f"Running {kind} menus on {socket.gethostname()} loopback: {run / kind}", flush=True)
            report["servers"][kind] = run_side(kind, run / kind, args, report)
        if manifest() != report["harness_manifest"]:
            report["failed"].append("harness.changed_during_run")
        if suite.git_state() != report["source_state"]:
            report["failed"].append("source.changed_during_run")
        if digest(args.rust_binary) != report["rust_binary_sha256"] or digest(official) != report["server_sha256"]:
            report["failed"].append("executables.changed_during_run")
        report["failed"].extend(f"{kind}:{case['id']}" for kind, cases in report["servers"].items() for case in cases if case["result"] != "passed")
        report["result"] = "failed" if report["failed"] else "passed"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        report["failed"].append("harness.setup")
        for kind in ("vanilla", "rust"):
            report["servers"].setdefault(kind, [{"id": test_id, "features": [feature], "result": "failed", "error": report["error"]} for test_id, feature in CASES.items()])
    report.update(finished=now(), duration_seconds=round(time.monotonic() - began, 3))
    raw = json.dumps(report, indent=2) + "\n"
    (run / "report.json").write_text(raw)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(raw)
    print(f"Live block-data acceptance {report['result']}: {args.output}; logs: {run}", flush=True)
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
