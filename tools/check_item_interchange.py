#!/usr/bin/env python3
"""Actual 26.3/protocol-777/DV5023 item import, mutation and reload composites.

Six sequential boots: fresh vanilla -> Rust -> Rust reload, then fresh Rust ->
vanilla -> vanilla reload. Commands author every item on the actual source. The
whole stopped world is copied unchanged, with the same offline player identity.
Generic PLAY probes deliberately do not decode component-bearing inventory
packets. Evidence is authoritative live item NBT plus independent typed stopped
player.dat readback, not item wire/rendering/use/equipment or full-component parity.
Fixtures below are codec-derived expectations; actual Java execution is required.
An explicitly supplied forensic bundle adds audit provenance only; omitting it
reads no bundle and still requires the pinned reference and actual runtime checks.
No downloads, builds, synthetic save writes, UUID conversion or legacy migration.
Importing this module does not start any processes.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
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
from typing import Callable, TextIO
import uuid

sys.dont_write_bytecode = True
import oracle
from check_block_data import interaction_deadline
from check_container_metadata import Fixture, manifest as inherited_manifest, now, typed_predicate, wait_predicate
from check_player_data import (
    JDK_SHA256, PLAYER_ID, UUID_INTS, cached_reference, clone_world, command_result,
    identity_witness, read_bytes, read_player, read_statistics, teleport_exact,
    unique_object, world_hashes, write_json,
)
from parity import protocol, suite, worlds
from parity.bot import Client, NAME
from parity.furnaces import digest, require_frozen
from parity.protocol import require

BASE = "5bf65bd2846bcf8199a1dbbbb68513f1d8446eb2"
INNER_JAR_SHA256 = "a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6"
POTION_MAPPING = "vendor/pumpkin/crates/pumpkin-data/src/generated/potion.rs"
POTION_MAPPING_SHA256 = "8406368beaa93ccf5c73a088f302d17d26e251b3c6757d4416c0ea1245a40f82"
PRIMARY_HASHES = {
    "handoff.txt": "6718498fc8936b7146fabc93d6c9f92851325663a9135685179abc0fa3d13212",
    "manifest.json": "9ceb4e7efbae94d72840d00fc4e52fc5a34baa626ef425af0bae3f63d55ef5e4",
    "SHA256SUMS": "acc3f96726998d266faf7fee6e59513a5897b95c79bd6b998e6feb3899040ba7",
    "nested-items.javap.txt": "cd9efdd9209694bfbcf2d679c0c979771950d3057bfcc4d451c83940f38b88ca",
    "potion-template-fireworks.javap.txt": "11cc3c8731c6c44d714a9f873ea1588003bdfe067fafd4823ef3210a0fa91b2a",
    "stack-effects.javap.txt": "d9943a79834635e97b046e6f11e6e60bd2067c7cbb06c565fac22b1dbb2eb989",
    "official-26.3-details-extracodecs-livingentity.javap.txt": "52f687d82a5b53f79cdeabc4ebdb3dc3f43ffd7d876ad94d38ac41ca865af85d",
    "official-26.3-player-extracodecs-verbose.javap.txt": "fda2c3360821c1142abc97eba6fc7b70b55fb5253c74e459341ce89f4a2ecb43",
}
CASES = {
    "item_interchange.water_bottle": ([0], "Explicit water potion name survives live import and destination save/reload."),
    "item_interchange.potion_effects": ([1, 2], "Named swiftness and Byte-255 speed, default/flag controls survive import and reload."),
    "item_interchange.nested_stacks": ([3, 4], "Shulker slot-0 cobblestone64, default-count slot-1, bundle cobblestone32 and exact opaque typed trees survive."),
    "item_interchange.fireworks": ([5, 6], "Empty-explosion flight3 Byte and zero/default controls survive import and reload."),
    "item_interchange.mutation_reload": ([7], "Ordinary cobblestone count37 imports, real item command changes it to41, new destination process/PLAY login reloads41."),
}
DEFERRED = [{"id": "item_interchange.remaining_components", "result": "unverified", "reason":
    "Books, hidden effects, nonempty firework explosions and loaded crossbow projectiles (network codec placeholder) are deferred, never expected-refusal successes. No gameplay, item wire decoding, rendering, equipment, full components, legacy private Statistics migration or full-server parity claim. Existing seven player assertions, barrel/XP cases and their failures are unchanged."}]
POSITION = {"x": 4097.5, "y": 200.0, "z": 3.5, "yaw": 90.0, "pitch": 0.0}
OPAQUE_SNBT = '{components:{},nested:{components:{},item:{id:"opaque",count:0,components:{}},keep:[I;1,-2,3]}}'
# Exact component grammar is handled by ItemStackArgumentType + SnbtParser;
# primary codecs, not bracket splitting, determine these command values.
ITEMS = (
    "minecraft:potion[minecraft:potion_contents={potion:'minecraft:water'}]",
    "minecraft:splash_potion[minecraft:potion_contents={potion:'minecraft:swiftness'}]",
    'minecraft:potion[minecraft:potion_contents={custom_effects:[{id:"minecraft:speed",amplifier:-1b,duration:40,show_particles:0b},{id:"minecraft:luck",amplifier:0b,duration:0,ambient:0b,show_particles:1b}]}]',
    'minecraft:shulker_box[minecraft:container=[{slot:0,item:{id:"minecraft:cobblestone",count:64}},{slot:1,item:{id:"minecraft:cobblestone"}}],minecraft:custom_data=' + OPAQUE_SNBT + ']',
    'minecraft:bundle[minecraft:bundle_contents=[{id:"minecraft:cobblestone",count:32}],minecraft:custom_data=' + OPAQUE_SNBT + ']',
    'minecraft:firework_rocket[minecraft:fireworks={flight_duration:3b,explosions:[]}] 4',
    'minecraft:firework_rocket[minecraft:fireworks={flight_duration:0b,explosions:[]}]',
    'minecraft:cobblestone 37',
)
MUTATION = f"item replace entity {NAME} hotbar.7 with minecraft:cobblestone 41"


def atom(kind: str, value: int) -> dict:
    return {"nbt_type": kind, "value": value}


def nbt_list(values: list) -> dict:
    return {"nbt_type": "List", "element_type": 10, "value": values}


def expected_inventory(count: int = 37) -> dict[int, dict]:
    """Codec-derived typed expectations, never written into a server world."""
    opaque = {"components": {}, "nested": {"components": {}, "item": {
        "id": "opaque", "count": atom("Int", 0), "components": {}},
        "keep": {"nbt_type": "IntArray", "value": [1, -2, 3]}}}
    specifications = (
        ("potion", 1, {"minecraft:potion_contents": {"potion": "minecraft:water"}}),
        ("splash_potion", 1, {"minecraft:potion_contents": {"potion": "minecraft:swiftness"}}),
        ("potion", 1, {"minecraft:potion_contents": {"custom_effects": nbt_list([
            {"id": "minecraft:speed", "amplifier": atom("Byte", -1), "duration": atom("Int", 40),
             "show_particles": atom("Byte", 0), "show_icon": atom("Byte", 0)},
            {"id": "minecraft:luck", "show_icon": atom("Byte", 1)},
        ])}}),
        ("shulker_box", 1, {"minecraft:container": nbt_list([
            {"slot": atom("Int", 0), "item": {"id": "minecraft:cobblestone", "count": atom("Int", 64)}},
            {"slot": atom("Int", 1), "item": {"id": "minecraft:cobblestone"}},
        ]), "minecraft:custom_data": deepcopy(opaque)}),
        ("bundle", 1, {"minecraft:bundle_contents": nbt_list([
            {"id": "minecraft:cobblestone", "count": atom("Int", 32)}]), "minecraft:custom_data": deepcopy(opaque)}),
        ("firework_rocket", 4, {"minecraft:fireworks": {"flight_duration": atom("Byte", 3)}}),
        ("firework_rocket", 1, {"minecraft:fireworks": {}}),
        ("cobblestone", count, {}),
    )
    return {slot: {"Slot": atom("Byte", slot), "id": "minecraft:" + name,
                   "count": atom("Int", amount), **({"components": components} if components else {})}
            for slot, (name, amount, components) in enumerate(specifications)}


def typed_int(value: object, kind: str, minimum: int, maximum: int) -> int:
    require(isinstance(value, dict) and set(value) == {"nbt_type", "value"}
            and value["nbt_type"] == kind and type(value["value"]) is int
            and minimum <= value["value"] <= maximum, {"expected_type": kind, "actual": value})
    return value["value"]


def compounds(value: object) -> list:
    require(isinstance(value, dict) and set(value) == {"nbt_type", "element_type", "value"}
            and value["nbt_type"] == "List" and type(value["element_type"]) is int
            and isinstance(value["value"], list), {"invalid_typed_list": value})
    require(value["element_type"] == 10 or (value["element_type"] == 0 and not value["value"]), value)
    require(all(isinstance(entry, dict) for entry in value["value"]), value)
    return value["value"]


def semantic_stack(stack: dict, *, nested: bool = False, depth: int = 0) -> dict:
    """Compare only named codec defaults at known stack/component boundaries.

    No generic recursive normalization: custom_data, unknown keys and their typed
    trees are retained exactly. Raw snapshots are always retained separately.
    Byte amplifier/flight and flag types never coerce from Int or other numerics.
    """
    require(depth <= 8 and isinstance(stack, dict), "Malformed/deep item stack")
    result = deepcopy(stack)
    require(isinstance(result.get("id"), str) and bool(worlds.IDENTIFIER.fullmatch(result["id"])), result)
    if nested:
        result.setdefault("count", atom("Int", 1))
    typed_int(result.get("count"), "Int", 1, 99)
    components = result.get("components", {})
    require(isinstance(components, dict), "Item components must be a compound")
    if not components:
        result.pop("components", None)
        return result
    potion = components.get("minecraft:potion_contents")
    if potion is not None:
        require(isinstance(potion, dict), "Fixture potion must use the full compound codec")
        if "custom_effects" in potion:
            effects = compounds(potion["custom_effects"])
            for effect in effects:
                require(isinstance(effect.get("id"), str), effect)
                for key, kind, default, minimum, maximum in (
                        ("amplifier", "Byte", 0, -128, 127), ("duration", "Int", 0, -2147483648, 2147483647),
                        ("ambient", "Byte", 0, 0, 1), ("show_particles", "Byte", 1, 0, 1)):
                    effect.setdefault(key, atom(kind, default))
                    typed_int(effect[key], kind, minimum, maximum)
                effect.setdefault("show_icon", deepcopy(effect["show_particles"]))
                typed_int(effect["show_icon"], "Byte", 0, 1)
            if effects:
                potion["custom_effects"] = nbt_list(effects)
            else:
                potion.pop("custom_effects")
    for key in ("minecraft:container", "minecraft:bundle_contents"):
        if key not in components:
            continue
        entries = compounds(components[key])
        if key == "minecraft:container":
            seen = set()
            for entry in entries:
                slot = typed_int(entry.get("slot"), "Int", 0, 255)
                require(slot not in seen, f"Duplicate nested container slot: {slot}")
                seen.add(slot)
                entry["item"] = semantic_stack(entry.get("item"), nested=True, depth=depth + 1)
        else:
            entries = [semantic_stack(entry, nested=True, depth=depth + 1) for entry in entries]
        components[key] = nbt_list(entries)
    if "minecraft:fireworks" in components:
        firework = components["minecraft:fireworks"]
        require(isinstance(firework, dict), "Fireworks must be a compound")
        firework.setdefault("flight_duration", atom("Byte", 0))
        typed_int(firework["flight_duration"], "Byte", -128, 127)
        if "explosions" in firework:
            require(not compounds(firework["explosions"]), "Nonempty explosions are outside this fixture")
            firework.pop("explosions")
    return result


def inventory_view(typed: dict) -> dict[int, dict]:
    entries = compounds(typed.get("Inventory"))
    require(len(entries) == len(ITEMS), "Fixture requires exactly eight occupied hotbar slots")
    slots = {}
    for entry in entries:
        slot = typed_int(entry.get("Slot"), "Byte", 0, len(ITEMS) - 1)
        require(slot not in slots, f"Duplicate player inventory slot: {slot}")
        slots[slot] = semantic_stack(entry)
    return slots


def saved_witness(world: Path, expected: dict[int, dict], artifact: Path) -> dict:
    saved = read_player(world)
    # Preserve the actual typed source even when the expectation fails.
    write_json(artifact, saved)
    typed = saved["typed"]
    require(typed.get("UUID") == {"nbt_type": "IntArray", "value": list(UUID_INTS)}, "Stopped player UUID differs")
    require(typed_int(typed.get("DataVersion"), "Int", 5023, 5023) == 5023, "Wrong player DataVersion")
    require("Statistics" not in typed, "Private Statistics NBT is not canonical statistics storage")
    actual = inventory_view(typed)
    require(actual == expected, {"expected_semantics": expected, "actual_semantics": actual})
    return {**saved, "inventory_semantics": actual}


def typed_snbt(value: object) -> str:
    """Emit predicates from explicit typed expectations, never parse console SNBT."""
    if isinstance(value, str):
        return json.dumps(value)
    require(isinstance(value, dict), "Unsupported predicate tree")
    kind = value.get("nbt_type")
    if kind in ("Byte", "Int"):
        number = typed_int(value, kind, -128 if kind == "Byte" else -2147483648,
                           127 if kind == "Byte" else 2147483647)
        return str(number) + ("b" if kind == "Byte" else "")
    if kind == "IntArray":
        return "[I;" + ",".join(str(number) for number in value["value"]) + "]"
    if kind == "List":
        return "[" + ",".join(typed_snbt(entry) for entry in compounds(value)) + "]"
    return "{" + ",".join(json.dumps(key) + ":" + typed_snbt(entry) for key, entry in value.items()) + "}"


def live_witness(fixture: Fixture, client: Client, count: int) -> dict:
    identity = identity_witness(fixture.server, client)
    state = client.state()
    require(state.get("player_inventory") is None and state.get("menu_packets") == [],
            "Focused inventory decoding must stay OFF")
    # Omitted nested count=1 and effect/rocket defaults may legitimately be absent
    # from live serialized NBT. Positive predicates check their nondefault typed
    # fields; the independent stopped comparisons check the full default semantics.
    inventory = expected_inventory(count)
    predicate = "if data entity " + NAME + " " + typed_snbt({"Inventory": nbt_list(list(inventory.values()))})
    return {"identity": identity, "typed_items": typed_predicate(fixture.server, predicate),
            "occupied_slots": command_result(fixture.server, f"data get entity {NAME} Inventory", len(ITEMS)),
            "focused_item_decoding": False, "inventory_packets_observed": state["inventory_packets"],
            "scope": "Authoritative live typed subset plus full independent stopped comparison; packets are only counted."}


def verify_artifacts(directory: Path, pins: dict[str, str]) -> dict:
    require(directory.is_dir() and not directory.is_symlink(), "Missing/linked primary evidence directory")
    result = {}
    for name, expected in pins.items():
        require(name not in ("", ".", "..") and Path(name).name == name, "Primary filename must be a basename")
        path = directory / name
        require(path.is_file() and not path.is_symlink(), f"Missing/linked primary artifact: {path}")
        raw = read_bytes(path)
        actual = hashlib.sha256(raw).hexdigest()
        require(actual == expected, f"Primary artifact changed: {path}")
        result[name] = {"path": str(path.resolve()), "sha256": actual, "bytes": len(raw)}
    return result


def harness_manifest() -> dict:
    return {**inherited_manifest(), **{path: digest(oracle.ROOT / path) for path in (
        "tools/check_item_interchange.py", "tests/test_item_interchange.py", POTION_MAPPING,
        "vendor/pumpkin/crates/pumpkin-command/src/argument_types/item.rs",
        "vendor/pumpkin/crates/pumpkin-command/src/snbt/mod.rs",
        "vendor/pumpkin/crates/pumpkin-command/src/snbt/rules.rs",
    )}}


def sidecars(world: Path) -> dict:
    statistics = read_statistics(world)
    path = world / "players/advancements" / f"{PLAYER_ID}.json"
    advancements = json.loads(read_bytes(path), object_pairs_hook=unique_object)
    require(isinstance(advancements, dict) and type(advancements.get("DataVersion")) is int
            and advancements["DataVersion"] == 5023, "Canonical advancement DataVersion required")
    return {"statistics": {"path": statistics["path"], "sha256": statistics["sha256"], "data": statistics["data"]},
            "advancements": {"path": str(path), "sha256": digest(path), "data": advancements},
            "scope": "Canonical copied sidecars observed; no counter/advancement continuation assertions or private NBT migration."}


def run_direction(kind: str, directory: Path, args: argparse.Namespace, report: dict) -> list[dict]:
    target = "rust" if kind == "vanilla" else "vanilla"
    direction = f"{kind}-to-{target}"
    detail = {"direction": direction, "source_kind": kind, "destination_kind": target, "stages": {}}
    report["transfers"][kind] = detail
    fixtures: list[Fixture] = []
    errors = []
    immutable = None

    def stage(label: str, fixture: Fixture, action: Callable, *, ordinary: bool = True):
        entry = {"started": now(), "directory": str(fixture.server.directory), "engine": fixture.kind}
        detail["stages"][label] = entry
        try:
            if ordinary:
                with interaction_deadline(fixture.server, label):
                    value = action()
            else:
                value = action()
            require(not fixture.server.broken and not any(client.failure for client in fixture.clients),
                    "Server/client fatal error")
            entry.update(result="passed", detail=value)
            return value
        except Exception as error:
            entry.update(result="failed", error=f"{type(error).__name__}: {error}")
            raise
        finally:
            entry["finished"] = now()

    def boot(fixture: Fixture, role: str, before_connect: Callable | None = None) -> Client:
        try:
            stage(role + "_startup", fixture, lambda: fixture.start(role), ordinary=False)
        finally:
            fixture.processes[-1].update(role=role, direction=direction, directory=str(fixture.server.directory))
            if fixture.server.process is not None:
                fixture.processes[-1].update(pid=fixture.server.process.pid, argv=fixture.server.process.args)
            detail.setdefault("watchdogs", {})[role] = fixture.server.abort_events
        stage(role + "_status", fixture, lambda: protocol.status_fields(fixture.server.port))
        stage(role + "_freeze", fixture, lambda: (fixture.server.command("tick freeze"), require_frozen(fixture.server)))
        if before_connect is not None:
            before_connect()
        client = stage(role + "_connect", fixture, lambda: fixture.connect(role + "-player", metadata=False), ordinary=False)
        detail["stages"][role + "_connect"]["detail"] = {
            "pid": client.process.pid, "argv": client.process.args, "name": client.name,
            "log": str(fixture.server.directory / f"{role}-player.jsonl")}
        stage(role + "_initial_chunks", fixture, lambda: {"chunks": client.wait_state(
            lambda state: state.get("chunks", 0) >= 49, timeout=60)["chunks"]}, ordinary=False)
        return client

    def stop(fixture: Fixture, client: Client, role: str) -> None:
        stage(role + "_disconnect", fixture, lambda: fixture.close(client))
        stage(role + "_no_player", fixture, lambda: wait_predicate(fixture.server, f"unless entity @a[name={NAME}]"))
        stage(role + "_save", fixture, fixture.server.save_flush, ordinary=False)
        stage(role + "_saved_frozen", fixture, lambda: require_frozen(fixture.server))
        stage(role + "_stop", fixture, fixture.stop, ordinary=False)
        require(fixture.server.process.returncode == 0 and all(
            entry.process.returncode == 0 and not entry.forced_close and not entry.failure for entry in fixture.clients),
            "Every owned process must exit cleanly before copy/readback")

    def snapshot(fixture: Fixture, role: str, expected: dict) -> dict:
        artifact = directory / f"{role}-player.json"
        detail.setdefault("typed_save_artifacts", {})[role] = str(artifact)
        saved = stage(role + "_typed_save", fixture,
                      lambda: saved_witness(fixture.server.directory / "world", expected, artifact), ordinary=False)
        detail["stages"][role + "_typed_save"]["detail"] = {
            "artifact": str(artifact), "sha256": saved["sha256"], "inventory_semantics": saved["inventory_semantics"]}
        stage(role + "_sidecars", fixture, lambda: sidecars(fixture.server.directory / "world"), ordinary=False)
        return saved["inventory_semantics"]

    try:
        source = Fixture(kind, directory / "source", args, report)
        fixtures.append(source)
        source.server.configure()

        def lane() -> None:
            # Fixed safe platform, frozen before login, as in the player harness.
            stage("source_forceload", source, lambda: source.server.command("forceload add 4096 0"))
            stage("source_loaded_lane", source, lambda: suite.wait_loaded(source.server, x=4096, z=0), ordinary=False)
            stage("source_platform", source, lambda: [source.server.command(command) for command in (
                "fill 4096 199 0 4111 199 15 stone", "difficulty peaceful", "scoreboard objectives add pdprobe dummy")])

        client = boot(source, "source", lane)
        stage("source_position", source, lambda: teleport_exact(source.server, client, POSITION))
        stage("source_prepare_player", source, lambda: [source.server.command(command) for command in (
            f"gamemode survival {NAME}", f"clear {NAME}")])
        for slot, item in enumerate(ITEMS):
            stage(f"source_author_slot_{slot}", source, lambda slot=slot, item=item: command_result(
                source.server, f"item replace entity {NAME} hotbar.{slot} with {item}", 1, mode="success"))
        stage("source_live_items", source, lambda: live_witness(source, client, 37))
        stop(source, client, "source")
        source_world = source.server.directory / "world"
        immutable = world_hashes(source_world)
        detail["source_after_stop"] = immutable
        reference = snapshot(source, "source", {slot: semantic_stack(stack) for slot, stack in expected_inventory().items()})

        destination = Fixture(target, directory / "destination", args, report)
        fixtures.append(destination)
        destination.server.configure()
        destination_world = destination.server.directory / "world"
        detail["discarded_unstarted_scaffold"] = world_hashes(destination_world)
        shutil.rmtree(destination_world)
        detail["copy"] = {}
        clone_world(source_world, destination_world, detail["copy"])
        require(detail["copy"]["source_before_copy"] == immutable, "Source changed before transfer")
        imported = boot(destination, "destination")
        stage("destination_live_import", destination, lambda: live_witness(destination, imported, 37))
        stage("destination_real_mutation", destination, lambda: command_result(destination.server, MUTATION, 1, mode="success"))
        stage("destination_live_mutation", destination, lambda: live_witness(destination, imported, 41))
        stop(destination, imported, "destination")
        continued = deepcopy(reference)
        continued[7]["count"] = atom("Int", 41)
        snapshot(destination, "destination", continued)
        first_pid = destination.server.process.pid
        reloaded = boot(destination, "reload")
        require(destination.server.process.pid != first_pid, "Reload requires a new server process")
        stage("reload_live_items", destination, lambda: live_witness(destination, reloaded, 41))
        stop(destination, reloaded, "reload")
        snapshot(destination, "reload", continued)
        detail["destination_after_reload"] = world_hashes(destination_world)
        detail["completed_three_boots"] = True
    except Exception as error:
        errors.append(f"{type(error).__name__}: {error}")
    finally:
        for fixture in reversed(fixtures):
            errors.extend(fixture.cleanup())
        detail["owned_clients"] = [{"pid": client.process.pid, "argv": client.process.args,
            "exit_code": client.process.returncode, "forced_close": client.forced_close,
            "failure": client.failure, "stderr": client.stderr.name}
            for fixture in fixtures for client in fixture.clients]
        try:
            if immutable is not None:
                detail["source_final"] = world_hashes(directory / "source/world")
                require(detail["source_final"] == immutable, "Source world changed during destination continuation")
        except Exception as error:
            errors.append(f"Source immutability: {type(error).__name__}: {error}")
    detail["runtime_errors"] = errors
    rows = [{"id": identifier, "direction": direction, "features": [], "slots": slots,
             "expectation": expectation, "result": "failed" if errors else "passed",
             "evidence": {"transfer": direction, "stages": list(detail["stages"]),
                          "source_reference": str(directory / "source-player.json"),
                          "destination_save": str(directory / "destination-player.json"),
                          "destination_reload": str(directory / "reload-player.json")},
             **({"runtime_errors": errors} if errors else {})}
            for identifier, (slots, expectation) in CASES.items()]
    report["servers"][kind] = rows
    alive = [process.pid for fixture in fixtures for process in (
        fixture.server.process, *(client.process for client in fixture.clients))
        if process is not None and process.poll() is None]
    require(not alive, {"owned_processes_still_alive": alive})
    return rows


def reserve_output(path: Path, cache: Path) -> TextIO:
    """Only fresh dedicated report names; never overwrite a ledger or follow links."""
    require(re.fullmatch(r"item-interchange-[A-Za-z0-9_-]+\.json", path.name), "Use a dedicated item-interchange-*.json report name")
    cache = cache.resolve()
    require(path.absolute().is_relative_to(cache) and path.resolve().is_relative_to(cache), "Report must stay under repository .cache")
    require(not path.is_symlink() and not any(parent.is_symlink() for parent in path.parents if parent != cache and parent.is_relative_to(cache)),
            "Linked report paths are refused")
    path.parent.mkdir(parents=True, exist_ok=True)
    return path.open("x")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accept-eula", action="store_true")
    parser.add_argument("--java", required=True)
    parser.add_argument("--reference-cache", required=True, type=Path)
    parser.add_argument("--rust-binary", required=True, type=Path)
    parser.add_argument("--rust-source-commit", required=True, help="Full source commit used to build the supplied binary; caller attestation")
    parser.add_argument("--primary-evidence", type=Path,
                        help="Optional forensic audit bundle matching PRIMARY_HASHES; no bundle is read when omitted")
    parser.add_argument("--workers", type=int, choices=(2,), default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, default=oracle.ROOT / f".cache/item-interchange-{uuid.uuid4().hex}.json")
    args = parser.parse_args()
    if not args.accept_eula:
        parser.error("Explicit --accept-eula is required for the isolated local reference: https://aka.ms/MinecraftEULA")
    if not 0 < args.startup_timeout <= 180 or not re.fullmatch(r"[0-9a-f]{40}", args.rust_source_commit):
        parser.error("Startup bound must be <=180s and Rust source commit must be a full lowercase SHA")
    if not args.rust_binary.is_file() or not os.access(args.rust_binary, os.X_OK):
        parser.error("--rust-binary must be an existing executable; this harness never builds")
    try:
        output = reserve_output(args.output, oracle.ROOT / ".cache")
    except (AssertionError, OSError) as error:
        parser.error(str(error))
    runs = oracle.ROOT / ".cache/runs"
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="item-interchange-", dir=runs))
    report = {"schema_version": 1, "kind": "item-interchange", "created_at": now(), "result": "failed",
              "target_version": "26.3", "protocol": 777, "data_version": 5023, "fixture": str(run),
              "scope": __doc__, "deferred": DEFERRED, "servers": {}, "transfers": {}, "processes": {}, "failed": [],
              "primary_evidence": {"status": "NOT_SUPPLIED" if args.primary_evidence is None else "SUPPLIED_UNVERIFIED",
                                   "role": "Optional forensic audit provenance, not actual Java save evidence"},
              "environment": {"host": socket.gethostname(), "platform": sys.platform, "auth": "offline isolated 127.0.0.1",
                              "workers": 2, "seed": "12345678", "ordinary_watchdog_seconds": 20,
                              "initial_chunks": 49, "chunk_timeout_seconds": 60},
              "case_mapping": "Each ID is a both-runtime persistence composite in vanilla-to-rust and rust-to-vanilla; no per-engine feature mapping."}
    began = time.monotonic()
    try:
        report["source_state"] = suite.git_state()
        require(report["source_state"]["dirty"] is False, "Commit the harness before running")
        require(oracle.TARGET["version"] == "26.3" and oracle.TARGET["protocol"] == 777, "Changed target")
        require(NAME == "FerraloomTest" and uuid.UUID(bytes=hashlib.md5(
            ("OfflinePlayer:" + NAME).encode(), usedforsecurity=False).digest(), version=3) == PLAYER_ID, "Changed offline identity")
        report["harness_manifest"] = harness_manifest()
        require(report["harness_manifest"][POTION_MAPPING] == POTION_MAPPING_SHA256, "Changed potion name table")
        if args.primary_evidence is not None:
            report["primary_artifacts"] = verify_artifacts(args.primary_evidence, PRIMARY_HASHES)
            report["primary_evidence"]["status"] = "VERIFIED_BEFORE_RUN"
        report["primary_contracts"] = {"scope": "Codec-derived expectations, not pre-existing actual Java fixtures. Runtime evidence requires actual source-authored saves, typed readback and the pinned official Java reference, independently of the optional forensic bundle.",
            "origin_commit": "25854145f3eaf4858ae64943c9fad5166d0d2bf3", "integrated_item_revision": "ae38e82",
            "harness_base": BASE, "types": "ItemStack Int count; template count defaults1; empty components optional. Effects amplifier UNSIGNED_BYTE, duration INT, ambient=false, particles=true, icon defaults to particles. Fireworks UNSIGNED_BYTE flight defaults0, explosions defaults[].",
            "equivalences": "Player Inventory entries are keyed by unique Byte Slot. Only known stack count/components, effect defaults, empty effect/explosion lists and flight0 compare semantically. Nested list order, unknown fields and opaque typed trees remain exact; raw NBT is never rewritten.",
            "item_ids": "Named minecraft identifiers only; no numeric registry IDs or Debug names are used."}
        oracle.CACHE = args.reference_cache.resolve()
        oracle.acquire = cached_reference
        suite.acquire = cached_reference
        official = cached_reference()
        report["official"] = {"path": str(official), "sha1": digest(official, "sha1"), "sha256": digest(official),
                              "inner_sha256": digest(oracle.CACHE / "minecraft.jar")}
        require(report["official"]["inner_sha256"] == INNER_JAR_SHA256, "Changed official codec JAR")
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        require(digest(Path(args.java)) == JDK_SHA256, "Requires the cached pinned JDK25 executable")
        version = subprocess.run([args.java, "-version"], capture_output=True, text=True, timeout=15, check=True)
        report["java"] = {"path": args.java, "sha256": JDK_SHA256, "version": version.stdout + version.stderr}
        require(re.search(r'version "25\.', report["java"]["version"]), "Requires JDK25")
        original = args.rust_binary.resolve()
        binary_hash = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        require(digest(args.rust_binary) == digest(original) == binary_hash, "Binary changed while pinning")
        report["rust_binary"] = {"source": str(original), "pinned": str(args.rust_binary), "sha256": binary_hash,
                                 "source_commit": args.rust_source_commit, "source_association": "Caller attestation, not binary-embedded proof"}
        for kind in ("vanilla", "rust"):
            print(f"Running item interchange {kind} source on {socket.gethostname()} loopback: {run / kind}", flush=True)
            run_direction(kind, run / kind, args, report)
        require(harness_manifest() == report["harness_manifest"] and suite.git_state() == report["source_state"], "Harness/source changed during run")
        if args.primary_evidence is not None:
            report["primary_artifacts_after"] = verify_artifacts(args.primary_evidence, PRIMARY_HASHES)
            require(report["primary_artifacts_after"] == report["primary_artifacts"], "Primary evidence changed")
            report["primary_evidence"]["status"] = "VERIFIED_BEFORE_AND_AFTER_RUN"
        require(digest(original) == digest(args.rust_binary) == binary_hash, "Target binary changed")
        require(digest(official) == report["official"]["sha256"]
                and digest(oracle.CACHE / "minecraft.jar") == INNER_JAR_SHA256
                and digest(Path(args.java)) == JDK_SHA256, "Pinned Java inputs changed")
        report["failed"] = [f'{row["direction"]}:{row["id"]}' for rows in report["servers"].values()
                            for row in rows if row["result"] != "passed"]
        report["result"] = "failed" if report["failed"] else "passed"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        report["failed"].append("harness.global")
        for kind, target in (("vanilla", "rust"), ("rust", "vanilla")):
            existing = {row["id"]: row for row in report["servers"].get(kind, [])}
            report["servers"][kind] = [{**existing.get(identifier, {}), "id": identifier,
                "direction": f"{kind}-to-{target}", "features": [], "result": "failed",
                "global_error": report["error"]} for identifier in CASES]
    finally:
        report.update(finished=now(), duration_seconds=round(time.monotonic() - began, 3))
        raw = json.dumps(report, indent=2, allow_nan=False) + "\n"
        with (run / "report.json").open("x") as retained:
            retained.write(raw)
        with output:
            output.write(raw)
    print(f"item interchange {report['result']}: {args.output}; retained artifacts: {run}", flush=True)
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
