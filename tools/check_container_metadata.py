#!/usr/bin/env python3
"""26 focused paired 26.3/protocol-777 container metadata observations.

Fresh sequential loopback servers, actual menus/locks/SWAP, and clean restart.
Literal/basic-style titles and exact plain custom-name predicates only. Base
custom_data persistence is opaque metadata evidence, never component gameplay.
Unsupported loaded predicates and other rich titles remain explicitly unverified.
No servers run without --accept-eula and an already cached official reference.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from typing import Callable
import uuid

sys.dont_write_bytecode = True
import oracle
from oracle import ROOT, TARGET
from parity import suite
from parity.bot import Client, NAME, delivered_chat
from parity.furnaces import digest, require_frozen, step_exact
from parity.protocol import require
from check_block_data import LoggedServer, WORLD_ERRORS, interaction_deadline, pos_text, teleport, close_menu

POSITIONS = {"barrel": (4097, 200, 1), "chest": (4101, 200, 1), "trapped_chest": (4105, 200, 1)}
BLOCK_ENTITY_TYPES = {"barrel": 26, "chest": 1, "trapped_chest": 2}
CASES = {f"container_metadata.{kind}_{case}": feature for kind in POSITIONS for case, feature in (
    ("title_recursive", "blocks.containers"), ("mainhand", "blocks.containers"),
    ("offhand", "blocks.containers"), ("creative", "blocks.containers"),
    ("spectator", "blocks.containers"), ("pending_loot", "blocks.containers"),
    ("deposit", "network.inventory_stream"), ("restart", "persistence.block_entities"))}
CASES.update({f"container_metadata.{kind}_double": "blocks.containers" for kind in ("chest", "trapped_chest")})
DEFERRED = [{"id": "container_metadata.unsupported_loaded_predicate", "result": "unverified",
             "reason": "Needs a valid Java-created world copied into Rust, semantic readback, and matching-key interaction. Rust live edits intentionally refuse unsupported predicates; loaded matching keys currently deny, a known compatibility gap."}]
KEY = "Ferraloom key"
LOCK = '{components:{"minecraft:custom_name":"' + KEY + '"}}'
GOLD = {"item_id": 1022, "count": 5}
STICK = 1060
PRIMARY = {"inner_jar_sha256": "a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6",
           "contracts": ["DataComponents lambda$static$6/$9: ComponentSerialization.STREAM_CODEC; IDs 6/9",
                         "DataComponents.CUSTOM_DATA: persistent CustomData.CODEC; BlockEntity components: DataComponentMap.CODEC",
                         "ChestBlock.getBlockType: RIGHT=FIRST; getConnectedDirection: RIGHT counterclockwise / LEFT clockwise",
                         "ClientboundSoundPacket + SoundEvent.STREAM_CODEC; sound=119; block.chest.locked registry ID=348",
                         "DedicatedServerProperties: command-spam-threshold-seconds/chat-spam-threshold-seconds; defaults=10",
                         "ServerboundChatPacket.STREAM_CODEC + LastSeenMessages.Update: offset, fixed20BitSet, checksum byte",
                         "ServerboundPlayerActionPacket: SWAP_ITEM_WITH_OFFHAND ordinal=7; packet ID=41"]}


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def manifest() -> dict:
    files = ("tools/check_container_metadata.py", "tools/check_chat_throttle.py", "tools/check_block_data.py",
             "tests/block_data_protocol.test.ts")
    return {**suite.harness_manifest(), **{path: digest(ROOT / path) for path in files}}


def typed_predicate(server: LoggedServer, predicate: str) -> dict:
    suite.check_predicate(server, predicate)
    return {"predicate": predicate, "commands_log": str(server.directory / "commands.jsonl")}


def wait_predicate(server: LoggedServer, predicate: str, timeout: float = 5) -> dict:
    deadline = time.monotonic() + timeout
    while True:
        try:
            return typed_predicate(server, predicate)
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)


def snbt(value: object) -> str:
    """The fixture uses only compounds/lists/strings/bools/integers, no coercions."""
    if isinstance(value, bool):
        return "1b" if value else "0b"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(key) + ":" + snbt(entry) for key, entry in value.items()) + "}"
    if isinstance(value, list):
        return "[" + ",".join(map(snbt, value)) + "]"
    if isinstance(value, (str, int)):
        return json.dumps(value)
    raise TypeError(f"Unsupported fixture NBT {value!r}")


def set_hand(server: LoggedServer, client: Client, item: str = "air", *, offhand: bool = False) -> dict:
    slot = "weapon.offhand" if offhand else "weapon.mainhand"
    output = server.command(f"item replace entity {NAME} {slot} with minecraft:{item}")
    if item == "air":
        expected = None
    elif item == "stick":
        expected = {"item_id": STICK, "count": 1}
    else:
        match = re.fullmatch(r'stick\[(minecraft:(?:custom_name|item_name))=("(?:[^"\\]|\\.)*")\]', item)
        require(match is not None, f"Item outside the exact hand fixture: {item}")
        expected = {"item_id": STICK, "count": 1, "components": {match[1]: json.loads(match[2])}}
    index = 45 if offhand else 36
    state = client.wait_state(lambda state: state.get("player_inventory") is not None
                              and state["player_inventory"]["slots"][index] == expected)
    return {"output": output, "slot": slot, "inventory_slot": index, "expected_stack": expected,
            "received_inventory": state["player_inventory"]}


def key_item() -> str:
    return 'stick[minecraft:custom_name="' + KEY + '"]'


def fresh(server: LoggedServer, client: Client, kind: str) -> tuple[int, int, int]:
    pos = POSITIONS[kind]
    close_menu(client)
    server.command(f"gamemode survival {NAME}")
    server.command(f"clear {NAME}")
    teleport(server, client, pos)
    server.command(f"setblock {pos_text(pos)} air")
    server.command(f"setblock {pos_text(pos)} {kind}")
    server.command(f'data merge block {pos_text(pos)} {{CustomName:"{kind} box",lock:{LOCK}}}')
    typed_predicate(server, f'if data block {pos_text(pos)} {{CustomName:"{kind} box",lock:{LOCK}}}')
    return pos


def title_for(kind: str, edited: bool = False) -> dict:
    return {"text": ("Edited " if edited else "Styled ") + kind, "bold": True, "italic": False,
            "color": "gold", "extra": [{"text": " extra", "color": "red"}, {"text": " literal"}]}


def opened(client: Client, pos: tuple[int, int, int], title: dict, size: int = 27, expected: list | None = None) -> dict:
    close_menu(client)
    before = client.state()["packets"]
    sent = client.request("use_item_on", **dict(zip(("x", "y", "z"), pos)))

    def complete(state: dict) -> bool:
        menu = state.get("menu")
        return (isinstance(menu, dict) and menu["opened_sequence"] > before and len(menu["slots"]) == size + 36
                and menu["menu_type"] == (2 if size == 27 else 5) and menu["carried"] is None)

    state = client.wait_state(complete)
    packets = [entry for entry in state["menu_packets"] if entry["sequence"] > before
               and entry["container_id"] == state["menu"]["container_id"]]
    screens = [entry for entry in packets if entry["kind"] == "open"]
    require(len(screens) == 1 and screens[0].get("title_component") == title, {"expected_title": title, "packets": packets})
    require(any(entry["kind"] == "content" and len(entry["slots"]) == size + 36 for entry in packets), packets)
    if expected is not None:
        require(state["menu"]["slots"][:size] == expected, {"expected_contents": expected, "menu": state["menu"]})
    return {"sent": sent, "packets": packets, "menu": state["menu"]}


def boundary(server: LoggedServer, client: Client, before: int) -> dict:
    """A real client trigger effect plus received console marker, after feedback.

    Forty listener ticks prevent these ordinary non-op fences accumulating spam
    debt. They are processing evidence, not precise throttle/timing assertions.
    """
    ticks = step_exact(server, 40)
    value = int(uuid.uuid4().hex[:7], 16)
    server.command(f"scoreboard players enable {NAME} metafence")
    sent = client.request("trigger", objective="metafence", value=value)
    effect = wait_predicate(server, f"if score {NAME} metafence matches {value}")
    marker = "META_" + uuid.uuid4().hex
    output = server.command("say " + marker)
    state = client.wait_state(lambda state: delivered_chat(state, marker) is not None)
    require(state["packets"] > before, state)
    return {"ticks": ticks, "sent": sent, "effect": effect, "marker_output": output,
            "received_marker": delivered_chat(state, marker)}


def private_chunk(server: LoggedServer, client: Client, kind: str) -> dict:
    """Require the container's actual entry, with absent NBT, in a fresh chunk."""
    pos = POSITIONS[kind]
    close_menu(client)
    before = client.state()["packets"]
    teleport(server, client, (0, 200, 0))
    client.wait_state(lambda state: any(chunk["sequence"] > before and chunk["x"] == 0 and chunk["z"] == 0
                                        for chunk in state["chunk_packets"]))
    fence = boundary(server, client, before)
    before_return = client.state()["packets"]
    teleport(server, client, pos)
    state = client.wait_state(lambda state: any(chunk["sequence"] > before_return
                              and chunk["x"] == pos[0] // 16 and chunk["z"] == pos[2] // 16
                              for chunk in state["chunk_packets"]))
    chunks = [chunk for chunk in state["chunk_packets"] if chunk["sequence"] > before_return
              and chunk["x"] == pos[0] // 16 and chunk["z"] == pos[2] // 16]
    entries = [entry for chunk in chunks for entry in chunk["block_entities"]
               if (entry["x"], entry["y"], entry["z"]) == pos]
    require(bool(entries), {"missing_container_entry": pos, "chunks": chunks})
    require(all(entry["block_entity_type"] == BLOCK_ENTITY_TYPES[kind] and entry["data"] is None
                for entry in entries), {"private_container_nbt_exposed": entries})
    return {"away_boundary": fence, "before_return_sequence": before_return, "received_entries": entries,
            "chunk_sequences": [chunk["sequence"] for chunk in chunks]}


def denied(server: LoggedServer, client: Client, pos: tuple[int, int, int], *, pending_spectator: bool = False,
           expected_name: str | None = None) -> dict:
    close_menu(client)
    before = client.state()["packets"]
    sent = client.request("use_item_on", **dict(zip(("x", "y", "z"), pos)))
    translation = "container.spectatorCantOpen" if pending_spectator else "container.isLocked"

    def feedback(state: dict) -> bool:
        overlay = any(entry["sequence"] > before and entry.get("overlay") is True
                      and isinstance(entry.get("component_nbt"), dict)
                      and entry["component_nbt"].get("translate") == translation
                      and (expected_name is None or entry["component_nbt"].get("with") == [expected_name])
                      and (entry["component_nbt"].get("color") == "red" if pending_spectator
                           else "color" not in entry["component_nbt"]) for entry in state["chat_messages"])
        sound = pending_spectator or any(entry["sequence"] > before
                                        and (entry["registry_id"] == 348 or entry["sound_id"] == "minecraft:block.chest.locked")
                                        and entry["volume"] == 1 and entry["pitch"] == 1 for entry in state["sounds"])
        return overlay and sound and state["block_ack"] >= sent["interaction_sequence"]

    observed = client.wait_state(feedback)
    fence = boundary(server, client, before)
    final = client.state()
    require(final["menu"] is None and not any(entry["sequence"] > before and entry["kind"] == "open"
                                              for entry in final["menu_packets"]), "Denied action opened a screen before the actual processing boundary")
    return {"before_sequence": before, "sent": sent, "ack": observed["block_ack"], "processing_boundary": fence,
            "overlays": [entry for entry in observed["chat_messages"] if entry["sequence"] > before and entry.get("overlay")],
            "sounds": [entry for entry in observed["sounds"] if entry["sequence"] > before], "no_open_screen": True}


def title_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    initial_chunk = private_chunk(server, client, kind)
    hand = set_hand(server, client, key_item())
    plain = opened(client, pos, {"text": kind + " box"})
    close_menu(client)
    server.command(f"data merge block {pos_text(pos)} " + snbt({"CustomName": title_for(kind)}))
    styled = opened(client, pos, title_for(kind))
    close_menu(client)
    server.command(f"data merge block {pos_text(pos)} " + snbt({"CustomName": {"text": "Edited " + kind}}))
    # Java can canonicalize a plain extra to a string and envelope heterogeneous
    # lists. Check root style with typed NBT and the ENTIRE extras via OpenScreen.
    root_style = {key: value for key, value in title_for(kind, True).items() if key != "extra"}
    typed = typed_predicate(server, f"if data block {pos_text(pos)} " + snbt({"CustomName": root_style}))
    edited = opened(client, pos, title_for(kind, True))
    close_menu(client)
    edited_chunk = private_chunk(server, client, kind)
    return {"hand": hand, "plain": plain, "styled": styled, "recursive_merge": typed, "edited_reopen": edited,
            "initial_private_chunk": initial_chunk, "edited_private_chunk": edited_chunk}


def mainhand_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    observations = []
    for item in ("air", 'stick[minecraft:custom_name="wrong"]', 'stick[minecraft:item_name="' + KEY + '"]'):
        observations.append({"hand": set_hand(server, client, item),
                             "denied": denied(server, client, pos, expected_name=kind + " box")})
    correct = set_hand(server, client, key_item())
    positive = opened(client, pos, {"text": kind + " box"})
    close_menu(client)
    # Empty ItemPredicate matches any stack (including the empty main hand).
    server.command(f"data modify block {pos_text(pos)} lock set value {{}}")
    empty_hand = set_hand(server, client)
    empty_predicate = opened(client, pos, {"text": kind + " box"})
    close_menu(client)
    return {"negatives": observations, "correct_mainhand": correct, "positive": positive,
            "empty_predicate": {"hand": empty_hand, "opened": empty_predicate}}


def offhand_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    offhand = set_hand(server, client, key_item(), offhand=True)
    mainhand = set_hand(server, client, "stick")
    negative = denied(server, client, pos, expected_name=kind + " box")
    # Actual player-inventory hotbar/offhand SWAP, not a console replacement.
    sent = client.request("swap_hands")
    state = client.wait_state(lambda state: state.get("player_inventory") is not None
                              and state["player_inventory"]["slots"][36] == {
                                  "item_id": STICK, "count": 1, "components": {"minecraft:custom_name": KEY}})
    mainhand_key = state["player_inventory"]
    positive = opened(client, pos, {"text": kind + " box"})
    close_menu(client)
    return {"offhand": offhand, "wrong_mainhand": mainhand, "denied": negative,
            "actual_swap_hands": sent, "mainhand_key": mainhand_key, "positive": positive}


def creative_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    server.command(f"gamemode creative {NAME}")
    negative = denied(server, client, pos, expected_name=kind + " box")
    correct = set_hand(server, client, key_item())
    positive = opened(client, pos, {"text": kind + " box"})
    close_menu(client)
    server.command(f"gamemode survival {NAME}")
    return {"denied_creative_empty_hand": negative, "correct_key": correct, "positive": positive}


def spectator_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    server.command(f"gamemode spectator {NAME}")
    positive = opened(client, pos, {"text": kind + " box"})
    close_menu(client)
    server.command(f"gamemode survival {NAME}")
    return {"empty_hand_spectator_materialized_lock_bypass": positive}


def loot_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    # Vanilla built-in empty table: unpacking is observable without random loot.
    loot = '{LootTable:"minecraft:empty",LootTableSeed:73L}'
    server.command(f"data merge block {pos_text(pos)} {loot}")
    pending_chunk = private_chunk(server, client, kind)
    negative = denied(server, client, pos, expected_name=kind + " box")
    retained = typed_predicate(server, f"if data block {pos_text(pos)} {loot}")
    server.command(f"gamemode spectator {NAME}")
    spectator = denied(server, client, pos, pending_spectator=True)
    spectator_retained = typed_predicate(server, f"if data block {pos_text(pos)} {loot}")
    server.command(f"gamemode survival {NAME}")
    correct = set_hand(server, client, key_item())
    positive = opened(client, pos, {"text": kind + " box"}, expected=[None] * 27)
    unpacked = typed_predicate(server, f"unless data block {pos_text(pos)} LootTable unless data block {pos_text(pos)} LootTableSeed")
    close_menu(client)
    return {"wrong_key": negative, "retained_loot_and_long_seed": retained, "spectator_pending": spectator,
            "spectator_retained_loot_and_long_seed": spectator_retained, "correct_key": correct,
            "unpacked_menu": positive, "unpacked_fields": unpacked, "pending_private_chunk": pending_chunk}


def metadata_predicate(server: LoggedServer, kind: str) -> dict:
    pos = POSITIONS[kind]
    root_style = {key: value for key, value in title_for(kind, True).items() if key != "extra"}
    return typed_predicate(server, f"if data block {pos_text(pos)} " + snbt({
        "CustomName": root_style, "lock": {"components": {"minecraft:custom_name": KEY}},
        "components": {"minecraft:custom_data": {"keep": 7, "nested": {"left": 1, "right": 2}}},
        "Items": [{"Slot": 2, "id": "minecraft:gold_ingot", "count": 5}]}).replace('"Slot":2', '"Slot":2b'))


def deposit_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = fresh(server, client, kind)
    set_hand(server, client, key_item())
    server.command(f"data merge block {pos_text(pos)} " + snbt({"CustomName": title_for(kind),
        "components": {"minecraft:custom_data": {"keep": 7, "nested": {"left": 1}}}}))
    server.command(f"data merge block {pos_text(pos)} " + snbt({"CustomName": {"text": "Edited " + kind},
        "components": {"minecraft:custom_data": {"nested": {"right": 2}}}}))
    server.command(f"item replace entity {NAME} hotbar.1 with minecraft:gold_ingot 5")
    initial = opened(client, pos, title_for(kind, True), expected=[None] * 27)
    require(initial["menu"]["slots"][55] == GOLD, initial)
    sent = client.request("swap", slot=2, hotbar=1)
    # Matching client predictions can suppress slot updates. A fresh menu must
    # independently report the committed deposit and the emptied player slot.
    expected = [None] * 27
    expected[2] = GOLD
    reopened = opened(client, pos, title_for(kind, True), expected=expected)
    require(reopened["menu"]["slots"][55] is None, reopened)
    close_menu(client)
    preserved = metadata_predicate(server, kind)
    edited_chunk = private_chunk(server, client, kind)
    return {"initial": initial, "actual_swap": sent, "fresh_menu_after_swap": reopened,
            "preserved": preserved, "private_chunk_after_deposit": edited_chunk,
            "scope": "Opaque custom_data survives recursive merge and a real plain-stack deposit; no component gameplay claim."}


def restart_case(server: LoggedServer, client: Client, kind: str) -> dict:
    pos = POSITIONS[kind]
    teleport(server, client, pos)
    server.command(f"clear {NAME}")
    wrong_key = set_hand(server, client, 'stick[minecraft:custom_name="restart wrong key"]')
    negative = denied(server, client, pos)
    set_hand(server, client, key_item())
    expected = [None] * 27
    expected[2] = GOLD
    positive = opened(client, pos, title_for(kind, True), expected=expected)
    close_menu(client)
    restarted_chunk = private_chunk(server, client, kind)
    return {"wrong_mainhand": wrong_key, "after_clean_restart_wrong_key": negative, "correct_key_menu": positive,
            "semantic_metadata_readback": metadata_predicate(server, kind), "private_chunk_after_restart": restarted_chunk}


def double_case(server: LoggedServer, client: Client, kind: str) -> dict:
    # ChestBlock.getBlockType: RIGHT -> FIRST. getConnectedDirection: RIGHT
    # counterclockwise, LEFT clockwise. Facing north therefore FIRST is east.
    first = (4102, 200, 7 if kind == "chest" else 11)
    second = (4101, 200, first[2])
    close_menu(client)
    server.command(f"clear {NAME}")
    teleport(server, client, second)
    for pos in (first, second):
        server.command(f"setblock {pos_text(pos)} air")
    server.command(f"setblock {pos_text(second)} {kind}[facing=north,type=single]")
    server.command(f"setblock {pos_text(first)} {kind}[facing=north,type=right]")
    orientation = typed_predicate(server, f"if block {pos_text(first)} {kind}[facing=north,type=right] if block {pos_text(second)} {kind}[facing=north,type=left]")
    for pos, title in ((first, "Right FIRST"), (second, "Left SECOND")):
        server.command(f'data merge block {pos_text(pos)} {{CustomName:"{title}",lock:{LOCK}}}')
    negatives = []
    # A correct key for one half must not bypass the OTHER half, from either side.
    set_hand(server, client, key_item())
    for locked, other in ((first, second), (second, first)):
        server.command(f'data modify block {pos_text(locked)} lock.components."minecraft:custom_name" set value "wrong"')
        for clicked in (first, second):
            teleport(server, client, clicked)
            negatives.append({"mismatched_half": locked, "clicked": clicked, "denied": denied(server, client, clicked)})
        server.command(f'data modify block {pos_text(locked)} lock.components."minecraft:custom_name" set value "{KEY}"')
    positives = []
    for clicked in (first, second):
        teleport(server, client, clicked)
        positives.append(opened(client, clicked, {"text": "Right FIRST"}, size=54))
        close_menu(client)
    server.command(f"data remove block {pos_text(first)} CustomName")
    fallback = []
    for clicked in (first, second):
        teleport(server, client, clicked)
        fallback.append(opened(client, clicked, {"text": "Left SECOND"}, size=54))
        close_menu(client)
    return {"primary_orientation": orientation, "both_halves_checked_from_both_sides": negatives,
            "right_first_title_from_both_sides": positives, "left_fallback_from_both_sides": fallback}


class Fixture:
    """Own exactly one server and its clients; forced cleanup always fails the run."""
    def __init__(self, kind: str, directory: Path, args: argparse.Namespace, report: dict):
        self.server = LoggedServer(kind, directory, args)
        self.report = report
        self.kind = kind
        self.processes = report["processes"].setdefault(kind, [])
        self.clients: list[Client] = []
        self.server.owned_clients = self.clients
        report.setdefault("watchdogs", {})[kind] = self.server.abort_events
        self.closed: set[int] = set()

    def start(self, boot: str) -> None:
        entry = {"boot": boot, "started": now(), "configuration": {path.name: {"sha256": digest(path), "text": path.read_text()}
                 for path in self.server.directory.iterdir() if path.name in ("eula.txt", "server.properties", "pumpkin.toml", "ops.json")}}
        self.processes.append(entry)
        self.server.start()
        entry.update(pid=self.server.process.pid, argv=self.server.process.args)

    def connect(self, label: str, *, name: str = NAME, metadata: bool = True) -> Client:
        client = Client(self.server, log_name=label, name=name, container_metadata=metadata, raw_packets=True,
                        strict_offline_chat=not metadata)
        self.clients.append(client)
        client.ready()
        return client

    def close(self, client: Client) -> None:
        if client.process.pid in self.closed:
            return
        client.close()
        self.closed.add(client.process.pid)
        entry = {"pid": client.process.pid, "name": client.name, "exit_code": client.process.returncode,
                 "forced_close": client.forced_close, "expected_disconnect": client.expected_disconnect,
                 "failure": client.failure}
        self.processes[-1].setdefault("clients", []).append(entry)
        require(not client.failure and client.process.returncode == 0 and not client.forced_close, entry)

    def stop(self) -> None:
        server = self.server
        if server.process is None or "exit_code" in self.processes[-1]:
            return
        if server.broken:
            try:
                code = server.process.wait(timeout=6)
            except subprocess.TimeoutExpired:
                server.signal(signal.SIGKILL)
                code = server.process.wait(timeout=2)
            if server.reader:
                server.reader.join(timeout=2)
        else:
            if server.process.poll() is not None:
                self.processes[-1].update(exit_code=server.process.returncode, forced_stop=server.forced_stop,
                                          unexpected_exit=True, stopped=now())
                raise RuntimeError(f"Server exited before requested clean stop: {server.process.returncode}")
            code = server.stop()
        self.processes[-1].update(exit_code=code, forced_stop=server.forced_stop, stopped=now())
        suite.check_exit(code, server.forced_stop)
        errors = [line for line in server.log_path.read_text().splitlines() if WORLD_ERRORS.search(line)]
        require(not errors, {"server_errors": errors[:10]})

    def cleanup(self) -> list[str]:
        errors = []
        for client in self.clients:
            try:
                self.close(client)
            except Exception as error:
                errors.append(f"Client cleanup: {type(error).__name__}: {error}")
        try:
            self.stop()
        except Exception as error:
            errors.append(f"Server cleanup: {type(error).__name__}: {error}")
        return errors


def run_side(kind: str, directory: Path, args: argparse.Namespace, report: dict) -> list[dict]:
    fixture = Fixture(kind, directory, args, report)
    server = fixture.server
    results: dict[str, dict] = {}
    errors = []

    def record(identifier: str, action: Callable[[], dict]) -> None:
        def guarded() -> dict:
            with interaction_deadline(server, identifier):
                return action()
        results[identifier] = suite.recorded(identifier, (CASES[identifier],), guarded)
        require(not server.broken, server.broken)
        require(not any(client.failure for client in fixture.clients), "Unexpected client protocol failure")

    try:
        server.configure()
        for boot in ("initial", "restart"):
            fixture.start(boot)
            server.command("tick freeze")
            require_frozen(server)
            server.command("forceload add 4096 0")
            suite.wait_loaded(server, x=4096, z=0)
            if boot == "initial":
                server.command("fill 4096 199 0 4111 199 15 stone")
                server.command("difficulty peaceful")
                server.command("scoreboard objectives add metafence trigger")
            client = fixture.connect("metadata-" + boot)
            server.command(f"gamemode survival {NAME}")
            if boot == "initial":
                actions = (("title_recursive", title_case), ("mainhand", mainhand_case), ("offhand", offhand_case),
                           ("creative", creative_case), ("spectator", spectator_case), ("pending_loot", loot_case), ("deposit", deposit_case))
                for container in POSITIONS:
                    for suffix, action in actions:
                        record(f"container_metadata.{container}_{suffix}", lambda action=action, container=container: action(server, client, container))
                for container in ("chest", "trapped_chest"):
                    record(f"container_metadata.{container}_double", lambda container=container: double_case(server, client, container))
            else:
                for container in POSITIONS:
                    def restarted(container=container) -> dict:
                        require(results[f"container_metadata.{container}_deposit"]["result"] == "passed", "Deposit prerequisite failed; restart cannot pass")
                        return restart_case(server, client, container)
                    record(f"container_metadata.{container}_restart", restarted)
            fixture.close(client)
            server.save_flush()
            require_frozen(server)
            fixture.stop()
            require(suite.data_version(directory / "world") == 5023, "Saved world DataVersion differs from 5023")
    except Exception as error:
        errors.append(f"{type(error).__name__}: {error}")
    finally:
        errors.extend(fixture.cleanup())
    report.setdefault("runtime_errors", {})[kind] = errors
    return finish_rows(CASES, results, errors)


def finish_rows(cases: dict[str, str], results: dict[str, dict], errors: list[str]) -> list[dict]:
    for identifier, feature in cases.items():
        if identifier not in results:
            results[identifier] = {"id": identifier, "features": [feature], "result": "failed", "error": "Case was not reached"}
        if errors:
            results[identifier].update(result="failed", runtime_errors=errors)
    return [results[identifier] for identifier in cases]


def paired_main(label: str, cases: dict[str, str], runner: Callable, scope: str, deferred: list[dict]) -> int:
    parser = argparse.ArgumentParser(description=scope)
    parser.add_argument("--accept-eula", action="store_true")
    parser.add_argument("--java", required=True)
    parser.add_argument("--reference-cache", type=Path, required=True, help="Existing pinned 26.3 cache; never downloaded")
    parser.add_argument("--rust-binary", type=Path, required=True)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, default=ROOT / f".cache/{label}.json")
    args = parser.parse_args()
    if not args.accept_eula:
        parser.error("Explicit --accept-eula is required for https://aka.ms/MinecraftEULA")
    if args.workers < 1 or args.startup_timeout <= 0:
        parser.error("Workers and startup timeout must be positive")
    if not args.rust_binary.is_file() or not os.access(args.rust_binary, os.X_OK):
        parser.error("--rust-binary must be an existing executable")
    args.output = args.output.resolve()
    if not args.output.is_relative_to((ROOT / ".cache").resolve()):
        parser.error("Reports must remain under this repository's ignored .cache")
    runs = ROOT / ".cache/runs"
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix=label + "-", dir=runs))
    report = {"schema_version": 1, "kind": label, "created_at": now(), "target_version": TARGET["version"],
              "protocol": TARGET["protocol"], "data_version": 5023, "fixture": str(run), "result": "failed",
              "servers": {}, "processes": {}, "failed": [], "deferred": deferred, "scope": scope, "primary_contracts": PRIMARY,
              "environment": {"host": socket.gethostname(), "platform": sys.platform, "auth": "offline isolated 127.0.0.1", "workers": args.workers, "seed": "12345678"}}
    began = time.monotonic()
    try:
        report["source_state"] = suite.git_state()
        require(report["source_state"]["dirty"] is False, "Commit harness changes before running paired acceptance")
        report["harness_manifest"] = manifest()
        report["test_suite_sha256"] = hashlib.sha256(json.dumps(report["harness_manifest"], sort_keys=True).encode()).hexdigest()
        oracle.CACHE = args.reference_cache.resolve()
        require((oracle.CACHE / "server.jar").is_file() and (oracle.CACHE / "minecraft.jar").is_file(), "Official reference must already be cached; downloads are forbidden")
        official = oracle.acquire()
        report["official"] = {"path": str(official), "sha1": digest(official, "sha1"), "sha256": digest(official),
                              "inner_sha256": digest(oracle.CACHE / "minecraft.jar")}
        report["server_sha1"] = report["official"]["sha1"]
        report["server_sha256"] = report["official"]["sha256"]
        require(report["official"]["inner_sha256"] == PRIMARY["inner_jar_sha256"], "Cached inner jar differs from the bytecode used to derive these fixtures")
        original = args.rust_binary.resolve()
        binary_digest = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        report["rust_binary"] = {"source": str(original), "pinned": str(args.rust_binary), "sha256": digest(args.rust_binary)}
        require(binary_digest == digest(args.rust_binary) == digest(original), "Rust binary changed while pinning")
        report["rust_binary_sha256"] = binary_digest
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        version = subprocess.run([args.java, "-version"], capture_output=True, text=True, timeout=15, check=True)
        report["java"] = {"path": args.java, "sha256": digest(Path(args.java)), "version": version.stdout + version.stderr}
        for kind in ("vanilla", "rust"):
            print(f"Running {label}: {kind} on {socket.gethostname()} loopback; {run / kind}", flush=True)
            report["servers"][kind] = runner(kind, run / kind, args, report)
        require(manifest() == report["harness_manifest"], "Harness changed during run")
        require(suite.git_state() == report["source_state"], "Source changed during run")
        require(digest(original) == binary_digest == digest(args.rust_binary), "Rust binary changed during run")
        require(digest(official) == report["official"]["sha256"] and digest(oracle.CACHE / "minecraft.jar") == report["official"]["inner_sha256"], "Official jars changed during run")
        report["failed"].extend(f"{kind}:{case['id']}" for kind, rows in report["servers"].items() for case in rows if case["result"] != "passed")
        report["result"] = "failed" if report["failed"] else "passed"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        report["failed"].append("harness.global")
        for kind in ("vanilla", "rust"):
            rows = {row["id"]: row for row in report["servers"].get(kind, [])}
            report["servers"][kind] = finish_rows(cases, rows, [report["error"]])
    report.update(finished=now(), duration_seconds=round(time.monotonic() - began, 3))
    raw = json.dumps(report, indent=2) + "\n"
    (run / "report.json").write_text(raw)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(raw)
    print(f"{label} {report['result']}: {args.output}; retained artifacts: {run}", flush=True)
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(paired_main("container-metadata", CASES, run_side, __doc__, DEFERRED))
