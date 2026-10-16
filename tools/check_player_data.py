#!/usr/bin/env python3
"""Bounded, actual same-identity 26.3 player interchange in both directions.

Exactly four sequential server boots: fresh Java -> Rust and fresh Rust -> Java.
Seven composite scenario IDs per direction cover received inventory/position/health,
typed live identity/XP, advancements, and actual requested statistics continuation.
Whole worlds are copied unchanged after clean disconnect/save/stop. No UUID
rewrites, external conversion, synthetic save mutation or player-directory rename.
Only open_barrel and leave_game statistics are asserted; time counters, opaque-field
gameplay and a third-boot continuation are not verified.
Requires --accept-eula, the cached official reference, pinned JDK25 and an existing
Rust binary on clean committed source. This module does nothing when imported.
"""
from __future__ import annotations

import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import sys
from typing import Callable, NamedTuple
import uuid
import zipfile

sys.dont_write_bytecode = True
import oracle
from check_block_data import LoggedServer, interaction_deadline
from check_container_metadata import Fixture, finish_rows, now, paired_main, typed_predicate, wait_predicate
from parity import suite, worlds
from parity.bot import Client, NAME
from parity.furnaces import TypedReader, digest, require_frozen, step_exact
from parity.protocol import require

CASES = {
    "player_data.same_identity": "persistence.player_data",
    "player_data.inventory_components": "network.inventory_stream",
    "player_data.vitals_xp": "players.progression",
    "player_data.advancement_import": "players.advancements",
    "player_data.saved_continuation": "persistence.player_data",
    "player_data.advancement_continuation": "players.advancements",
    "player_data.statistics_continuation": "players.statistics",
}
DEFERRED = [
    {"id": "player_data.statistics_remaining", "result": "unverified", "reason":
     "Only same-UUID DataVersion 5023 open_barrel 3->5 and leave_game 1->2 are covered by this composite. Legacy private Statistics refusal, DFU, unknown named/numeric IDs, time/freeze semantics, contention and full counter gameplay remain unverified; no verified-family promotion."},
    {"id": "player_data.remaining_scope", "result": "unverified", "reason":
     "No legacy UUID migration, DFU, failed-load fixtures, unknown-ID sentinels, equipment/offhand/ender inventory, nondefault selected slot/dimension/respawn, gameplay for recipes/attributes/Brain/post-effects, XP wire decoding, signed/online identity or network-security claim. No third server boot to reload the destination's final mutation."},
]
PLAYER_ID = uuid.UUID("e7ee076d-be5b-31df-a414-3e907115f4a3")
UUID_INTS = struct.unpack(">4i", PLAYER_ID.bytes)
STORY = "minecraft:story/root"
ADVENTURE = "minecraft:adventure/adventuring_time"
PLAIN = "minecraft:plains"
UNTOUCHED_CRITERION = "minecraft:snowy_taiga"
POSITION = {"x": 4097.5, "y": 200.0, "z": 3.5, "yaw": 90.0, "pitch": 15.0}
CONTINUED_TELEPORT = {**POSITION, "x": 4099.5, "yaw": 180.0, "pitch": -10.0}
# Keep the +180 command boundary; Java 26.3 sends and saves its canonical -180 yaw.
CONTINUED_POSITION = {**CONTINUED_TELEPORT, "yaw": -180.0}
BARREL = {"x": 4098, "y": 200, "z": 3}
BARREL_POS = "4098 200 3"
REGISTRIES_SHA256 = "51f62d56f8bd0e9134ee88dd9226b64e9e56cf6a32377540d44291bf47ecdac6"
PACKETS_SHA256 = "2f0486311f18b7d3ec232028f3bff82c657d2f0240d2d65a3df40408db926886"
ITEM_NAME = "Interchange item"
CUSTOM_NAME = "Interchange custom"
OPAQUE = ("recipeBook", "attributes", "Brain", "post_effects")
MAX_WORLD_FILES = 8192
MAX_WORLD_BYTES = 1024 * 1024 * 1024
JDK_SHA256 = "7380ce48ed5013735d2c8414db54adb8f981e7933ff594bd36f3baccddaafba3"
RESOURCE_HASHES = {
    STORY: "edc34899c2f765403251c355f126dbb82c27887c45cfd39e8478c48f5fae5d53",
    ADVENTURE: "f1b0ed571ee97ceda83719d0a0926c9399da4fd3b6f2a340e48dd8da4e0d611c",
}


class ExperienceReference(NamedTuple):
    """Pin progress bits and query floor independently of the awarded total."""
    level: int
    total: int
    progress_bits: int
    query_points: int

    @property
    def progress(self) -> float:
        return struct.unpack(">f", struct.pack(">I", self.progress_bits))[0]


# Exact 26.3 f32 operation order, not an idealized within-level points / cost.
SOURCE_XP = ExperienceReference(level=12, total=223, progress_bits=0x3E6739C4, query_points=6)
CONTINUED_XP = ExperienceReference(level=12, total=227, progress_bits=0x3EB5AD66, query_points=10)
XP_PRIMARY = {
    "inner_jar_sha256": "a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6",
    "methods": ["Player.giveExperiencePoints", "ServerPlayer.giveExperiencePoints",
                "ExperienceCommand$Type.lambda$static$1", "Mth.floor(float)"],
    "bytecode_dumps": {
        "/tmp/ferraloom-experience-2ddab74-primary.javap": "bf8551a7a37170c48405b8ab0b8f79aa034e9f92002112a01b39efd744e49869",
        "/tmp/ferraloom-experience-2ddab74-support.javap": "7d6a810da51a6cc9b8adc2434b93c6bcc3aca7fef7d3c1cf640d7c784c528c7a"},
    "initial_save_corroboration": {
        "path": "/home/doc/code/Ferraloom/.cache/runs/player-data-bnvpygj5/vanilla/source/world/players/data/e7ee076d-be5b-31df-a414-3e907115f4a3.dat",
        "sha256": "29642fc9a1d29e745981adde0c4c12693d042655c01cc333f73e455019a38318",
        "commands_sha256": "5c0bb8497ff929df812cb1f0804e00231cb24cd97cab3a7458c52089f2122d9d",
        "report_sha256": "1ecd98c660d38b1571241122b1aab022bfccfc42c14a61a7b676d8dd7ae0058e"},
    "scope": "Both query counts and +4 continuation are bytecode-derived expectations, not prior runtime observations. Only initial XpP is corroborated by the stopped Java source; its first run failed before the post-award query or transfer.",
}


def write_json(path: Path, value: object) -> str:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")
    return str(path)


def read_bytes(path: Path, limit: int = worlds.MAX_NBT_BYTES) -> bytes:
    with path.open("rb") as source:
        raw = source.read(limit + 1)
    require(len(raw) <= limit, f"Oversized fixture file: {path}")
    return raw


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


class PlayerReader(TypedReader):
    """Extend the shared bounded reader to retain array and empty-list types."""
    def value(self, tag: int, depth: int = 0) -> object:
        element = self.data[self.offset] if tag == 9 and self.offset < len(self.data) else None
        value = super().value(tag, depth)
        if tag == 9:
            return {"nbt_type": "List", "element_type": element, "value": value}
        if tag in (7, 11, 12):
            return {"nbt_type": {7: "ByteArray", 11: "IntArray", 12: "LongArray"}[tag], "value": list(value)}
        return value


def read_player(world: Path) -> dict:
    path = world / "players/data" / f"{PLAYER_ID}.dat"
    raw = worlds.decompress_nbt(read_bytes(path), 1)
    reader = PlayerReader(raw)
    require(reader.number("B") == 10, "Player NBT root must be a compound")
    reader.string()
    typed = reader.value(10)
    require(reader.offset == len(raw) and isinstance(typed, dict), "Invalid player NBT root or trailing data")
    return {"path": str(path), "sha256": digest(path), "raw_nbt_sha256": hashlib.sha256(raw).hexdigest(), "typed": typed}


def timestamp(value: object) -> datetime:
    require(isinstance(value, str) and re.fullmatch(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [+-]\d{4}", value),
            {"noncanonical_criterion_timestamp": value})
    return datetime.strptime(value, "%Y-%m-%d %H:%M:%S %z")


def read_advancements(world: Path, definitions: dict) -> dict:
    path = world / "players/advancements" / f"{PLAYER_ID}.json"
    data = json.loads(read_bytes(path), object_pairs_hook=unique_object)
    require(isinstance(data, dict) and type(data.get("DataVersion")) is int and data["DataVersion"] == 5023,
            "Advancement file must independently declare DataVersion 5023")
    for identifier, progress in data.items():
        if identifier == "DataVersion":
            continue
        require(worlds.IDENTIFIER.fullmatch(identifier) and isinstance(progress, dict), identifier)
        require(type(progress.get("done")) is bool and isinstance(progress.get("criteria", {}), dict), progress)
        for criterion, achieved in progress.get("criteria", {}).items():
            require(isinstance(criterion, str) and bool(criterion), progress)
            timestamp(achieved)
    for identifier, done in ((STORY, True), (ADVENTURE, False)):
        progress = data.get(identifier, {})
        criteria = progress.get("criteria", {})
        definition = definitions[identifier]
        require(bool(criteria) and set(criteria) <= set(definition["criteria"]), progress)
        computed = all(any(criterion in criteria for criterion in group) for group in definition["requirements"])
        require(progress.get("done") is done and computed is done, {"id": identifier, "progress": progress})
    require("crafting_table" in data[STORY]["criteria"]
            and {PLAIN, UNTOUCHED_CRITERION} <= data[ADVENTURE]["criteria"].keys(), data)
    return {"path": str(path), "sha256": digest(path), "data": data}


def read_statistics(world: Path) -> dict:
    """Independent canonical JSON readback, never player NBT or a wire prediction."""
    path = world / "players/stats" / f"{PLAYER_ID}.json"
    raw = read_bytes(path)
    data = json.loads(raw, object_pairs_hook=unique_object)
    require(isinstance(data, dict) and type(data.get("DataVersion")) is int and data["DataVersion"] == 5023,
            "Statistics file must independently declare DataVersion 5023")
    require(isinstance(data.get("stats"), dict), "Missing named statistics object")
    count = 0
    for category, entries in data["stats"].items():
        require(worlds.IDENTIFIER.fullmatch(category) and isinstance(entries, dict), {"invalid_stat_category": category})
        for identifier, value in entries.items():
            count += 1
            require(count <= 65536 and worlds.IDENTIFIER.fullmatch(identifier)
                    and type(value) is int and 0 <= value <= 2147483647, {"invalid_stat": [category, identifier, value]})
    return {"path": str(path), "sha256": hashlib.sha256(raw).hexdigest(), "data": data}


def statistics_counts(saved: dict, opens: int, leaves: int) -> dict:
    custom = saved["data"]["stats"].get("minecraft:custom", {})
    expected = {"minecraft:open_barrel": opens, "minecraft:leave_game": leaves}
    for identifier, value in expected.items():
        require(type(custom.get(identifier)) is int and custom[identifier] == value,
                {"file": saved["path"], "statistic": identifier, "expected": value, "actual": custom.get(identifier)})
    return {"path": saved["path"], "sha256": saved["sha256"], "counts": {key: custom[key] for key in expected},
            "data_version": saved["data"]["DataVersion"]}


def statistics_contract() -> dict:
    reports = {}
    for name, expected in (("registries", REGISTRIES_SHA256), ("packets", PACKETS_SHA256)):
        path = oracle.CACHE / f"generated/reports/{name}.json"
        raw = read_bytes(path)
        require(hashlib.sha256(raw).hexdigest() == expected, f"Changed primary statistics report: {path}")
        reports[name] = {"path": str(path), "sha256": expected, "data": json.loads(raw, object_pairs_hook=unique_object)}
    registries, packets = reports["registries"].pop("data"), reports["packets"].pop("data")
    types = registries["minecraft:stat_type"]["entries"]
    custom = registries["minecraft:custom_stat"]["entries"]
    require(len(types) == 9 and len(custom) == 78 and types["minecraft:custom"]["protocol_id"] == 8
            and custom["minecraft:open_barrel"]["protocol_id"] == 63
            and custom["minecraft:leave_game"]["protocol_id"] == 0, "Changed primary statistics IDs")
    require(packets["play"]["clientbound"]["minecraft:award_stats"]["protocol_id"] == 3
            and packets["play"]["serverbound"]["minecraft:client_command"]["protocol_id"] == 12, "Changed statistics packet IDs")
    return {"reports": reports, "stat_types": types, "custom_stats": custom,
            "award_stats_packet_id": 3, "request_packet_id": 12, "request_action_id": 1,
            "layout_witness": "Pinned inner JAR: ClientboundAwardStatsPacket map(Stat.STREAM_CODEC, VAR_INT); Stat dispatches STAT_TYPE then StatType value registry. ServerboundClientCommandPacket.Action.REQUEST_STATS ordinal 1; corroborated by local award_stats.rs/client_command.rs.",
            "scope": "Names only from these reports; other wellformed numeric entries remain unnamed observations."}


def world_hashes(world: Path) -> dict:
    """Bound the fresh fixture and refuse links before copying any world files."""
    require(world.is_dir() and not world.is_symlink(), f"Missing/linked world: {world}")
    files, directories, total = {}, [], 0
    for path in sorted(world.rglob("*")):
        relative = path.relative_to(world).as_posix()
        require(not path.is_symlink(), f"World symlink refused: {path}")
        if path.is_dir():
            directories.append(relative)
        else:
            require(path.is_file(), f"Non-regular world entry: {path}")
            total += path.stat().st_size
            require(total <= MAX_WORLD_BYTES and len(files) < MAX_WORLD_FILES, "Fresh world exceeds copy bounds")
            files[relative] = digest(path)
        require(len(files) + len(directories) <= MAX_WORLD_FILES * 2, "Fresh world exceeds entry bound")
    return {"files": files, "directories": directories, "bytes": total,
            "players": {path: value for path, value in files.items() if path.startswith("players/")}}


def clone_world(source: Path, destination: Path, evidence: dict) -> None:
    """Caller has closed all clients and stopped the source; keep it immutable."""
    require(not destination.exists(), f"Copy destination already exists: {destination}")
    evidence["source_before_copy"] = world_hashes(source)
    evidence["destination_before_copy"] = {"exists": False}
    shutil.copytree(source, destination)
    evidence["source_after_copy"] = world_hashes(source)
    evidence["destination_after_copy"] = world_hashes(destination)
    require(evidence["source_before_copy"] == evidence["source_after_copy"], "Source world changed during copy")
    require(evidence["source_before_copy"] == evidence["destination_after_copy"], "Whole-world copy changed source bytes/paths")


def primary_inputs(report: dict) -> dict:
    require(NAME == "FerraloomTest", "Exact-case fixture name changed")
    md5 = hashlib.md5(("OfflinePlayer:" + NAME).encode("utf-8"), usedforsecurity=False).digest()
    require(uuid.UUID(bytes=md5, version=3) == PLAYER_ID, "Offline UUID does not match the official witness")
    require(report["official"]["inner_sha256"] == XP_PRIMARY["inner_jar_sha256"], "Changed primary experience bytecode")
    require(re.search(r'version "25\.', report["java"]["version"]) is not None
            and report["java"]["sha256"] == JDK_SHA256 == digest(Path(report["java"]["path"])),
            "Requires the retained pinned Temurin 25.0.4.1+1 reference JDK")
    definitions, resources = {}, {}
    with zipfile.ZipFile(oracle.CACHE / "minecraft.jar") as jar:
        for identifier, expected_hash in RESOURCE_HASHES.items():
            path = "data/minecraft/advancement/" + identifier.removeprefix("minecraft:") + ".json"
            require(jar.getinfo(path).file_size <= 65536, "Oversized advancement resource")
            raw = jar.read(path)
            require(hashlib.sha256(raw).hexdigest() == expected_hash, f"Changed official resource: {path}")
            definitions[identifier] = json.loads(raw, object_pairs_hook=unique_object)
            resources[path] = {"sha256": expected_hash, "definition": definitions[identifier]}
    report["primary_contracts"] = {"inner_jar_sha256": report["official"]["inner_sha256"], "resources": resources,
        "identity": {"name": NAME, "offline_uuid": str(PLAYER_ID), "uuid_int_array": UUID_INTS},
        "layout": "world/players/{data,advancements,stats}; actual retained official 26.3 layout",
        "item_wire_ids": {"minecraft:stick": 1060, "minecraft:gold_ingot": 1022},
        "statistics": statistics_contract(),
        "experience": {**XP_PRIMARY, "expectations": {
            label: {**xp._asdict(), "progress": xp.progress}
            for label, xp in (("after_add_223", SOURCE_XP), ("after_reload_then_add_4", CONTINUED_XP))}},
        "scope": "Exact literal item_name/custom_name through the shared client codec; no rendering normalization."}
    return definitions


def cached_reference() -> Path:
    """Read-only acquire adapter for both shared entry points, even on a race."""
    jar = oracle.CACHE / "server.jar"
    require(jar.is_file() and (oracle.CACHE / "minecraft.jar").is_file(), "Official reference must already be cached")
    require(jar.stat().st_size == oracle.TARGET["server_size"] and digest(jar, "sha1") == oracle.TARGET["server_sha1"],
            "Cached official reference differs from the pinned target")
    return jar


def expected_inventory(count: int) -> list:
    slots = [None] * 46
    slots[36] = {"item_id": 1060, "count": 1, "components": {"minecraft:item_name": ITEM_NAME}}
    slots[37] = {"item_id": 1022, "count": count}
    slots[38] = {"item_id": 1060, "count": 1, "components": {"minecraft:custom_name": CUSTOM_NAME}}
    return slots


def command_result(server: LoggedServer, command: str, expected: int, *, mode: str = "result") -> dict:
    """Reset the score so a skipped store cannot masquerade as a zero result."""
    server.command("scoreboard players set Observation pdprobe -2147483648")
    output = server.command(f"execute store {mode} score Observation pdprobe run {command}")
    witness = typed_predicate(server, f"if score Observation pdprobe matches {expected}")
    return {"command": command, "mode": mode, "expected": expected, "output": output, **witness}


def inventory_witness(server: LoggedServer, client: Client, count: int) -> dict:
    expected = expected_inventory(count)
    state = client.wait_state(lambda state: (state.get("player_inventory") or {}).get("slots") == expected)
    inventory = state["player_inventory"]
    contents = [packet for packet in state["menu_packets"] if packet["kind"] == "content" and packet["container_id"] == 0]
    require(bool(contents) and contents[-1]["carried"] is None, "Missing received player contents with empty cursor")
    predicate = (f'if data entity {NAME} {{Inventory:[{{Slot:0b,id:"minecraft:stick",count:1,components:{{"minecraft:item_name":"{ITEM_NAME}"}}}},'
                 f'{{Slot:1b,id:"minecraft:gold_ingot",count:{count}}},'
                 f'{{Slot:2b,id:"minecraft:stick",count:1,components:{{"minecraft:custom_name":"{CUSTOM_NAME}"}}}}]}}')
    return {"expected_slots": expected, "received_inventory": inventory, "received_contents": contents[-1],
            "typed_entity": typed_predicate(server, predicate)}


def identity_witness(server: LoggedServer, client: Client) -> dict:
    state = client.state()
    require(client.name == NAME and state.get("login") is True and state.get("phase") == "play"
            and state.get("protocol") == 777 and state.get("registry_packets", 0) > 0, state)
    predicate = f'if entity @a[name={NAME},nbt={{UUID:[I;{",".join(map(str, UUID_INTS))}]}}]'
    return {"name": NAME, "uuid": str(PLAYER_ID), "protocol": state["protocol"], "registry_packets": state["registry_packets"],
            "login": state["login"], "typed_live_uuid": typed_predicate(server, predicate),
            "scope": "Real same-name PLAY login plus authoritative live UUID; no additional LoginSuccess UUID decoder."}


def vitals_witness(server: LoggedServer, client: Client, xp: ExperienceReference, position: dict) -> dict:
    state = client.wait_state(lambda state: state.get("health") == 14 and state.get("food") == 20
                              and state.get("server_position") == position)
    predicate = (f'if data entity {NAME} {{Health:14.0f,foodLevel:20,XpLevel:{xp.level},XpP:{xp.progress}f,XpTotal:{xp.total},'
                 f'SelectedItemSlot:0,Dimension:"minecraft:overworld",playerGameType:0,'
                 f'Pos:[{position["x"]}d,{position["y"]}d,{position["z"]}d],Rotation:[{position["yaw"]}f,{position["pitch"]}f]}}')
    return {"received_health": state["health"], "received_food": state["food"], "received_position": state["server_position"],
            "typed_entity": typed_predicate(server, predicate),
            "expected_xp": xp._asdict(),
            "levels": command_result(server, f"experience query {NAME} levels", xp.level),
            "query_points": command_result(server, f"experience query {NAME} points", xp.query_points)}


def teleport_exact(server: LoggedServer, client: Client, position: dict, *,
                   expected_server_rotation: tuple[float, float] | None = None) -> dict:
    """Keep command angles independent of the exact received rotation expectation."""
    expected_position = (position if expected_server_rotation is None else
                         {**position, "yaw": expected_server_rotation[0], "pitch": expected_server_rotation[1]})
    before = client.state()["teleport_confirmations"]
    command = f"tp {NAME} " + " ".join(str(position[axis]) for axis in ("x", "y", "z", "yaw", "pitch"))
    output = server.command(command)
    state = client.wait_state(lambda state: state["teleport_confirmations"] > before
                              and state.get("server_position") == expected_position)
    return {"command": command, "expected_position": expected_position, "output": output,
            "received_position": state["server_position"], "confirmations": state["teleport_confirmations"]}


def barrel_ready(server: LoggedServer, client: Client) -> dict:
    state = client.wait_state(lambda state: any(chunk["x"] == 256 and chunk["z"] == 0 for chunk in state["chunk_packets"]))
    require(state.get("server_position") == POSITION and state.get("menu") is None, "Barrel requires observed safe reachable position and no menu")
    return {"received_lane_chunk": next(chunk for chunk in reversed(state["chunk_packets"]) if chunk["x"] == 256 and chunk["z"] == 0),
            "received_position": state["server_position"], "predicates": [
                typed_predicate(server, f"if block {BARREL_POS} minecraft:barrel[open=false]"),
                *[typed_predicate(server, f"unless data block {BARREL_POS} {field}") for field in ("lock", "LootTable", "CustomName")]]}


def barrel_open_close(server: LoggedServer, client: Client) -> dict:
    before = client.state()
    require(before.get("menu") is None, "Each barrel interaction requires a real preceding close")
    sent = client.request("use_item_on", **BARREL)
    require(isinstance(sent, dict) and sent.get("written") is True, "Barrel use was not queued")

    def opened(state: dict) -> bool:
        menu = state.get("menu")
        return (isinstance(menu, dict) and menu["opened_sequence"] > before["packets"] and menu["container_id"] > 0
                and menu["menu_type"] == 2 and len(menu["slots"]) == 63 and menu["slots"][:27] == [None] * 27
                and menu["carried"] is None)

    state = client.wait_state(opened)
    packets = [packet for packet in state["menu_packets"] if packet["sequence"] > before["packets"]
               and packet["container_id"] == state["menu"]["container_id"]]
    screens = [packet for packet in packets if packet["kind"] == "open"]
    require(len(screens) == 1 and screens[0].get("title_component", {}).get("translate") == "container.barrel"
            and any(packet["kind"] == "content" and len(packet["slots"]) == 63 for packet in packets), packets)
    opened_block = wait_predicate(server, f"if block {BARREL_POS} minecraft:barrel[open=true]")
    closed = client.request("close_menu")
    require(isinstance(closed, dict) and closed.get("written") is True, "Barrel close was not queued")
    # Local menu=null is only a prediction. The authoritative closed block is
    # the processing fence before another open, with no tick/time-counter claim.
    closed_block = wait_predicate(server, f"if block {BARREL_POS} minecraft:barrel[open=false]")
    return {"position": BARREL, "before_sequence": before["packets"], "use": sent, "received": packets,
            "opened_block": opened_block, "close": closed, "closed_block": closed_block}


def request_statistics(client: Client, contract: dict, opens: int) -> dict:
    sent = client.request("request_stats")
    require(isinstance(sent, dict) and sent.get("written") is True and sent.get("packet_id") == contract["request_packet_id"]
            and sent.get("action_id") == contract["request_action_id"] and type(sent.get("after_sequence")) is int,
            {"unqueued_statistics_request": sent})
    state = client.wait_state(lambda state: (state.get("statistics_packet") or {}).get("sequence", -1) > sent["after_sequence"])
    packet = state["statistics_packet"]
    category = contract["stat_types"]["minecraft:custom"]["protocol_id"]
    identifier = contract["custom_stats"]["minecraft:open_barrel"]["protocol_id"]
    matches = [entry for entry in packet["entries"] if entry["category_id"] == category and entry["statistic_id"] == identifier]
    require(packet["packet_id"] == contract["award_stats_packet_id"] and packet["decoded_bytes"] == packet["payload_bytes"]
            and len(matches) == 1 and matches[0]["value"] == opens, {"expected_open_barrel": opens, "received": packet})
    return {"request": sent, "received": packet, "named_observation": {"category": "minecraft:custom",
            "statistic": "minecraft:open_barrel", **matches[0]}, "registry_report_sha256": contract["reports"]["registries"]["sha256"],
            "scope": "Fresh received packet after queued request; protocol has no request correlation ID. No predicted totals."}


def validate_experience(data: dict, xp: ExperienceReference) -> dict:
    """Compare independently read typed NBT to the pinned reference, without rounding."""
    expected = {"XpLevel": {"nbt_type": "Int", "value": xp.level},
                "XpP": {"nbt_type": "Float", "value": xp.progress},
                "XpTotal": {"nbt_type": "Int", "value": xp.total}}
    for key, value in expected.items():
        require(data.get(key) == value, {"field": key, "expected": value, "actual": data.get(key)})
    return {"typed": {key: data[key] for key in expected},
            "progress_bits": "0x" + struct.pack(">f", data["XpP"]["value"]).hex()}


def validate_player(saved: dict, *, count: int, xp: ExperienceReference, position: dict) -> dict:
    """Exact typed selected state; never coerce component strings into objects."""
    data = saved["typed"]
    require(data.get("Statistics", {}) == {}, "Private nonempty/malformed Statistics NBT is not canonical statistics storage")
    experience = validate_experience(data, xp)
    expected = {"UUID": {"nbt_type": "IntArray", "value": list(UUID_INTS)}, "Dimension": "minecraft:overworld"}
    for key, kind, value in (("DataVersion", "Int", 5023), ("SelectedItemSlot", "Int", 0),
                             ("Health", "Float", 14.0), ("foodLevel", "Int", 20), ("playerGameType", "Int", 0),
                             ("fall_distance", "Double", 0.0)):
        expected[key] = {"nbt_type": kind, "value": value}
    for key, kind, tag, axes in (("Pos", "Double", 6, ("x", "y", "z")), ("Rotation", "Float", 5, ("yaw", "pitch"))):
        expected[key] = {"nbt_type": "List", "element_type": tag,
                         "value": [{"nbt_type": kind, "value": position[axis]} for axis in axes]}
    for key, value in expected.items():
        require(data.get(key) == value, {"field": key, "expected": value, "actual": data.get(key)})
    require("FallDistance" not in data, "Legacy FallDistance alias was emitted")
    wanted = []
    for slot, item, amount, components in ((0, "stick", 1, {"minecraft:item_name": ITEM_NAME}),
                                           (1, "gold_ingot", count, {}), (2, "stick", 1, {"minecraft:custom_name": CUSTOM_NAME})):
        entry = {"Slot": {"nbt_type": "Byte", "value": slot}, "id": "minecraft:" + item,
                 "count": {"nbt_type": "Int", "value": amount}}
        if components:
            entry["components"] = components
        wanted.append(entry)
    inventory = data.get("Inventory", {})
    require(inventory.get("nbt_type") == "List" and inventory.get("element_type") == 10, inventory)
    entries = inventory.get("value", [])
    # Inventory list order is not slot order. Empty component maps on plain
    # stacks are equivalent; the two named stacks require their exact maps.
    normalized = [{key: value for key, value in entry.items() if not (key == "components" and value == {})}
                  for entry in entries]
    require(sorted(normalized, key=lambda entry: entry["Slot"]["value"]) == wanted,
            {"expected_inventory": wanted, "actual_inventory": entries})
    require(data.get("XpSeed", {}).get("nbt_type") == "Int", "Missing typed enchantment seed")
    return {"player_dat": saved["path"], "sha256": saved["sha256"], "independent_player_data_version": 5023,
            "checked_fields": [*expected, "XpLevel", "XpP", "XpTotal", "Inventory", "XpSeed"],
            "experience": experience, "inventory": inventory}


def advancement_command(server: LoggedServer, action: str, identifier: str, criterion: str, expected: int) -> dict:
    return command_result(server, f"advancement {action} {NAME} only {identifier} {criterion}", expected, mode="success")


def advancement_import(server: LoggedServer) -> dict:
    return {"already_complete": advancement_command(server, "grant", STORY, "crafting_table", 0),
            "revoke_loaded_complete": advancement_command(server, "revoke", STORY, "crafting_table", 1),
            "restore_complete": advancement_command(server, "grant", STORY, "crafting_table", 1),
            "already_loaded_partial": advancement_command(server, "grant", ADVENTURE, PLAIN, 0),
            "revoke_loaded_partial": advancement_command(server, "revoke", ADVENTURE, PLAIN, 1),
            "restore_partial": advancement_command(server, "grant", ADVENTURE, PLAIN, 1)}


def compare_advancements(source: dict, destination: dict, added: str, *, destination_kind: str) -> dict:
    require(destination_kind in ("vanilla", "rust"), "Unknown advancement destination runtime")
    original, final = source["data"], destination["data"]
    preserved = []
    for identifier, progress in original.items():
        if identifier == "DataVersion":
            continue
        require(identifier in final, f"Lost advancement: {identifier}")
        if progress["done"]:
            require(final[identifier]["done"] is True, f"Lost completion: {identifier}")
        for criterion, achieved in progress.get("criteria", {}).items():
            actual = final[identifier].get("criteria", {}).get(criterion)
            if (identifier, criterion) in ((STORY, "crafting_table"), (ADVENTURE, PLAIN)):
                require(timestamp(actual) >= timestamp(achieved), "Re-granted criterion predates the source")
            else:
                require(timestamp(actual) == timestamp(achieved),
                        {"changed_criterion_instant": [identifier, criterion], "source": achieved, "final": actual})
                if destination_kind == "rust":
                    require(actual == achieved, {"rewritten_criterion_text": [identifier, criterion], "source": achieved, "final": actual})
                preserved.append([identifier, criterion, achieved])
    require(added not in original[ADVENTURE]["criteria"] and added in final[ADVENTURE]["criteria"], "New criterion not persisted")
    return {"source_file": source["path"], "destination_file": destination["path"], "preserved_criteria": preserved,
            "controlled_regrants": {STORY: final[STORY], ADVENTURE: {PLAIN: final[ADVENTURE]["criteria"][PLAIN]}},
            "controlled_addition": {added: final[ADVENTURE]["criteria"][added]},
            "partial_done": final[ADVENTURE]["done"], "data_version": final["DataVersion"],
            "timestamp_preservation": "instant_and_exact_text" if destination_kind == "rust" else "instant"}


def run_direction(kind: str, directory: Path, args: argparse.Namespace, report: dict) -> list[dict]:
    target = "rust" if kind == "vanilla" else "vanilla"
    results, errors = {}, []
    detail = {"source_kind": kind, "destination_kind": target, "direction": f"{kind}-to-{target}", "stages": {},
              "scope": "servers keys identify the SOURCE runtime, not two unrelated fresh player fixtures"}
    report.setdefault("transfers", {})[kind] = detail
    fixtures: list[Fixture] = []
    immutable: dict | None = None

    def stage(label: str, fixture: Fixture, action: Callable[[], object], *, guarded: bool = True):
        entry = {"started": now()}
        detail["stages"][label] = entry
        try:
            if guarded:
                with interaction_deadline(fixture.server, label):
                    value = action()
            else:
                value = action()
            entry.update(result="passed", detail=value)
            return value
        except Exception as error:
            entry.update(result="failed", error=f"{type(error).__name__}: {error}")
            raise
        finally:
            entry["finished"] = now()

    def record(suffix: str, fixture: Fixture, action: Callable[[], dict], *, live: bool = True) -> None:
        identifier = "player_data." + suffix
        results[identifier] = suite.recorded(identifier, (CASES[identifier],),
                                             lambda: stage(identifier, fixture, action, guarded=live))
        require(not fixture.server.broken and not any(client.failure for client in fixture.clients), "Server/client protocol failure")

    def boot(fixture: Fixture, role: str) -> None:
        try:
            stage(role + "_startup", fixture, lambda: fixture.start(role), guarded=False)
        finally:
            fixture.processes[-1].update(directory=str(fixture.server.directory), role=role, direction=detail["direction"])
            if fixture.server.process is not None:
                fixture.processes[-1].update(pid=fixture.server.process.pid, argv=fixture.server.process.args)
            detail.setdefault("watchdogs", {})[role] = fixture.server.abort_events
        stage(role + "_freeze", fixture, lambda: (fixture.server.command("tick freeze"), require_frozen(fixture.server)))

    def connect(fixture: Fixture, role: str) -> Client:
        # Startup/chunk readiness has its own bound, outside the 20s ordinary
        # interaction watchdog. Do not demand a new chunk after a nearby tp.
        client = stage(role + "_connect", fixture, lambda: fixture.connect(role + "-player"), guarded=False)
        detail["stages"][role + "_connect"]["detail"] = {"pid": client.process.pid, "name": client.name}
        stage(role + "_initial_chunks", fixture, lambda: {"chunks": client.wait_state(
            lambda state: state.get("chunks", 0) >= 49, timeout=60)["chunks"]}, guarded=False)
        return client

    def disconnect_and_stop(fixture: Fixture, client: Client, role: str) -> None:
        stage(role + "_disconnect", fixture, lambda: fixture.close(client))
        stage(role + "_no_player", fixture, lambda: wait_predicate(fixture.server, f"unless entity @a[name={NAME}]"))
        stage(role + "_save", fixture, fixture.server.save_flush, guarded=False)
        stage(role + "_saved_frozen", fixture, lambda: require_frozen(fixture.server))
        stage(role + "_stop", fixture, fixture.stop, guarded=False)
        require(fixture.server.process is not None and fixture.server.process.returncode == 0
                and all(client.process.returncode == 0 and not client.forced_close for client in fixture.clients),
                "All owned processes must exit successfully before copy/readback")

    try:
        definitions = primary_inputs(report)
        stats_contract = report["primary_contracts"]["statistics"]
        source = Fixture(kind, directory / "source", args, report)
        fixtures.append(source)
        source.server.configure()
        boot(source, "source")
        server = source.server
        stage("source_forceload", source, lambda: server.command("forceload add 4096 0"))
        stage("source_loaded_lane", source, lambda: suite.wait_loaded(server, x=4096, z=0), guarded=False)
        stage("source_lane", source, lambda: (server.command("fill 4096 199 0 4111 199 15 stone"),
              server.command("difficulty normal"), server.command("scoreboard objectives add pdprobe dummy"),
              command_result(server, "gamerule natural_health_regeneration false", 1, mode="success")))
        stage("source_barrel_place", source, lambda: command_result(server, f"setblock {BARREL_POS} minecraft:barrel", 1, mode="success"))
        client = connect(source, "source")
        stage("source_identity", source, lambda: identity_witness(server, client))
        stage("source_position", source, lambda: teleport_exact(server, client, POSITION))
        stage("source_initial_state", source, lambda: (server.command(f"gamemode survival {NAME}"),
              server.command(f"clear {NAME}"), command_result(server, "gamerule natural_health_regeneration", 0),
              command_result(server, f"experience query {NAME} levels", 0), command_result(server, f"experience query {NAME} points", 0)))
        stage("source_inventory_commands", source, lambda: [
            command_result(server, f'item replace entity {NAME} hotbar.0 with minecraft:stick[minecraft:item_name="{ITEM_NAME}"]', 1, mode="success"),
            command_result(server, f"item replace entity {NAME} hotbar.1 with minecraft:gold_ingot 37", 1, mode="success"),
            command_result(server, f'item replace entity {NAME} hotbar.2 with minecraft:stick[minecraft:custom_name="{CUSTOM_NAME}"]', 1, mode="success")])
        # Expire initial join immunity with exact game ticks on a safe platform;
        # regeneration is disabled and no wall-clock parity sleep is involved.
        stage("source_join_immunity_ticks", source, lambda: step_exact(server, 80))
        stage("source_damage_xp", source, lambda: [
            command_result(server, f"damage {NAME} 6 minecraft:generic", 1, mode="success"),
            command_result(server, f"experience add {NAME} 223 points", 1, mode="success"), step_exact(server, 1)])
        stage("source_inventory", source, lambda: inventory_witness(server, client, 37))
        stage("source_vitals", source, lambda: vitals_witness(server, client, SOURCE_XP, POSITION))
        stage("source_advancements", source, lambda: [
            advancement_command(server, "grant", STORY, "crafting_table", 1),
            server.command(f"advancement revoke {NAME} only {ADVENTURE}"),
            advancement_command(server, "grant", ADVENTURE, PLAIN, 1),
            advancement_command(server, "grant", ADVENTURE, UNTOUCHED_CRITERION, 1)])
        stage("source_barrel_ready", source, lambda: barrel_ready(server, client))
        for opening in range(1, 4):
            stage(f"source_barrel_open_{opening}", source, lambda: barrel_open_close(server, client))
        stage("source_statistics_live", source, lambda: request_statistics(client, stats_contract, 3))
        disconnect_and_stop(source, client, "source")
        source_world = server.directory / "world"
        immutable = world_hashes(source_world)
        detail["source_after_stop"] = immutable
        original_player = read_player(source_world)
        detail["source_player"] = write_json(directory / "source-player.json", original_player)
        detail["source_canonical_state"] = validate_player(original_player, count=37, xp=SOURCE_XP, position=POSITION)
        original_statistics = stage("source_statistics_json", source, lambda: read_statistics(source_world), guarded=False)
        detail["source_statistics"] = write_json(directory / "source-statistics.json", original_statistics)
        stage("source_statistics_counts", source, lambda: statistics_counts(original_statistics, 3, 1), guarded=False)
        original_advancements = read_advancements(source_world, definitions)
        detail["source_advancements"] = write_json(directory / "source-advancements.json", original_advancements)
        missing = sorted(set(definitions[ADVENTURE]["criteria"]) - set(original_advancements["data"][ADVENTURE]["criteria"]))
        require(bool(missing), "Source adventure fixture is not partial")
        added = missing[0]
        detail["new_destination_criterion"] = added
        if kind == "vanilla":
            require(all(field in original_player["typed"] for field in OPAQUE), "Official opaque-field prerequisite missing")

        destination = Fixture(target, directory / "destination", args, report)
        fixtures.append(destination)
        destination.server.configure()
        destination_world = destination.server.directory / "world"
        detail["discarded_fresh_destination_scaffold"] = world_hashes(destination_world)
        # Only the unstarted destination's harness-created datapack scaffold is
        # removed. Copy the entire stopped source world, including datapacks.
        shutil.rmtree(destination_world)
        copy_evidence = detail.setdefault("copy", {})
        clone_world(source_world, destination_world, copy_evidence)
        require(copy_evidence["source_before_copy"] == immutable, "Stopped source changed before copy")
        boot(destination, "destination")
        target_server = destination.server
        target_client = connect(destination, "destination")
        record("same_identity", destination, lambda: {"source": detail["stages"]["source_identity"]["detail"],
               "destination": identity_witness(target_server, target_client), "whole_world_copy": copy_evidence})
        record("inventory_components", destination, lambda: {"source": detail["stages"]["source_inventory"]["detail"],
               "destination": inventory_witness(target_server, target_client, 37)})
        record("vitals_xp", destination, lambda: {"source": detail["stages"]["source_vitals"]["detail"],
               "destination": vitals_witness(target_server, target_client, SOURCE_XP, POSITION)})
        record("advancement_import", destination, lambda: {"source_json": detail["source_advancements"],
               "runtime_commands": advancement_import(target_server)})
        require(all(row["result"] == "passed" for row in results.values()), "Destination import prerequisite failed; mutation cannot pass")
        stage("destination_statistics_import", destination, lambda: request_statistics(target_client, stats_contract, 3))
        stage("destination_barrel_ready", destination, lambda: barrel_ready(target_server, target_client))
        for opening in range(1, 3):
            stage(f"destination_barrel_open_{opening}", destination, lambda: barrel_open_close(target_server, target_client))
        stage("destination_statistics_live", destination, lambda: request_statistics(target_client, stats_contract, 5))
        stage("destination_mutation", destination, lambda: [
            command_result(target_server, f"item replace entity {NAME} hotbar.1 with minecraft:gold_ingot 41", 1, mode="success"),
            command_result(target_server, f"experience add {NAME} 4 points", 1, mode="success"),
            advancement_command(target_server, "grant", ADVENTURE, added, 1),
            advancement_command(target_server, "revoke", ADVENTURE, added, 1),
            advancement_command(target_server, "grant", ADVENTURE, added, 1)])
        stage("destination_position", destination, lambda: teleport_exact(target_server, target_client, CONTINUED_TELEPORT,
              expected_server_rotation=(CONTINUED_POSITION["yaw"], CONTINUED_POSITION["pitch"])))
        stage("destination_mutated_inventory", destination, lambda: inventory_witness(target_server, target_client, 41))
        stage("destination_mutated_vitals", destination, lambda: vitals_witness(target_server, target_client, CONTINUED_XP, CONTINUED_POSITION))
        disconnect_and_stop(destination, target_client, "destination")
        final_player = read_player(destination_world)
        detail["destination_player"] = write_json(directory / "destination-player.json", final_player)

        def saved_continuation() -> dict:
            selected = validate_player(final_player, count=41, xp=CONTINUED_XP, position=CONTINUED_POSITION)
            require(final_player["typed"]["XpSeed"] == original_player["typed"]["XpSeed"], "Enchantment seed was reset")
            opaque = {field: value for field, value in original_player["typed"].items() if field in OPAQUE}
            for field, value in opaque.items():
                require(final_player["typed"].get(field) == value, {"lost_opaque_field": field, "source": value,
                        "destination": final_player["typed"].get(field)})
            return {**selected, "source_snapshot": detail["source_player"], "destination_snapshot": detail["destination_player"],
                    "opaque_retention": {"result": "passed" if opaque else "unverified", "fields": list(opaque),
                        "scope": "Typed original fields only; no fixture sentinel, recipe/attribute/Brain/post-effect gameplay claim."},
                    "live_mutation": detail["stages"]["destination_mutation"], "third_boot_reload": "unverified"}

        record("saved_continuation", destination, saved_continuation, live=False)

        def saved_advancements() -> dict:
            final = read_advancements(destination_world, definitions)
            detail["destination_advancements"] = write_json(directory / "destination-advancements.json", final)
            return compare_advancements(original_advancements, final, added, destination_kind=target)

        record("advancement_continuation", destination, saved_advancements, live=False)

        def saved_statistics() -> dict:
            final = read_statistics(destination_world)
            detail["destination_statistics"] = write_json(directory / "destination-statistics.json", final)
            return {"direction": detail["direction"], "uuid": str(PLAYER_ID),
                    "source_json": statistics_counts(original_statistics, 3, 1), "destination_json": statistics_counts(final, 5, 2),
                    "source_live": detail["stages"]["source_statistics_live"]["detail"],
                    "destination_import": detail["stages"]["destination_statistics_import"]["detail"],
                    "destination_live": detail["stages"]["destination_statistics_live"]["detail"],
                    "interactions": {role: [detail["stages"][f"{role}_barrel_open_{opening}"]["detail"]
                        for opening in range(1, count + 1)] for role, count in (("source", 3), ("destination", 2))},
                    "logs": {role: {"client": str(fixture.server.directory / f"{role}-player.jsonl"),
                                    "server": str(fixture.server.log_path), "commands": str(fixture.server.directory / "commands.jsonl")}
                             for role, fixture in (("source", source), ("destination", destination))},
                    "primary_reports": stats_contract["reports"], "whole_world_copy": copy_evidence,
                    "scope": "Both runtimes in one transfer composite; received open_barrel 3/3/5 and independent stopped JSON 3/1 -> 5/2. Other counters are observations only."}

        record("statistics_continuation", destination, saved_statistics, live=False)
        detail["destination_after_stop"] = world_hashes(destination_world)
    except Exception as error:
        errors.append(f"{type(error).__name__}: {error}")
    finally:
        for fixture in reversed(fixtures):
            errors.extend(fixture.cleanup())
        try:
            if immutable is not None:
                detail["source_final"] = world_hashes(directory / "source/world")
                require(detail["source_final"] == immutable, "Immutable source world changed during destination continuation")
            require(digest(Path(report["java"]["path"])) == JDK_SHA256, "Pinned Java executable changed during run")
            for witness in report.get("primary_contracts", {}).get("statistics", {}).get("reports", {}).values():
                require(digest(Path(witness["path"])) == witness["sha256"], "Primary statistics report changed during run")
        except Exception as error:
            errors.append(f"Final provenance: {type(error).__name__}: {error}")
    report.setdefault("runtime_errors", {})[kind] = errors
    rows = finish_rows(CASES, results, errors)
    # If even force cleanup could not reap a process, do not start the other
    # direction. Preserve these rows before propagating to the global harness.
    report["servers"][kind] = rows
    alive = [process.pid for fixture in fixtures
             for process in [fixture.server.process, *(client.process for client in fixture.clients)]
             if process is not None and process.poll() is None]
    require(not alive, {"owned_processes_still_alive": alive})
    return rows


if __name__ == "__main__":
    oracle.acquire = cached_reference
    suite.acquire = cached_reference
    raise SystemExit(paired_main("player-data", CASES, run_direction, __doc__, DEFERRED))
