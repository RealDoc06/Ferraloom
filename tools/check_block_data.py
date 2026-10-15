#!/usr/bin/env python3
"""Observe selected live container and sign changes through protocol-777 clients.

Runs fresh, sequential Java/Rust loopback fixtures and a clean restart of each.
Selected plain stacks, single container menus and standing/ceiling hanging signs.
Includes bounded literal/style/run_command fixtures, nonop permission controls,
pending empty loot, one frozen-tick editor expiry boundary and restart readback.
No general component rendering, content moderation, random loot distribution,
broad editor lifecycle or full parity claim.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
from contextlib import contextmanager
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
import threading
import time
import uuid

sys.dont_write_bytecode = True
from oracle import CACHE, ROOT, TARGET, acquire
from parity import suite
from parity.bot import Client, NAME
from parity.furnaces import digest, require_frozen, step_exact
from parity.protocol import require

CONTAINERS = {"barrel": (4097, 200, 1), "chest": (4101, 200, 1), "trapped_chest": (4105, 200, 1)}
LOOT_POSITIONS = {"chest": (4109, 200, 1), "trapped_chest": (4109, 200, 5), "barrel": (4109, 200, 3)}
PENDING_BARREL = (4111, 200, 3)
SIGNS = {"sign": ((4098, 200, 11), "oak_sign[rotation=0,waterlogged=false]", 7),
         "hanging_sign": ((4106, 201, 11), "oak_hanging_sign[rotation=0,attached=false,waterlogged=false]", 8)}
# Separate supported fixtures in the same forced chunk. Never overwrite SIGNS,
# whose waxed plain state is a prerequisite for the original restart scenarios.
COMMAND_SIGNS = {name: ((pos[0], pos[1], 7), block, be_type) for name, (pos, block, be_type) in SIGNS.items()}
RICH_SIGNS = {name: ((pos[0], pos[1], 4), block, be_type) for name, (pos, block, be_type) in SIGNS.items()}
EDITOR_SIGN = ((4098, 200, 14), SIGNS["sign"][1], 7)
SIGN_COMMAND = "scoreboard players add @s signcmd 1"
LITERAL_COMMAND = json.dumps({"text": "RUN", "click_event": {"action": "run_command", "command": SIGN_COMMAND}}, separators=(",", ":"))
ITEMS = {"diamond": 1012, "emerald": 1013, "gold_ingot": 1022}
CASES = {
    **{f"block_data.{name}_menu": "network.inventory_stream" for name in CONTAINERS},
    **{f"block_data.{name}_swap": "network.inventory_stream" for name in CONTAINERS},
    **{f"block_data.{name}_restart": "persistence.block_entities" for name in CONTAINERS},
    **{f"block_data.{name}_cleared_loot_menu": "blocks.containers" for name in LOOT_POSITIONS},
    **{f"block_data.{name}_{case}": "blocks.interactives" for name in SIGNS
       for case in ("live", "editor_none", "wax")},
    **{f"block_data.{name}_restart": "persistence.block_entities" for name in SIGNS},
    "block_data.barrel_pending_empty_loot": "blocks.containers",
    "block_data.barrel_spectator_pending_loot": "blocks.containers",
    "block_data.sign_editor_tick": "blocks.interactives",
    "block_data.sign_numeric_flags": "blocks.interactives",
    **{f"block_data.{name}_{case}": "blocks.interactives" for name in SIGNS
       for case in ("literal_command", "structured_command", "command_reentry", "rich_text")},
    **{f"block_data.{name}_command_restart": "persistence.block_entities" for name in SIGNS},
}
WORLD_ERRORS = re.compile(r"\bERROR\b|Failed to parse saved data|Couldn't load chunk|Failed to (?:load|read|write|save|parse).*?(?:chunk|region|world|data)", re.IGNORECASE)


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def manifest() -> dict:
    return {**suite.harness_manifest(), **{path: digest(ROOT / path) for path in (
        "tools/check_block_data.py", "tests/block_data_protocol.test.ts",
    )}}


class LoggedServer(suite.Server):
    def __init__(self, kind: str, directory: Path, args: argparse.Namespace):
        super().__init__(kind, directory, args)
        self.broken: str | None = None
        self.owned_clients: list[Client] = []
        self.abort_events: list[dict] = []

    def abort(self, reason: str) -> None:
        """Watchdog thread kills only this side's owned process groups, even if
        synchronous sign reentry has stopped both tick and console processing.
        """
        if self.broken:
            return
        self.broken = reason
        self.forced_stop = True
        processes = [process for process in [self.process, *(client.process for client in self.owned_clients)] if process is not None]
        event = {"reason": reason, "at": now(), "pids": [process.pid for process in processes], "term": [], "kill": []}
        self.abort_events.append(event)

        def send(signum: int, field: str) -> None:
            for process in processes:
                if process.poll() is None:
                    try:
                        os.killpg(process.pid, signum)
                        event[field].append(process.pid)
                    except ProcessLookupError:
                        pass
            (self.directory / "watchdog.json").write_text(json.dumps(self.abort_events, indent=2) + "\n")

        send(signal.SIGTERM, "term")
        killer = threading.Timer(5, lambda: send(signal.SIGKILL, "kill"))
        killer.daemon = True
        killer.start()

    def command(self, command: str, timeout: float = 15) -> list[str]:
        if self.broken:
            raise RuntimeError(self.broken)
        entry = {"command": command, "started": now()}
        try:
            output = super().command(command, timeout)
            entry["output"] = output
            return output
        except Exception as error:
            entry["error"] = f"{type(error).__name__}: {error}"
            if isinstance(error, (TimeoutError, RuntimeError, BrokenPipeError)):
                self.abort(entry["error"])
            raise
        finally:
            with (self.directory / "commands.jsonl").open("a") as log:
                log.write(json.dumps(entry) + "\n")
                log.flush()


@contextmanager
def interaction_deadline(server: LoggedServer, label: str):
    timer = threading.Timer(20, lambda: server.abort(f"20s interaction watchdog: {label}"))
    timer.daemon = True
    timer.start()
    try:
        yield
    finally:
        timer.cancel()
    require(not server.broken, server.broken)


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


def pending_barrel_case(server: LoggedServer, client: Client) -> dict:
    pos = PENDING_BARREL
    fresh_container(server, client, "barrel", pos)
    command = (f'data merge block {pos_text(pos)} {{LootTable:"minecraft:empty",LootTableSeed:19L,'
               'Items:[{Slot:0b,id:"minecraft:diamond",count:7}]}')
    output = server.command(command)
    # /data reads themselves must not consume pending loot, nor expose stored
    # Items that the pending table replaces. Check twice before actual menu use.
    predicate = f'if data block {pos_text(pos)} {{LootTable:"minecraft:empty",LootTableSeed:19L}} unless data block {pos_text(pos)} Items[0]'
    reads = []
    for _ in range(2):
        reads.append(server.command(f"data get block {pos_text(pos)}"))
        suite.check_predicate(server, predicate)
    observed = open_menu(client, pos, contents())
    suite.check_predicate(server, f"unless data block {pos_text(pos)} LootTable unless data block {pos_text(pos)} Items[0]")
    close_menu(client)
    return {"position": list(pos), "command": command, "output": output, "pre_open_reads": reads,
            "pre_open_predicate": predicate, "first_open": observed,
            "scope": "Pending minecraft:empty replaces stored diamond 7 on actual first menu; no RNG claim."}


def spectator_barrel_case(server: LoggedServer, client: Client) -> dict:
    pos = PENDING_BARREL
    fresh_container(server, client, "barrel", pos)
    setup = server.command(f'data merge block {pos_text(pos)} {{LootTable:"minecraft:empty",LootTableSeed:19L}}')
    server.command(f"op {NAME}")
    server.command(f"gamemode spectator {NAME}")
    before = client.state()["packets"]
    sent = client.request("use_item_on", **dict(zip(("x", "y", "z"), pos)))
    boundary = processing_boundary(server, client)
    state = client.state()
    require(state.get("menu") is None and not any(packet["kind"] == "open" and packet["sequence"] > before
                                                 for packet in state["menu_packets"]),
            "Spectator opened a barrel whose loot was still pending")
    predicate = f'if data block {pos_text(pos)} {{LootTable:"minecraft:empty",LootTableSeed:19L}}'
    suite.check_predicate(server, predicate)
    server.command(f"gamemode survival {NAME}")
    positive = open_menu(client, pos, contents())
    suite.check_predicate(server, f"unless data block {pos_text(pos)} LootTable")
    close_menu(client)
    return {"setup": setup, "spectator_action": sent, "processing_boundary": boundary,
            "pending_after_spectator_predicate": predicate, "survival_positive": positive}


def sign_state(name: str, stage: str, waxed: bool = False) -> dict:
    return {"front_text": {"messages": [f"{name}/{stage}/front", "south", "plain", "four"],
                           "color": "red", "has_glowing_text": True},
            "back_text": {"messages": [f"{name}/{stage}/back", "north", "other", "face"],
                          "color": "blue", "has_glowing_text": False}, "is_waxed": waxed}


def sign_snbt(data: dict) -> str:
    # Direct component strings are supported by the pinned official codec.
    # NBT booleans need byte tags; JSON booleans are not SNBT byte literals.
    def face(value: dict) -> str:
        return (f'{{messages:{json.dumps(value["messages"])},color:{json.dumps(value["color"])},'
                f'has_glowing_text:{int(value["has_glowing_text"])}b}}')
    return (f'{{front_text:{face(data["front_text"])},back_text:{face(data["back_text"])},'
            f'is_waxed:{int(data["is_waxed"])}b}}')


def sign_packets(state: dict, name: str, after: int, kind: str | None = None) -> list[dict]:
    pos = SIGNS[name][0]
    return [packet for packet in state["sign_packets"] if packet["sequence"] > after
            and tuple(packet[axis] for axis in ("x", "y", "z")) == pos
            and (kind is None or packet["kind"] == kind)]


def received_sign(client: Client, name: str, expected: dict, after: int, kind: str = "block_entity") -> dict:
    def matching(state: dict) -> bool:
        packets = sign_packets(state, name, after, kind)
        return bool(packets and packets[-1]["block_entity_type"] == SIGNS[name][2] and packets[-1]["data"] == expected)
    state = client.wait_state(matching)
    return sign_packets(state, name, after, kind)[-1]


def processing_boundary(server: LoggedServer, client: Client) -> dict:
    """Same-connection command follows the tested packet on the server executor.

    Its scoreboard effect, exact tick step, and a later received server marker
    establish processing and outgoing wire boundaries. A timeout only fails.
    """
    marker = "SIGN_BOUNDARY_" + uuid.uuid4().hex
    fence = int(uuid.uuid4().hex[:7], 16)
    before = client.state()["packets"]
    server.command("scoreboard players set SignFence blockdata -1")
    sent = client.request("command", text=f"scoreboard players set SignFence blockdata {fence}")
    predicate = f"if score SignFence blockdata matches {fence}"
    deadline = time.monotonic() + 15
    while True:
        try:
            suite.check_predicate(server, predicate)
            break
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)
    ticks = step_exact(server, 1)
    output = server.command("say " + marker)
    state = client.wait_state(lambda state: any(message["sequence"] > before and marker in message["text"]
                                               for message in state["chat_messages"]))
    received = next(message for message in state["chat_messages"] if marker in message["text"])
    return {"client_command": sent, "server_predicate": predicate, "ticks": ticks,
            "server_output": output, "received_marker": received}


def sign_teleport(server: LoggedServer, client: Client, name: str, slot: int) -> None:
    x, _, z = SIGNS[name][0]
    target = {"x": x + 0.5, "y": 200, "z": z + (2.5 if slot == 1 else -1.5)}
    before = client.state()["teleport_confirmations"]
    server.command(f"tp {NAME} {target['x']} 200 {target['z']} {180 if slot == 1 else 0} 0")
    client.wait_state(lambda state: state["teleport_confirmations"] > before
                      and all(abs((state.get("server_position") or {}).get(axis, float("inf")) - value) < 0.01
                              for axis, value in target.items()))


def edit_sign_data(server: LoggedServer, client: Client, name: str, expected: dict) -> dict:
    before = client.state()["packets"]
    command = f"data merge block {pos_text(SIGNS[name][0])} {sign_snbt(expected)}"
    output = server.command(command)
    ticks = step_exact(server, 1)
    observed = received_sign(client, name, expected, before)
    return {"command": command, "output": output, "ticks": ticks, "before_sequence": before, "received": observed}


def fresh_sign(server: LoggedServer, client: Client, name: str) -> dict:
    close_menu(client)
    server.command(f"clear {NAME}")
    pos, block, _ = SIGNS[name]
    sign_teleport(server, client, name, 1)
    before = client.state()["packets"]
    server.command(f"setblock {pos_text(pos)} air")
    if name == "hanging_sign":
        server.command(f"setblock {pos[0]} {pos[1] + 1} {pos[2]} stone")
    server.command(f"setblock {pos_text(pos)} {block}")
    suite.check_predicate(server, f"if block {pos_text(pos)} {block}")
    suite.check_predicate(server, f"if block {pos[0]} {pos[1] + (1 if name == 'hanging_sign' else -1)} {pos[2]} stone")
    initial = edit_sign_data(server, client, name, sign_state(name, "base"))
    require(not sign_packets(client.state(), name, before, "editor"), "Fresh sign unexpectedly granted an editor")
    return {**initial, "fresh_block_before_sequence": before}


def interact_sign(client: Client, name: str, slot: int) -> int:
    before = client.state()["packets"]
    client.request("use_item_on", **dict(zip(("x", "y", "z"), SIGNS[name][0])), face=3 if slot == 1 else 2)
    return before


def open_sign(server: LoggedServer, client: Client, name: str, slot: int) -> dict:
    sign_teleport(server, client, name, slot)
    before = interact_sign(client, name, slot)
    state = client.wait_state(lambda state: bool(sign_packets(state, name, before, "editor")))
    editor = sign_packets(state, name, before, "editor")[-1]
    require(editor["slot"] == slot, f"Wrong face from {'south' if slot == 1 else 'north'}: {editor}")
    return editor


def submit_sign(server: LoggedServer, client: Client, name: str, expected: dict, slot: int) -> dict:
    before = client.state()["packets"]
    action = client.request("sign_update", **dict(zip(("x", "y", "z"), SIGNS[name][0])),
                            slot=slot, messages=expected["front_text" if slot == 1 else "back_text"]["messages"])
    boundary = processing_boundary(server, client)
    observed = received_sign(client, name, expected, before)
    return {"sent": action, "before_sequence": before, "boundary": boundary, "received": observed}


def fresh_sign_chunk(server: LoggedServer, client: Client, name: str, expected: dict) -> dict:
    # Leave the view before returning. A NEW chunk packet is mandatory even
    # though the forced fixture remains loaded on the server throughout.
    before = client.state()["packets"]
    teleport(server, client, (0, 200, 0))
    client.wait_state(lambda state: any(chunk["sequence"] > before and chunk["x"] == 0 and chunk["z"] == 0
                                        for chunk in state["chunk_packets"]))
    boundary = processing_boundary(server, client)
    before_return = client.state()["packets"]
    sign_teleport(server, client, name, 1)
    observed = received_sign(client, name, expected, before_return, "chunk")
    return {"away_boundary": boundary, "before_return_sequence": before_return, "received": observed}


def sign_live_case(server: LoggedServer, client: Client, name: str) -> dict:
    initial = fresh_sign(server, client, name)
    editor = open_sign(server, client, name, 1)
    changed = sign_state(name, "data")
    changed["front_text"].update(color="green", has_glowing_text=False)
    changed["back_text"].update(color="yellow", has_glowing_text=True)
    edits = [edit_sign_data(server, client, name, changed)]
    for waxed in (True, False):
        changed["is_waxed"] = waxed
        edits.append(edit_sign_data(server, client, name, changed))
    require(not sign_packets(client.state(), name, editor["sequence"], "editor"), "Unexpected replacement editor")
    changed["front_text"]["messages"] = [f"{name}/owner/front", "retained", "after data", "four"]
    front = submit_sign(server, client, name, changed, 1)
    back_editor = open_sign(server, client, name, 0)
    changed["back_text"]["messages"] = [f"{name}/owner/back", "north", "preserved front", "four"]
    back = submit_sign(server, client, name, changed, 0)
    return {"initial": initial, "front_editor": editor, "live_edits": edits,
            "retained_owner_submission": front, "back_editor": back_editor, "back_submission": back}


def sign_editor_none_case(server: LoggedServer, client: Client, name: str) -> dict:
    initial = fresh_sign(server, client, name)
    expected = sign_state(name, "base")
    before = client.state()["packets"]
    attack = client.request("sign_update", **dict(zip(("x", "y", "z"), SIGNS[name][0])),
                            slot=1, messages=["UNSOLICITED", "must", "not", "apply"])
    boundary = processing_boundary(server, client)
    state = client.state()
    require(not sign_packets(state, name, before, "editor"), "Fixture unexpectedly granted editor ownership")
    # A real unchanged initial BE packet, a typed server read, and a fresh
    # received chunk are independent witnesses; absence of BEData cannot pass.
    predicate = f"if data block {pos_text(SIGNS[name][0])} {sign_snbt(expected)}"
    suite.check_predicate(server, predicate)
    for packet in sign_packets(state, name, before):
        if packet["kind"] != "editor":
            require(packet["data"] == expected, "Unsolicited submission changed received sign data")
    chunk = fresh_sign_chunk(server, client, name, expected)
    editor = open_sign(server, client, name, 1)
    expected["front_text"]["messages"] = [f"{name}/authorized", "positive", "same face", "four"]
    legitimate = submit_sign(server, client, name, expected, 1)
    return {"initial": initial, "unsolicited": attack, "before_sequence": before, "boundary": boundary,
            "unchanged_server_predicate": predicate, "fresh_unchanged_chunk": chunk,
            "positive_editor": editor, "legitimate_submission": legitimate}


def waxed_interaction(server: LoggedServer, client: Client, name: str, expected: dict, slot: int) -> dict:
    require(expected["is_waxed"], "Waxed interaction needs a positively observed waxed sign")
    observations = [packet for packet in sign_packets(client.state(), name, 0) if packet["kind"] != "editor"]
    require(observations and observations[-1]["data"] == expected, "Missing received waxed-state precondition")
    sign_teleport(server, client, name, slot)
    before = interact_sign(client, name, slot)
    boundary = processing_boundary(server, client)
    state = client.state()
    require(not sign_packets(state, name, before, "editor"), "Waxed sign opened an editor after processing boundary")
    return {"before_sequence": before, "slot": slot, "received_precondition": observations[-1], "boundary": boundary,
            "packets": sign_packets(state, name, before)}


def sign_wax_case(server: LoggedServer, client: Client, name: str) -> dict:
    initial = fresh_sign(server, client, name)
    expected = sign_state(name, "base")
    control = open_sign(server, client, name, 1)
    expected["front_text"]["messages"] = [f"{name}/control", "south", "positive", "four"]
    positive = submit_sign(server, client, name, expected, 1)
    expected["is_waxed"] = True
    wax = edit_sign_data(server, client, name, expected)
    blocked = waxed_interaction(server, client, name, expected, 0)
    expected["is_waxed"] = False
    unwax = edit_sign_data(server, client, name, expected)
    editor = open_sign(server, client, name, 0)
    expected["back_text"]["messages"] = [f"{name}/resumed", "north", "positive", "four"]
    resumed = submit_sign(server, client, name, expected, 0)
    expected["is_waxed"] = True
    persisted = edit_sign_data(server, client, name, expected)
    return {"initial": initial, "unwaxed_control_editor": control, "unwaxed_control_submission": positive,
            "wax": wax, "blocked_interaction": blocked, "unwax": unwax,
            "resumed_editor": editor, "resumed_submission": resumed, "persisted": persisted,
            "persisted_state": deepcopy(expected)}


def sign_restart_case(server: LoggedServer, client: Client, name: str, expected: dict) -> dict:
    chunk = fresh_sign_chunk(server, client, name, expected)
    blocked = waxed_interaction(server, client, name, expected, 1)
    expected = deepcopy(expected)
    expected["is_waxed"] = False
    unwax = edit_sign_data(server, client, name, expected)
    edits = []
    for slot in (1, 0):
        editor = open_sign(server, client, name, slot)
        expected["front_text" if slot == 1 else "back_text"]["messages"] = [f"{name}/restart/{slot}", "editable", "other intact", "four"]
        edits.append({"editor": editor, "submission": submit_sign(server, client, name, expected, slot)})
    return {"after_clean_restart_chunk": chunk, "waxed_interaction": blocked, "unwax": unwax, "edits": edits}


def fixture_packets(state: dict, fixture: tuple, after: int, kind: str | None = None) -> list[dict]:
    return [packet for packet in state["sign_packets"] if packet["sequence"] > after
            and tuple(packet[axis] for axis in ("x", "y", "z")) == fixture[0]
            and (kind is None or packet["kind"] == kind)]


def received_rich(client: Client, fixture: tuple, expected: dict, after: int, kind: str = "block_entity") -> dict:
    def matches(state: dict) -> bool:
        packets = fixture_packets(state, fixture, after, kind)
        return bool(packets and packets[-1]["block_entity_type"] == fixture[2]
                    and packets[-1].get("rich_data") == expected and "component_nbt" in packets[-1])
    state = client.wait_state(matches)
    return fixture_packets(state, fixture, after, kind)[-1]


def component_state(name: str, stage: str) -> dict:
    data = sign_state(name, stage)
    data["allow_op_features"] = False
    for key in ("front_text", "back_text"):
        data[key]["filtered_messages"] = deepcopy(data[key]["messages"])
    return data


def component_snbt(data: dict) -> str:
    def encode(value) -> str:
        if isinstance(value, bool):
            return f"{int(value)}b"
        if isinstance(value, dict):
            return "{" + ",".join(json.dumps(key) + ":" + encode(item) for key, item in value.items()) + "}"
        if isinstance(value, list):
            return "[" + ",".join(encode(item) for item in value) + "]"
        return json.dumps(value)
    # Use homogeneous compound input. Vanilla may collapse plain literal
    # components to scalars and envelope those in the actual outgoing NBT list.
    authored = deepcopy(data)
    for key in ("front_text", "back_text"):
        for field in ("messages", "filtered_messages"):
            authored[key][field] = [{"text": line} if isinstance(line, str) else line for line in authored[key][field]]
    return encode(authored)


def fixture_teleport(server: LoggedServer, client: Client, fixture: tuple, slot: int = 1) -> None:
    x, _, z = fixture[0]
    target = {"x": x + 0.5, "y": 200, "z": z + (2.5 if slot == 1 else -1.5)}
    before = client.state()["teleport_confirmations"]
    server.command(f"tp {NAME} {target['x']} 200 {target['z']} {180 if slot == 1 else 0} 0")
    client.wait_state(lambda state: state["teleport_confirmations"] > before
                      and all(abs((state.get("server_position") or {}).get(axis, float("inf")) - value) < 0.01
                              for axis, value in target.items()))


def edit_rich(server: LoggedServer, client: Client, fixture: tuple, expected: dict, command: str | None = None) -> dict:
    before = client.state()["packets"]
    command = command or f"data merge block {pos_text(fixture[0])} {component_snbt(expected)}"
    output = server.command(command)
    ticks = step_exact(server, 1)
    observed = received_rich(client, fixture, expected, before)
    return {"command": command, "output": output, "ticks": ticks, "before_sequence": before, "received": observed}


def fresh_rich(server: LoggedServer, client: Client, fixture: tuple, expected: dict) -> dict:
    close_menu(client)
    server.command(f"clear {NAME}")
    fixture_teleport(server, client, fixture)
    pos, block, _ = fixture
    server.command(f"setblock {pos_text(pos)} air")
    support = (pos[0], pos[1] + (1 if fixture[2] == 8 else -1), pos[2])
    server.command(f"setblock {pos_text(support)} stone")
    server.command(f"setblock {pos_text(pos)} {block}")
    suite.check_predicate(server, f"if block {pos_text(pos)} {block} if block {pos_text(support)} stone")
    return edit_rich(server, client, fixture, expected)


def prepare_nonop_boundary(server: LoggedServer, client: Client) -> dict:
    """Arm the one-shot trigger BEFORE sending the action under observation."""
    fence = int(uuid.uuid4().hex[:7], 16)
    outputs = [server.command(f"scoreboard players set {NAME} signfence -1"),
               server.command(f"scoreboard players enable {NAME} signfence")]
    return {"nonce": fence, "before_sequence": client.state()["packets"], "setup_output": outputs}


def marker_boundary(server: LoggedServer, client: Client, before: int) -> dict:
    # Non-op commands cost 20 throttle ticks. Allow two commands' decay for
    # the permission control plus its trigger without accumulating spam debt.
    ticks = step_exact(server, 40)
    marker = "SIGN_NONOP_" + uuid.uuid4().hex
    output = server.command("say " + marker)
    state = client.wait_state(lambda state: any(message["sequence"] > before and marker in message["text"]
                                               for message in state["chat_messages"]))
    received = next(message for message in state["chat_messages"] if message["sequence"] > before and marker in message["text"])
    return {"ticks": ticks, "server_output": output, "received_marker": received}


def finish_nonop_boundary(server: LoggedServer, client: Client, prepared: dict) -> dict:
    sent = client.request("trigger", objective="signfence", value=prepared["nonce"])
    predicate = f"if score {NAME} signfence matches {prepared['nonce']}"
    deadline = time.monotonic() + 5
    while True:
        try:
            suite.check_predicate(server, predicate)
            break
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)
    return {**prepared, "client_command": sent, "server_predicate": predicate,
            **marker_boundary(server, client, prepared["before_sequence"])}


def require_sign_score(server: LoggedServer, value: int) -> str:
    predicate = f"if score {NAME} signcmd matches {value}"
    suite.check_predicate(server, predicate)
    return predicate


def nonop_control(server: LoggedServer, client: Client) -> dict:
    """A forbidden client command must produce a real denial without changing
    score 17. A working nonop trigger then orders that command on this socket.
    """
    output = server.command(f"deop {NAME}")
    server.command(f"scoreboard players set {NAME} signcmd 17")
    prepared = prepare_nonop_boundary(server, client)
    with interaction_deadline(server, "nonop permission/trigger control"):
        forbidden = client.request("command", text="scoreboard players add @s signcmd 1000")
        boundary = finish_nonop_boundary(server, client, prepared)
        predicate = require_sign_score(server, 17)
        messages = [message for message in client.state()["chat_messages"] if message["sequence"] > prepared["before_sequence"]]
        denials = [message for message in messages if any(token in message["text"].lower() for token in
                   ("command.unknown.command", "unknown or incomplete command", "commands.generic.permission", "permission"))]
        require(denials, f"Missing actual forbidden-command response: {messages}")
        # TriggerCommand locks after successful use. A second command must fail,
        # keep the first nonce and still allow a later received marker.
        before = client.state()["packets"]
        second = client.request("trigger", objective="signfence", value=prepared["nonce"] + 1)
        def locked_messages(state: dict) -> list[dict]:
            return [message for message in state["chat_messages"] if message["sequence"] > before
                    and any(token in message["text"].lower() for token in
                            ("commands.trigger.failed.unprimed", "cannot trigger this objective yet", "not enabled"))]
        state = client.wait_state(lambda state: bool(locked_messages(state)), timeout=5)
        locked = locked_messages(state)
        locked_boundary = marker_boundary(server, client, before)
        suite.check_predicate(server, boundary["server_predicate"])
        require(locked, "No one-shot trigger failure response")
    return {"deop_output": output, "forbidden_client_command": forbidden, "denials": denials,
            "unchanged_score_predicate": predicate, "boundary": boundary,
            "one_shot": {"sent": second, "boundary": locked_boundary, "denial": locked,
                         "retained_score_predicate": boundary["server_predicate"]}}


def fixture_interaction(client: Client, fixture: tuple, slot: int = 1) -> dict:
    return client.request("use_item_on", **dict(zip(("x", "y", "z"), fixture[0])), face=3 if slot == 1 else 2)


def fixture_editor(server: LoggedServer, client: Client, fixture: tuple, slot: int) -> dict:
    fixture_teleport(server, client, fixture, slot)
    before = client.state()["packets"]
    fixture_interaction(client, fixture, slot)
    state = client.wait_state(lambda state: bool(fixture_packets(state, fixture, before, "editor")))
    editor = fixture_packets(state, fixture, before, "editor")[-1]
    require(editor["slot"] == slot, editor)
    return editor


def submit_rich(server: LoggedServer, client: Client, fixture: tuple, messages: list[str], expected: dict, slot: int) -> dict:
    prepared = prepare_nonop_boundary(server, client)
    with interaction_deadline(server, "legitimate literal sign submission"):
        sent = client.request("sign_update", **dict(zip(("x", "y", "z"), fixture[0])), slot=slot, messages=messages)
        boundary = finish_nonop_boundary(server, client, prepared)
        observed = received_rich(client, fixture, expected, prepared["before_sequence"])
    return {"sent": sent, "boundary": boundary, "received": observed}


def click_rich(server: LoggedServer, client: Client, fixture: tuple, expected_score: int,
               disabled: bool = False, changed: dict | None = None) -> dict:
    fixture_teleport(server, client, fixture)
    prepared = prepare_nonop_boundary(server, client)
    with interaction_deadline(server, "sign click including command reentry"):
        sent = fixture_interaction(client, fixture)
        boundary = finish_nonop_boundary(server, client, prepared)
        predicate = require_sign_score(server, expected_score)
        observed = received_rich(client, fixture, changed, prepared["before_sequence"]) if changed is not None else None
        overlays = [message for message in client.state()["chat_messages"] if message["sequence"] > prepared["before_sequence"]
                    and message.get("overlay") is True and "sign.click_actions_disabled" in message["text"]]
        if disabled:
            require(overlays, "Missing fresh disabled-click overlay after processed interaction")
    return {"sent": sent, "boundary": boundary, "score_predicate": predicate, "changed_wire": observed, "disabled_overlays": overlays}


def fresh_rich_chunk(server: LoggedServer, client: Client, fixture: tuple, expected: dict) -> dict:
    prepared = prepare_nonop_boundary(server, client)
    teleport(server, client, (0, 200, 0))
    client.wait_state(lambda state: any(chunk["sequence"] > prepared["before_sequence"] and chunk["x"] == 0 and chunk["z"] == 0
                                        for chunk in state["chunk_packets"]))
    boundary = finish_nonop_boundary(server, client, prepared)
    before = client.state()["packets"]
    fixture_teleport(server, client, fixture)
    return {"away_boundary": boundary, "before_return_sequence": before,
            "received": received_rich(client, fixture, expected, before, "chunk")}


def command_component(command: str = SIGN_COMMAND) -> dict:
    return {"text": "RUN", "click_event": {"action": "run_command", "command": command}}


def literal_identity(server: LoggedServer, fixture: tuple, line: str) -> str:
    # NBT string equality cannot match a compound component with identical text.
    predicate = f'if data block {pos_text(fixture[0])} {{front_text:{{messages:[{json.dumps(line)}]}}}}'
    suite.check_predicate(server, predicate)
    return predicate


def literal_command_case(server: LoggedServer, client: Client, name: str) -> dict:
    fixture = COMMAND_SIGNS[name]
    control = nonop_control(server, client)
    expected = component_state(name, "literal-base")
    initial = fresh_rich(server, client, fixture, expected)
    editor = fixture_editor(server, client, fixture, 1)
    messages = [LITERAL_COMMAND, "§cRed§r §khidden§r §G stays §x stays", "§AUpper§r §7Gray§r", "literal fourth"]
    received_messages = [LITERAL_COMMAND, "Red hidden §G stays §x stays", "Upper Gray", "literal fourth"]
    expected["front_text"].update(messages=received_messages, filtered_messages=received_messages.copy())
    submitted = submit_rich(server, client, fixture, messages, expected, 1)
    typed = literal_identity(server, fixture, LITERAL_COMMAND)
    expected.update(is_waxed=True, allow_op_features=True)
    worst_case = edit_rich(server, client, fixture, expected)
    blocked = click_rich(server, client, fixture, 17)
    chunk = fresh_rich_chunk(server, client, fixture, expected)
    chunk_blocked = click_rich(server, client, fixture, 17)
    # Same command, same flags and same player, with actual administrator-authored
    # structure. This positive control must execute before restoring the literal.
    structured = deepcopy(expected)
    structured["front_text"]["messages"][0] = command_component()
    structured["front_text"]["filtered_messages"] = deepcopy(structured["front_text"]["messages"])
    positive_fixture = edit_rich(server, client, fixture, structured)
    positive = click_rich(server, client, fixture, 18)
    restored = edit_rich(server, client, fixture, expected)
    literal_identity(server, fixture, LITERAL_COMMAND)
    return {"nonop_control": control, "initial": initial, "editor": editor, "literal_submission": submitted,
            "literal_nbt_predicate": typed, "worst_case_flags": worst_case, "no_execution": blocked,
            "fresh_chunk": chunk, "fresh_chunk_no_execution": chunk_blocked,
            "structured_positive_fixture": positive_fixture, "structured_positive": positive,
            "restored_literal": restored, "persisted_state": deepcopy(expected)}


def structured_command_case(server: LoggedServer, client: Client, name: str) -> dict:
    fixture = RICH_SIGNS[name]
    control = nonop_control(server, client)
    expected = component_state(name, "structured")
    expected.update(is_waxed=True, allow_op_features=True)
    expected["front_text"]["messages"] = [command_component(), "", "", ""]
    expected["front_text"]["filtered_messages"] = deepcopy(expected["front_text"]["messages"])
    initial = fresh_rich(server, client, fixture, expected)
    positive = click_rich(server, client, fixture, 18)
    expected["allow_op_features"] = False
    explicit_false = edit_rich(server, client, fixture, expected)
    blocked = click_rich(server, client, fixture, 18, disabled=True)
    expected["allow_op_features"] = True
    established_true = edit_rich(server, client, fixture, expected)
    suite.check_predicate(server, f"if data block {pos_text(fixture[0])} {{allow_op_features:1b}}")
    expected["allow_op_features"] = False
    absent = edit_rich(server, client, fixture, expected, f"data remove block {pos_text(fixture[0])} allow_op_features")
    suite.check_predicate(server, f"unless data block {pos_text(fixture[0])} allow_op_features")
    default_blocked = click_rich(server, client, fixture, 18, disabled=True)
    expected["allow_op_features"] = True
    reenabled = edit_rich(server, client, fixture, expected)
    end_control = click_rich(server, client, fixture, 19)
    return {"nonop_control": control, "initial": initial, "true_positive": positive, "explicit_false": explicit_false,
            "false_blocked": blocked, "established_true_before_removal": established_true, "removed_key": absent,
            "default_blocked": default_blocked, "reenabled": reenabled, "end_positive": end_control}


def command_reentry_case(server: LoggedServer, client: Client, name: str) -> dict:
    fixture = RICH_SIGNS[name]
    control = nonop_control(server, client)
    expected = component_state(name, "self-unwax")
    expected.update(is_waxed=True, allow_op_features=True)
    command = f"data merge block {pos_text(fixture[0])} {{is_waxed:0b}}"
    expected["front_text"]["messages"] = [command_component(command), "", "", ""]
    expected["front_text"]["filtered_messages"] = deepcopy(expected["front_text"]["messages"])
    initial = fresh_rich(server, client, fixture, expected)
    expected["is_waxed"] = False
    unwaxed = click_rich(server, client, fixture, 17, changed=expected)
    suite.check_predicate(server, f"if data block {pos_text(fixture[0])} {{is_waxed:0b}}")
    editor = fixture_editor(server, client, fixture, 0)
    messages = [name + "/reentry/back", "live", "after self-unwax", "four"]
    expected["back_text"].update(messages=messages, filtered_messages=messages.copy())
    edited = submit_rich(server, client, fixture, messages, expected, 0)
    return {"nonop_control": control, "initial": initial, "self_unwax": unwaxed,
            "back_editor": editor, "back_submission": edited, "interaction_deadline_seconds": 20}


def rich_text_case(server: LoggedServer, client: Client, name: str) -> dict:
    fixture = RICH_SIGNS[name]
    control = nonop_control(server, client)
    expected = component_state(name, "styled")
    expected["front_text"]["messages"] = [{"text": "raw zero", "bold": True, "color": "red"}, "raw one",
                                          {"text": "raw two", "bold": True, "color": "blue"}, ""]
    expected["front_text"]["filtered_messages"] = [{"text": "filtered zero", "bold": True, "color": "red"}, "filtered one",
                                                   {"text": "filtered two", "bold": True, "color": "blue"}, "filtered three"]
    expected["back_text"]["filtered_messages"] = ["back filtered zero", {"text": "back filtered one", "bold": True}, "back filtered two", ""]
    initial = fresh_rich(server, client, fixture, expected)
    chunk = fresh_rich_chunk(server, client, fixture, expected)
    editor = fixture_editor(server, client, fixture, 1)
    messages = ["edited zero", "edited one", "edited two", "edited three"]
    expected["front_text"]["messages"] = [{**line, "text": messages[index]} if isinstance(line, dict) else messages[index]
                                          for index, line in enumerate(expected["front_text"]["messages"])]
    expected["front_text"]["filtered_messages"] = deepcopy(expected["front_text"]["messages"])
    edited = submit_rich(server, client, fixture, messages, expected, 1)
    expected.update(is_waxed=True, allow_op_features=True)
    expected["front_text"]["messages"][0]["click_event"] = command_component()["click_event"]
    expected["front_text"]["filtered_messages"] = deepcopy(expected["front_text"]["messages"])
    persisted = edit_rich(server, client, fixture, expected)
    positive = click_rich(server, client, fixture, 18)
    return {"nonop_control": control, "initial_raw_and_filtered": initial, "fresh_chunk": chunk, "editor": editor,
            "styled_literal_edit": edited, "persisted_styled_click": persisted, "positive_click": positive,
            "persisted_state": deepcopy(expected), "scope": "Selected literal/style/click NBT, not full component rendering or content moderation."}


def command_restart_case(server: LoggedServer, client: Client, name: str, literal: dict, rich: dict) -> dict:
    control = nonop_control(server, client)
    literal_chunk = fresh_rich_chunk(server, client, COMMAND_SIGNS[name], literal)
    typed = literal_identity(server, COMMAND_SIGNS[name], LITERAL_COMMAND)
    blocked = click_rich(server, client, COMMAND_SIGNS[name], 17)
    rich_chunk = fresh_rich_chunk(server, client, RICH_SIGNS[name], rich)
    positive = click_rich(server, client, RICH_SIGNS[name], 18)
    return {"nonop_control": control, "literal_chunk_after_restart": literal_chunk, "literal_nbt_predicate": typed,
            "literal_no_execution": blocked, "styled_click_chunk_after_restart": rich_chunk, "post_restart_positive_click": positive}


def sign_numeric_flags_case(server: LoggedServer, client: Client) -> dict:
    expected = component_state("sign", "numeric")
    initial = fresh_rich(server, client, EDITOR_SIGN, expected)
    phases = []
    for number, direct, glow in [("1", True, True), ("256", False, True), ("257s", True, True),
                                 ("256L", False, True), ("0.5f", False, True),
                                 ("-0.5d", True, True), ("0d", False, False)]:
        expected.update(is_waxed=direct, allow_op_features=direct)
        expected["front_text"]["has_glowing_text"] = glow
        command = (f"data merge block {pos_text(EDITOR_SIGN[0])} "
                   f"{{is_waxed:{number},allow_op_features:{number},front_text:{{has_glowing_text:{number}}}}}")
        phases.append(edit_rich(server, client, EDITOR_SIGN, expected, command))
    return {"initial": initial, "phases": phases,
            "scope": "Direct low-byte flags versus codec nonzero glow; nested merge retains both faces and all lines."}


def far_editor_submission(server: LoggedServer, client: Client, messages: list[str]) -> dict:
    """Establish socket ordering without advancing a tick. Distance expiry must
    not run between the submitted sign packet and this successful op command.
    """
    fence = int(uuid.uuid4().hex[:7], 16)
    output = server.command("scoreboard players set SignFence blockdata -1")
    sent = client.request("sign_update", **dict(zip(("x", "y", "z"), EDITOR_SIGN[0])), slot=1, messages=messages)
    command = client.request("command", text=f"scoreboard players set SignFence blockdata {fence}")
    predicate = f"if score SignFence blockdata matches {fence}"
    deadline = time.monotonic() + 5
    while True:
        try:
            suite.check_predicate(server, predicate)
            break
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)
    require_frozen(server)
    return {"sent": sent, "setup_output": output, "ordered_client_command": command,
            "processing_predicate": predicate, "tick_steps_before_predicate": 0}


def sign_editor_tick_case(server: LoggedServer, client: Client) -> dict:
    server.command(f"op {NAME}")
    expected = component_state("sign", "tick-base")
    initial = fresh_rich(server, client, EDITOR_SIGN, expected)
    editor = fixture_editor(server, client, EDITOR_SIGN, 1)
    require_frozen(server)
    before_away = client.state()["packets"]
    teleport(server, client, (0, 200, 0))
    client.wait_state(lambda state: any(chunk["sequence"] > before_away and chunk["x"] == 0 and chunk["z"] == 0
                                        for chunk in state["chunk_packets"]))
    messages = ["retained before tick", "far editor", "UUID remains", "four"]
    with interaction_deadline(server, "retained editor before any tick"):
        accepted = far_editor_submission(server, client, messages)
        # Typed server state is mandatory while the client cannot see the sign.
        typed = literal_identity(server, EDITOR_SIGN, messages[0])
        boundary = processing_boundary(server, client)
    expected["front_text"].update(messages=messages, filtered_messages=messages.copy())
    before_return = client.state()["packets"]
    fixture_teleport(server, client, EDITOR_SIGN)
    retained_chunk = received_rich(client, EDITOR_SIGN, expected, before_return, "chunk")
    reopened = fixture_editor(server, client, EDITOR_SIGN, 1)
    teleport(server, client, (0, 200, 0))
    suite.check_predicate(server, f"if loaded {pos_text(EDITOR_SIGN[0])}")
    expired_tick = step_exact(server, 1)
    with interaction_deadline(server, "editor expired by exactly one far tick"):
        ignored = far_editor_submission(server, client, ["must be ignored", "expired", "one tick", "four"])
        unchanged = literal_identity(server, EDITOR_SIGN, messages[0])
        expired_boundary = processing_boundary(server, client)
    before_return = client.state()["packets"]
    fixture_teleport(server, client, EDITOR_SIGN)
    unchanged_chunk = received_rich(client, EDITOR_SIGN, expected, before_return, "chunk")
    positive_editor = fixture_editor(server, client, EDITOR_SIGN, 1)
    messages = ["legitimate after expiry", "near again", "positive", "four"]
    expected["front_text"].update(messages=messages, filtered_messages=messages.copy())
    before = client.state()["packets"]
    positive_sent = client.request("sign_update", **dict(zip(("x", "y", "z"), EDITOR_SIGN[0])), slot=1, messages=messages)
    positive_boundary = processing_boundary(server, client)
    positive = received_rich(client, EDITOR_SIGN, expected, before)
    return {"initial": initial, "near_editor": editor, "far_before_tick_submission": accepted,
            "accepted_typed_predicate_before_tick": typed, "boundary_after_acceptance": boundary,
            "accepted_fresh_chunk": retained_chunk, "reopened_editor": reopened, "far_expiry_tick": expired_tick,
            "expired_submission": ignored, "unchanged_typed_predicate": unchanged,
            "expired_boundary": expired_boundary, "unchanged_fresh_chunk": unchanged_chunk,
            "positive_editor": positive_editor, "positive_sent": positive_sent,
            "positive_boundary": positive_boundary, "positive_received": positive,
            "scope": "Standing sign editor retention before tick and far expiry after one tick; no threshold inference."}


def run_side(kind: str, directory: Path, args: argparse.Namespace, report: dict) -> list[dict]:
    server = LoggedServer(kind, directory, args)
    results: dict[str, dict] = {}
    processes = report["processes"][kind] = []
    clients: list[Client] = []
    server.owned_clients = clients
    server.abort_events = []
    report.setdefault("watchdogs", {})[kind] = server.abort_events
    failures: list[str] = []

    def record(test_id: str, action) -> dict:
        entry = suite.recorded(test_id, (CASES[test_id],), action)
        results[test_id] = entry
        if clients and (clients[-1].failure or clients[-1].process.poll() is not None):
            server.abort(f"Protocol client broken during {test_id}: {clients[-1].failure or clients[-1].process.returncode}")
        if server.broken:
            raise RuntimeError(server.broken)
        return entry

    def stop() -> None:
        if server.broken and server.process is not None:
            # abort() already sent TERM and scheduled KILL. Do not enter the
            # ordinary stop path's 45+15 second wait after a reentry deadlock.
            try:
                code = server.process.wait(timeout=6)
            except subprocess.TimeoutExpired:
                server.signal(signal.SIGKILL)
                code = server.process.wait(timeout=2)
            if server.reader is not None:
                server.reader.join(timeout=2)
        else:
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
            server.command("forceload add 0 0")
            suite.wait_loaded(server, x=0, z=0)
            if boot == "initial":
                server.command("fill 4096 199 0 4111 199 7 stone")
                server.command("difficulty peaceful")
                server.command("scoreboard objectives add blockdata dummy")
                server.command("scoreboard objectives add signcmd dummy")
                server.command("scoreboard objectives add signfence trigger")
            client = connect("wire-" + boot)
            server.command(f"gamemode survival {NAME}")
            if boot == "initial":
                for name in CONTAINERS:
                    record(f"block_data.{name}_menu", lambda name=name: menu_case(server, client, name))
                    record(f"block_data.{name}_swap", lambda name=name: swap_case(server, client, name))
                for name in LOOT_POSITIONS:
                    record(f"block_data.{name}_cleared_loot_menu", lambda name=name: loot_case(server, client, name))
                record("block_data.barrel_pending_empty_loot", lambda: pending_barrel_case(server, client))
                record("block_data.barrel_spectator_pending_loot", lambda: spectator_barrel_case(server, client))
                server.command("fill 4096 199 8 4111 199 15 stone")
                server.command(f"op {NAME}")
                for name in SIGNS:
                    for case, action in (("live", sign_live_case), ("editor_none", sign_editor_none_case), ("wax", sign_wax_case)):
                        record(f"block_data.{name}_{case}", lambda name=name, action=action: action(server, client, name))
                for name in SIGNS:
                    for case, action in (("literal_command", literal_command_case), ("structured_command", structured_command_case),
                                         ("command_reentry", command_reentry_case), ("rich_text", rich_text_case)):
                        record(f"block_data.{name}_{case}", lambda name=name, action=action: action(server, client, name))
                record("block_data.sign_editor_tick", lambda: sign_editor_tick_case(server, client))
                record("block_data.sign_numeric_flags", lambda: sign_numeric_flags_case(server, client))
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
                server.command(f"op {NAME}")
                for name in SIGNS:
                    test_id = f"block_data.{name}_restart"
                    prerequisite = results[f"block_data.{name}_wax"]
                    if prerequisite["result"] != "passed":
                        results[test_id] = {"id": test_id, "features": [CASES[test_id]], "result": "skipped",
                                            "detail": {"reason": "Wax, legitimate edits and persisted wire-state prerequisite failed."}}
                        continue
                    expected = prerequisite["detail"]["persisted_state"]
                    record(test_id, lambda name=name, expected=expected: sign_restart_case(server, client, name, expected))
                for name in SIGNS:
                    test_id = f"block_data.{name}_command_restart"
                    prerequisites = [results[f"block_data.{name}_{case}"] for case in ("literal_command", "rich_text")]
                    if any(entry["result"] != "passed" for entry in prerequisites):
                        results[test_id] = {"id": test_id, "features": [CASES[test_id]], "result": "skipped",
                                            "detail": {"reason": "Literal non-execution and persisted styled click prerequisites must pass."}}
                        continue
                    literal, rich = [entry["detail"]["persisted_state"] for entry in prerequisites]
                    record(test_id, lambda name=name, literal=literal, rich=rich: command_restart_case(server, client, name, literal, rich))
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
            print(f"Running {kind} menus and signs on {socket.gethostname()} loopback: {run / kind}", flush=True)
            report["servers"][kind] = run_side(kind, run / kind, args, report)
        if manifest() != report["harness_manifest"]:
            report["failed"].append("harness.changed_during_run")
        if suite.git_state() != report["source_state"]:
            report["failed"].append("source.changed_during_run")
        if (digest(args.rust_binary) != report["rust_binary_sha256"] or digest(original) != before
                or digest(official) != report["server_sha256"]):
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
