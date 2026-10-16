#!/usr/bin/env python3
"""Bounded real 26.3/777/DV5023 horn campaign, separate from existing gates.

Four sequential boots: Java author/calibrate -> Rust transfer/use -> Rust reload
-> Java reopen/use. Only the source Java authors items. Whole stopped worlds are
copied unchanged; players retain their real offline identities. Runtime output is
evidence only after execution. Source helper tests are not runtime observations.
No downloads, builds, synthetic NBT writes, UUID conversion or suite integration.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
from functools import wraps
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
import uuid
from typing import Callable

sys.dont_write_bytecode = True
import oracle
from check_block_data import close_menu, interaction_deadline
from check_container_metadata import Fixture, manifest as inherited_manifest, now, typed_predicate, wait_predicate
from check_item_interchange import INNER_JAR_SHA256, atom, compounds, typed_int, typed_snbt, verify_artifacts
from check_player_data import (JDK_SHA256, NAME, PLAYER_ID, UUID_INTS, PlayerReader,
    cached_reference, clone_world, command_result, identity_witness, read_bytes,
    read_player, read_statistics, teleport_exact, unique_object, world_hashes, write_json)
from parity import protocol, suite, worlds
from parity.bot import Client, delivered_chat
from parity.furnaces import digest, require_frozen, step_exact
from parity.protocol import require

BUILTINS = tuple(f"minecraft:{name}_goat_horn" for name in
                 ("admire", "call", "dream", "feel", "ponder", "seek", "sing", "yearn"))
SING, PONDER = BUILTINS[6], BUILTINS[4]
OBSERVER = "HornObserver"
CHEST = (4097, 200, 1)
POS = {"x": 4097.5, "y": 200.0, "z": 3.5, "yaw": 90.0, "pitch": 0.0}
GROUP = "test:horn_group"
BOOTS = ("java_source", "rust_import", "rust_reload", "java_return")
CASES = (
    "horns.registry_each_connection", "horns.java_predicted_hash_acceptance",
    "horns.eight_references_transfer", "horns.sing_both_hands",
    "horns.default_ponder_both_hands", "horns.removal_and_opaque_controls",
    "horns.shared_group_retry", "horns.held_expiry_and_explicit_seconds",
    "horns.rust_restart", "horns.java_reopen",
)
GAPS = [
    "No full-parity or verified-family claim; only this completed bounded campaign is evidence.",
    "Predicted checksum acceptance is inferred from actual Java responses and subsequent actions; direct numeric Java hash is not exposed.",
    "All eight effective builtin identities transfer; both-hand gameplay is bounded to Sing and default Ponder.",
    "Ponder equals the vanilla item default, so its item patch may be absent. Raw evidence distinguishes seven nondefault holder payloads from default Ponder.",
    "Inline/direct holders and unknown/custom/overridden instrument profiles are explicitly unsupported.",
    "No sculk/vibration, sound radius, rendering, arbitrary datapack or other component gameplay claim.",
    "Opaque custom_data and explicit use_cooldown controls are Java-authored player slots; they do not exercise predicted component hashes.",
    "PLAY Login parsing is deliberately header-only for authoritative entity-ID association; the entire raw packet and undecoded remainder are retained, without a full Login codec/parity claim.",
]
PRIMARY = {
    "r25-instrument-primary/Instrument-Items-Holder-Codecs.javap.txt": "6752df8ed74912db0a51b7b3a32c99f2113d9dc7a5c2fb17324b90ac1c8736bb",
    "r25-instrument-primary/RegistryHolder-ByteBuf33-Cooldowns.javap.txt": "ef3c9c4dead9a07da0d2b7af7e89faaa1920a96d1bcae84519de71574bdea350",
    "r26-identifier-primary/Identifier.javap.txt": "b8de87b4617c5e47209a7c43e23b695dddc999b95f7af3909cfc91af4e5f7852",
    "item-name-primary/primary-codecs.txt": "4465f73168b67c0aadbed931af3342e2d417dad443ee3ce3c06a0556011d179f",
    "item-name-primary/primary-details.txt": "8e023559ae65aad60441be822e28b9e2fc0372259bddb7b6d07ff4974bbbf29a",
    "r26-horn-packet-primary/HornPackets.javap.txt": "2794ae66b19ab782ded6528debe06fb2eb05cbdb1c1d1dc838f54e3655072ee5",
    "r26-horn-packet-primary/receipt.json": "ee6e750a6ebe00fef866e3d846d402792c04f5574989f3621d522b6ac006f0ec",
    "r26-horn-entity-sound-primary/EntitySound.javap.txt": "7984142429e6b28b3c036b66876d9d03d75781352bd319c2868be463ab47338b",
    "r26-horn-entity-sound-primary/receipt.json": "b9e07b2452f457034885977b31a5c7ad28feb15e0a6227086811a74246dc84d4",
    "round23-primary/javap-ServerLevel.txt": "6affa9e7974302dbb5246337b9f9957802f382f06ea0390ea13200ceaf4636d5",
    "end-return-primary25/EndPortalBlock-PlayerList.javap.txt": "aa4fc7d86718597de46f8411b2111e8c2124a0d56e9881af03d437cc5285fffd",
    "r26-horn-login-primary/Login.javap.txt": "d08d3a1429c2494b1aaf8e7f4aef39bc731d4692e3db9cafd1cb8cba6e932ee0",
    "r26-horn-login-primary/receipt.json": "a3248966a9510d915bb25311adf54cb5d98d184d9bd9701a9b3e669358035377",
}
REPORT_PINS = {
    "registries.json": "51f62d56f8bd0e9134ee88dd9226b64e9e56cf6a32377540d44291bf47ecdac6",
    "packets.json": "2f0486311f18b7d3ec232028f3bff82c657d2f0240d2d65a3df40408db926886",
}


def stack(instrument: str | None = None, *, removed: bool = False, opaque: bool = False,
          cooldown: bool = False) -> dict:
    """Typed expected patch, never written to a world. Ponder is the item default."""
    components = {}
    if instrument is not None and instrument != PONDER:
        require(instrument in BUILTINS, "Unknown expected reference")
        components["minecraft:instrument"] = instrument
    if removed:
        require(instrument is None, "Conflicting expectation")
        components["!minecraft:instrument"] = {}
    if opaque:
        components["minecraft:custom_data"] = {"instrument": SING}
    if cooldown:
        components["minecraft:use_cooldown"] = {"seconds": atom("Float", 1.0), "cooldown_group": GROUP}
    return {"id": "minecraft:goat_horn", "count": atom("Int", 1),
            **({"components": components} if components else {})}


def expected(retained: bool) -> tuple[dict[int, dict], dict[int, dict]]:
    chest = {slot: stack(name) for slot, name in enumerate(BUILTINS)}
    chest[8] = stack()
    player = {1: stack(opaque=True), 2: stack(removed=True), 3: stack(SING, cooldown=True), 4: stack(BUILTINS[1])}
    if retained:
        player[0] = chest.pop(6)
    return chest, player


def item_slots(value: object, minimum: int, maximum: int) -> dict[int, dict]:
    result = {}
    for entry in compounds(value):
        entry = deepcopy(entry)
        slot = typed_int(entry.pop("Slot", None), "Byte", minimum, maximum)
        require(slot not in result, f"Duplicate slot {slot}")
        require(set(entry) <= {"id", "count", "components"}, {"unknown_stack_fields": entry})
        require(entry.get("id") == "minecraft:goat_horn", entry)
        typed_int(entry.get("count"), "Int", 1, 1)
        components = entry.get("components", {})
        require(isinstance(components, dict), entry)
        require(not ("minecraft:instrument" in components and "!minecraft:instrument" in components),
                "Conflicting instrument addition/removal")
        # Only codec-equivalent explicit Ponder/default and empty patch normalize.
        if components.get("minecraft:instrument") == PONDER:
            components.pop("minecraft:instrument")
        if not components:
            entry.pop("components", None)
        result[slot] = entry
    return result


def campaign_rows(completed: list[str], errors: list[str]) -> list[dict]:
    passed = completed == list(BOOTS) and not errors
    return [{"id": case, "features": [], "result": "passed" if passed else "failed",
             "completed_boots": completed[:], "errors": errors[:], "composite": "all four boots required"}
            for case in CASES]


def bounded(action: Callable) -> Callable:
    @wraps(action)
    def guarded(fixture: Fixture, *args, **kwargs):
        with interaction_deadline(fixture.server, action.__name__):
            return action(fixture, *args, **kwargs)
    return guarded


def wire_stack(value: dict | None) -> dict | None:
    if value is None:
        return None
    components = deepcopy(value.get("components", {}))
    removed = components.pop("!minecraft:instrument", None) is not None
    if "minecraft:use_cooldown" in components:
        components["minecraft:use_cooldown"]["seconds"] = components["minecraft:use_cooldown"]["seconds"]["value"]
    return {"item_id": 1504, "count": 1, **({"components": components} if components else {}),
            **({"removed_components": ["minecraft:instrument"]} if removed else {})}


def menu_slots(retained: bool) -> list:
    chest, player = expected(retained)
    slots = [None] * 63
    for slot, value in chest.items():
        slots[slot] = wire_stack(value)
    for slot, value in player.items():
        slots[54 + slot] = wire_stack(value)
    return slots


def menu_semantics(slots: list) -> list:
    result = [deepcopy(value) for value in slots]
    for value in result:
        if value is not None and value.get("item_id") == 1504:
            components = value.get("components", {})
            require(not ("minecraft:instrument" in components and "minecraft:instrument" in value.get("removed_components", [])),
                    "Conflicting wire instrument addition/removal")
            if components.get("minecraft:instrument") == PONDER:
                components.pop("minecraft:instrument")
                if not components:
                    value.pop("components", None)
    return result


def read_chest(world: Path) -> dict:
    """Read exact chunk256,0 using existing Anvil/NBT bounds, preserving tag types."""
    directories = [p for p in (world / "region", world / "dimensions/minecraft/overworld/region") if p.is_dir()]
    require(len(directories) == 1, "Expected one overworld region directory")
    region = directories[0] / "r.8.0.mca"
    with region.open("rb") as source:
        header = source.read(worlds.SECTOR_BYTES * 2)
        require(len(header) == worlds.SECTOR_BYTES * 2, "Truncated Anvil header")
        location = int.from_bytes(header[:4], "big")
        sector, count = location >> 8, location & 255
        require(sector >= 2 and count > 0 and (sector + count) * worlds.SECTOR_BYTES <= region.stat().st_size, "Invalid chunk allocation")
        source.seek(sector * worlds.SECTOR_BYTES)
        length = int.from_bytes(source.read(4), "big")
        require(2 <= length <= min(count * worlds.SECTOR_BYTES - 4, worlds.MAX_NBT_BYTES), "Invalid chunk length")
        record = source.read(length)
        require(len(record) == length, "Truncated chunk")
    raw = worlds.decompress_nbt(record[1:], record[0])
    reader = PlayerReader(raw)
    require(reader.number("B") == 10, "Expected compound root")
    reader.string()
    chunk = reader.value(10)
    require(reader.offset == len(raw), "Trailing chunk bytes")
    require(chunk.get("DataVersion") == atom("Int", 5023) and chunk.get("xPos") == atom("Int", 256)
            and chunk.get("zPos") == atom("Int", 0), "Wrong chunk identity/version")
    entries = [be for be in compounds(chunk["block_entities"])
               if tuple(be.get(axis) for axis in ("x", "y", "z")) == tuple(atom("Int", n) for n in CHEST)]
    require(len(entries) == 1 and entries[0].get("id") == "minecraft:chest", "Missing/duplicate/wrong chest")
    return {"path": str(region), "sha256": digest(region), "raw_nbt_sha256": hashlib.sha256(raw).hexdigest(), "typed": entries[0]}


def saved(fixture: Fixture, role: str, retained: bool, used: int) -> dict:
    world = fixture.server.directory / "world"
    player, chest, stats = read_player(world), read_chest(world), read_statistics(world)
    evidence = {"player": player, "chest": chest, "statistics": stats}
    artifact = fixture.server.directory / f"{role}-stopped.json"
    write_json(artifact, evidence)
    require(player["typed"].get("UUID") == {"nbt_type": "IntArray", "value": list(UUID_INTS)}, "UUID changed")
    require(player["typed"].get("DataVersion") == atom("Int", 5023), "Wrong player version")
    require(player["typed"].get("SelectedItemSlot") == atom("Int", 0), "Selected slot changed")
    chest_expected, player_expected = expected(retained)
    require(item_slots(chest["typed"]["Items"], 0, 26) == chest_expected, "Stopped chest differs")
    require(item_slots(player["typed"]["Inventory"], -106, 35) == player_expected, "Stopped inventory differs")
    require(stats["data"]["stats"].get("minecraft:used", {}).get("minecraft:goat_horn", 0) == used, "Stopped Used differs")
    return {"artifact": str(artifact), "sha256": digest(artifact), "used": used}


def live(server, retained: bool) -> dict:
    chest, player = expected(retained)
    entries = lambda mapping: [{"Slot": atom("Byte", slot), **value} for slot, value in mapping.items()]
    observations = []
    for target, key, mapping in ((f"block {' '.join(map(str, CHEST))}", "Items", chest), (f"entity {NAME}", "Inventory", player)):
        observations.append(command_result(server, f"data get {target} {key}", len(mapping)))
        for entry in entries(mapping):
            observations.append(typed_predicate(server, f"if data {target} {{{key}:[{typed_snbt(entry)}]}}"))
    return {"typed_predicates": observations}


def live_active_stack(server, slot: int, value: dict) -> dict:
    """Count fields as well as matching typed values, so expiry cannot add a patch."""
    if slot == -106:
        # 26.3 saves offhand equipment separately from slotted Inventory entries.
        path, expected = "equipment.offhand", {"equipment": {"offhand": value}}
        count = len(value)
    else:
        require(type(slot) is int and 0 <= slot <= 8, "Expected a hotbar slot or offhand")
        path = f'Inventory[{{Slot:{slot}b}}]'
        expected = {"Inventory": {"nbt_type": "List", "element_type": 10,
                                  "value": [{"Slot": atom("Byte", slot), **value}]}}
        count = len(value) + 1
    predicate = typed_predicate(server, f'if data entity {NAME} {typed_snbt(expected)}')
    fields = command_result(server, f"data get entity {NAME} {path}", count)
    components = value.get("components")
    if components:
        patch = command_result(server, f"data get entity {NAME} {path}.components", len(components))
    else:
        patch = typed_predicate(server, f"unless data entity {NAME} {path}.components")
    return {"predicate": predicate, "field_count": fields, "patch": patch, "nbt_path": path}


def fence(server, clients: list[Client]) -> dict:
    marker = "HORN_" + uuid.uuid4().hex
    output = server.command("say " + marker)
    return {"output": output, "received": [delivered_chat(c.wait_state(lambda s: delivered_chat(s, marker) is not None), marker) for c in clients]}


def statistics(client: Client, contract: dict) -> dict:
    key = (contract["used_category"], contract["horn_item"])
    client.watch_statistic(*key)
    request = client.request("request_stats")
    state = client.wait_state(lambda s: s.get("statistics_packet") is not None and s["statistics_packet"]["sequence"] > request["after_sequence"])
    packet = state["statistics_packet"]
    value = client.statistics_values[key]
    return {"request": request, "packet": packet, "used": value["entry"]["value"] if value else 0,
            "value_observation": value,
            "scope": "Connection-local absolute updates; omitted unchanged counters retain their last raw witness."}


def open_menu(client: Client) -> dict:
    close_menu(client)
    before = client.state()["packets"]
    client.request("use_item_on", **dict(zip(("x", "y", "z"), CHEST)))
    state = client.wait_state(lambda s: s.get("menu") is not None and s["menu"]["opened_sequence"] > before and len(s["menu"]["slots"]) == 63)
    require(state["menu"]["menu_type"] == 2 and state["menu"]["carried"] is None, "Wrong menu/cursor")
    return state["menu"]


@bounded
def transfer(fixture: Fixture, client: Client, slot: int) -> dict:
    """Retain actual replies before reopening; full resync means no hash acceptance proof."""
    before = open_menu(client)
    predicted = deepcopy(before["slots"])
    predicted[slot], predicted[54] = predicted[54], predicted[slot]
    mark = client.state()["packets"]
    sent = client.request("swap", slot=slot, hotbar=0)
    processed = statistics(client, fixture.report["contract"])
    replies = [p for p in client.state()["menu_packets"] if p["sequence"] > mark and p["container_id"] == before["container_id"]]
    # A successful action alone is insufficient: Java also applies clicks with
    # bad predictions, then sends a corrective full-content packet.
    require(not any(p["kind"] == "content" for p in replies), {"prediction_resync": replies})
    authoritative = open_menu(client)
    require(authoritative["slots"] == predicted, {"predicted": predicted, "authoritative": authoritative})
    close_menu(client)
    return {"sent": sent, "before": before, "immediate_replies": replies, "processing_stats": processed,
            "authoritative_reopen": authoritative,
            "hash_claim": "Acceptance inferred from no correction after same-stream stats fence, authoritative reopen and following gameplay; no direct Java numeric hash"}


def swap_hands(server, client: Client) -> dict:
    client.request("swap_hands")
    return fence(server, [client])


def horn_sounds(state: dict, mark: int, contract: dict) -> list[dict]:
    """Retain both transports; coordinate119 is a failure, never a substitute118."""
    return sorted([p for p in state["horn_entity_sounds"] if p["sequence"] > mark] +
                  [p for p in state["sounds"] if p["sequence"] > mark and p["registry_id"] in contract["sound_ids"].values()],
                  key=lambda p: p["sequence"])


def sound_binding(client: Client, instrument: str, contract: dict) -> dict:
    registry = client.state()["horn_registry"]
    entries = [entry for entry in registry["entries"] if entry["identifier"] == instrument]
    require(len(entries) == 1, "Instrument absent/duplicate in this connection's registry")
    entry = entries[0]
    sound = entry["data"]["sound_event"]
    require(sound in contract["sound_registry"], "Unknown captured instrument sound")
    return {"client": client.name, "instrument_reference": entry["reference_id"], "instrument_registry_id": entry["registry_id"],
            "sound_name": sound, "sound_registry_id": contract["sound_registry"][sound], "configuration_sha256": registry["payload_sha256"]}


def validate_effects(before: int, after: int, sounds: list[dict], owner_sounds: list[dict],
                     cooldowns: list[dict], sound_id: int | None, group: str, owner_entity_id: int) -> None:
    require(type(owner_entity_id) is int and 0 <= owner_entity_id <= 2147483647, "Missing/invalid owner entity ID")
    require(after - before == (0 if sound_id is None else 1), {"used_before": before, "used_after": after})
    if sound_id is None:
        require(not sounds and not owner_sounds and not cooldowns, {"sounds": sounds, "owner_sounds": owner_sounds, "cooldowns": cooldowns})
    else:
        require(len(sounds) == 1 and sounds[0]["packet_id"] == 118 and sounds[0]["registry_id"] == sound_id
                and sounds[0].get("entity_id") == owner_entity_id
                and sounds[0]["source"] == 2 and sounds[0]["volume"] == 16 and sounds[0]["pitch"] == 1,
                {"underlying_gap": "missing, duplicate or wrong entity sound118", "expected_sound_id": sound_id,
                 "expected_source_entity_id": owner_entity_id, "sounds": sounds})
        require(not owner_sounds, {"underlying_gap": "instrument player was not excluded", "owner_sounds": owner_sounds})
        require(len(cooldowns) == 1 and cooldowns[0]["group"] == group and cooldowns[0]["ticks"] == 140, cooldowns)


@bounded
def effects(fixture: Fixture, owner: Client, observer: Client, *, hand: int, instrument: str | None,
            group: str = "minecraft:goat_horn", other_slot: int | None = None) -> dict:
    """Two immediate ordinary uses; optionally the second targets another builtin."""
    contract = fixture.report["contract"]
    before = statistics(owner, contract)
    owner_mark, observer_mark = owner.state()["packets"], observer.state()["packets"]
    sent = owner.request("horn_use", hand=hand, **({"other_hotbar": other_slot} if other_slot is not None else {}))
    after = statistics(owner, contract)
    boundary = fence(fixture.server, [owner, observer])
    cooldowns = [p for p in owner.state()["horn_cooldowns"] if p["sequence"] > owner_mark]
    sounds = horn_sounds(observer.state(), observer_mark, contract)
    owner_sounds = horn_sounds(owner.state(), owner_mark, contract)
    bindings = [sound_binding(c, instrument, contract) for c in (owner, observer)] if instrument is not None else []
    owner_login = owner.state()["horn_login_header"]
    observation = {"before": before, "sent": sent, "after": after, "sounds": sounds, "owner_sounds": owner_sounds,
                   "sound_bindings": bindings, "owner_login_header": owner_login, "cooldowns": cooldowns, "boundary": boundary,
                   "entity118": [p for p in sounds + owner_sounds if p["packet_id"] == 118],
                   "coordinate119": [p for p in sounds + owner_sounds if p["packet_id"] == 119]}
    # Retain the failing observation too, before any composite assertion raises.
    fixture.report.setdefault("effect_observations", []).append(observation)
    if bindings:
        require(bindings[0]["sound_registry_id"] == bindings[1]["sound_registry_id"], bindings)
    validate_effects(before["used"], after["used"], sounds, owner_sounds, cooldowns,
                     bindings[0]["sound_registry_id"] if bindings else None, group, owner_login["entity_id"])
    return observation


def validate_expiry_cooldowns(cooldowns: list[dict], group: str) -> None:
    expected = 20 if group == GROUP else 0
    require(all(p["group"] == group and p["ticks"] >= 0 for p in cooldowns), cooldowns)
    applied = [p["ticks"] for p in cooldowns if p["ticks"] != 0]
    require(applied == ([expected] if expected else []), {"expiry_component_cooldown": cooldowns})


@bounded
def expire(fixture: Fixture, owner: Client, observer: Client, used: int,
           group: str = "minecraft:goat_horn") -> dict:
    """Keep the active hand/slot untouched through actual game ticks, then check expiry."""
    mark, sound_mark = owner.state()["packets"], observer.state()["packets"]
    ticks = step_exact(fixture.server, 141)
    after = statistics(owner, fixture.report["contract"])
    boundary = fence(fixture.server, [owner, observer])
    require(after["used"] == used, "Expiry awarded duplicate Used")
    cooldowns = [p for p in owner.state()["horn_cooldowns"] if p["sequence"] > mark]
    validate_expiry_cooldowns(cooldowns, group)
    sounds = horn_sounds(observer.state(), sound_mark, fixture.report["contract"])
    sounds += horn_sounds(owner.state(), mark, fixture.report["contract"])
    require(not sounds, {"expiry_duplicate_sound": sounds})
    return {"ticks": ticks, "statistics": after, "cooldowns": cooldowns, "boundary": boundary}


def exercise(fixture: Fixture, owner: Client, observer: Client, *, full: bool, retained: bool,
             initial_used: int, detail: dict) -> dict:
    server = fixture.server
    detail.update(identity=identity_witness(server, owner), initial_live=live(server, retained), actions=[])
    detail["initial_menu"] = open_menu(owner)
    require(menu_semantics(detail["initial_menu"]["slots"]) == menu_slots(retained), "Received initial horn contents differ from independent expectations")
    if full:
        detail["builtin_items"] = [{"chest_slot": slot, "effective_instrument": name,
            "patch_reference_present": "minecraft:instrument" in (detail["initial_menu"]["slots"][slot] or {}).get("components", {}),
            "binding": sound_binding(owner, name, fixture.report["contract"])} for slot, name in enumerate(BUILTINS)]
        require(all(row["patch_reference_present"] for row in detail["builtin_items"] if row["effective_instrument"] != PONDER),
                "Every nondefault builtin must have an actual received reference patch")
        detail["builtin_item_counts"] = {"effective_identities": 8,
            "nondefault_reference_patches": 7,
            "received_reference_patches": sum(row["patch_reference_present"] for row in detail["builtin_items"])}
    close_menu(owner)
    actions = detail["actions"]
    def use(name: str, hand: int = 0, group: str = "minecraft:goat_horn"):
        result = effects(fixture, owner, observer, hand=hand, instrument=name, group=group)
        actions.append(result)
        actions.append(expire(fixture, owner, observer, result["after"]["used"], group))
        # Authoritative live item/count/component immediately after active expiry.
        selected = -106 if hand else (3 if group == GROUP else 0)
        expected_stack = stack(name, cooldown=group == GROUP)
        actions.append(live_active_stack(server, selected, expected_stack))
        if group == GROUP:
            # Vanilla applies the explicit seconds component after finishUsingItem,
            # in addition to the instrument's 140-tick cooldown at use start.
            cooldown_ticks = step_exact(server, 20)
            after_cooldown = statistics(owner, fixture.report["contract"])
            require(after_cooldown["used"] == result["after"]["used"], "Cooldown wait changed Used")
            actions.append({"finish_component_cooldown_wait": cooldown_ticks,
                            "statistics": after_cooldown, "boundary": fence(server, [owner, observer])})
    if full:
        # Each distinct reference crosses actual predicted clicks in both engines.
        for slot, name in enumerate(BUILTINS):
            actions.append(transfer(fixture, owner, slot))
            if name == SING:
                use(SING)
                actions.append(swap_hands(server, owner))
                use(SING, 1)
                actions.append(swap_hands(server, owner))
            actions.append(transfer(fixture, owner, slot))
        actions.append(transfer(fixture, owner, 8))
        use(PONDER)
        actions.append(swap_hands(server, owner))
        use(PONDER, 1)
        actions.append(swap_hands(server, owner))
        # Canonical item group must also block a different builtin reference.
        actions.append(effects(fixture, owner, observer, hand=0, instrument=PONDER, other_slot=4))
        step_exact(server, 141)
        owner.request("horn_select", hotbar=0)
        actions.append(transfer(fixture, owner, 8))
        owner.request("horn_select", hotbar=2)
        actions.append(effects(fixture, owner, observer, hand=0, instrument=None))
        owner.request("horn_select", hotbar=1)
        opaque = effects(fixture, owner, observer, hand=0, instrument=PONDER)
        actions.append(opaque)
        actions.append(expire(fixture, owner, observer, opaque["after"]["used"]))
        owner.request("horn_select", hotbar=3)
        use(SING, group=GROUP)
        use(SING, group=GROUP)  # subsequent valid use after held expiry
        owner.request("horn_select", hotbar=0)
        if fixture.kind == "rust":
            actions.append(transfer(fixture, owner, 6))
            retained = True
    else:
        use(SING)
        actions.append(swap_hands(server, owner))
        use(SING, 1)
        actions.append(swap_hands(server, owner))
    detail["final_menu"] = open_menu(owner)
    require(menu_semantics(detail["final_menu"]["slots"]) == menu_slots(retained), "Received final horn contents differ")
    close_menu(owner)
    detail.update(final_live=live(server, retained), used=statistics(owner, fixture.report["contract"])["used"], retained=retained)
    require(detail["used"] == initial_used + (8 if full else 2), "Unexpected Used outside successful ordinary uses")
    return detail


def author(fixture: Fixture, owner: Client) -> dict:
    server = fixture.server
    typed_predicate(server, f"if entity @a[name={NAME},gamemode=survival]")
    command_result(server, f"data get entity {NAME} Inventory", 0)
    commands = [f"setblock {' '.join(map(str, CHEST))} chest"]
    commands += [f"item replace block {' '.join(map(str, CHEST))} container.{slot} with minecraft:goat_horn[minecraft:instrument='{name}']" for slot, name in enumerate(BUILTINS)]
    commands += [f"item replace block {' '.join(map(str, CHEST))} container.8 with minecraft:goat_horn",
        f'item replace entity {NAME} hotbar.1 with minecraft:goat_horn[minecraft:custom_data={{instrument:"{SING}"}}]',
        f"item replace entity {NAME} hotbar.2 with minecraft:goat_horn[!minecraft:instrument]",
        f'item replace entity {NAME} hotbar.3 with minecraft:goat_horn[minecraft:instrument="{SING}",minecraft:use_cooldown={{seconds:1.0f,cooldown_group:"{GROUP}"}}]',
        f'item replace entity {NAME} hotbar.4 with minecraft:goat_horn[minecraft:instrument="{BUILTINS[1]}"]']
    return {"commands": [command_result(server, command, 1, mode="success") for command in commands], "live": live(server, False)}


def run_campaign(args: argparse.Namespace, run: Path, report: dict) -> None:
    fixtures: list[Fixture] = []
    completed, errors, immutable = [], [], []
    report["boots"] = {}
    previous = None
    retained = False
    previous_used = 0
    try:
        for index, role in enumerate(BOOTS):
            kind = "rust" if role.startswith("rust") else "vanilla"
            if role == "rust_reload":
                fixture = fixtures[-1]
            else:
                fixture = Fixture(kind, run / role, args, report)
                fixtures.append(fixture)
                fixture.server.configure()
                if previous is not None:
                    target = fixture.server.directory / "world"
                    scaffold = world_hashes(target)
                    # Only the just-created, unstarted owned datapack scaffold.
                    shutil.rmtree(target)
                    transfer_evidence = {"discarded_unstarted_scaffold": scaffold}
                    clone_world(previous, target, transfer_evidence)
                    immutable.append((previous, world_hashes(previous)))
                    report.setdefault("world_transfers", []).append(transfer_evidence)
            entry = report["boots"][role] = {"engine": kind, "directory": str(fixture.server.directory), "started": now(), "result": "failed"}
            old_pid = fixture.server.process.pid if fixture.server.process is not None else None
            fixture.start(role)
            require(fixture.server.process.pid != old_pid, "Restart must create a fresh process")
            server = fixture.server
            entry["status"] = protocol.status_fields(server.port)
            server.command("tick freeze")
            require_frozen(server)
            if index == 0:
                server.command("forceload add 4096 0")
                suite.wait_loaded(server, x=4096, z=0)
                for command in ("fill 4096 199 0 4111 199 15 stone", "difficulty peaceful", "scoreboard objectives add pdprobe dummy"):
                    server.command(command)
            clients = []
            for name in (NAME, OBSERVER):
                client = Client(server, name=name, log_name=f"{role}-{name}", horns=True, raw_packets=True)
                fixture.clients.append(client)
                clients.append(client)
                client.ready()
                client.wait_state(lambda s: s["chunks"] >= 49, timeout=60)
                require(client.state().get("horn_registry") is not None, "Missing actual sent instrument registry")
                require(client.state().get("horn_login_header") is not None, "Missing actual sent Login entity ID")
                identity = uuid.UUID(bytes=hashlib.md5(("OfflinePlayer:" + name).encode(), usedforsecurity=False).digest(), version=3)
                ints = [int.from_bytes(identity.bytes[start:start + 4], "big", signed=True) for start in range(0, 16, 4)]
                typed_predicate(server, f'if entity @a[name={name},nbt={{UUID:[I;{",".join(map(str, ints))}]}}]')
            owner, observer = clients
            entry["clients"] = [{"name": c.name, "pid": c.process.pid, "argv": c.process.args,
                                 "registry": c.state()["horn_registry"], "login_header": c.state()["horn_login_header"]} for c in clients]
            require(len({c["login_header"]["entity_id"] for c in entry["clients"]}) == 2, "Owner/observer entity IDs must differ")
            # Every connection captures its own IDs; even two clients may not borrow mappings.
            teleport_exact(server, owner, POS)
            server.command(f"tp {OBSERVER} 4099.5 200 3.5")
            observer.wait_state(lambda s: s.get("server_position") is not None and
                all(abs(s["server_position"][axis] - value) < .01 for axis, value in (("x", 4099.5), ("y", 200), ("z", 3.5))))
            entry["recipient_scope"] = {"owner": NAME, "observer": OBSERVER,
                "owner_position": owner.state()["server_position"], "observer_position": observer.state()["server_position"],
                "expected_distance_squared": 4, "radius": 256,
                "authoritative_positions": [typed_predicate(server,
                    f'if entity @a[name={name},nbt={{Dimension:"minecraft:overworld",Pos:[{x}d,200.0d,3.5d]}}]')
                    for name, x in ((NAME, 4097.5), (OBSERVER, 4099.5))],
                "claim": "One in-range observer and performer exclusion; no boundary or different-dimension runtime claim."}
            entry["initial_statistics"] = statistics(owner, report["contract"])
            require(entry["initial_statistics"]["used"] == previous_used, "Used statistic did not continue across world/restart")
            if index == 0:
                with interaction_deadline(server, role + "_author"):
                    entry["author"] = author(fixture, owner)
            # Whole exercises contain multiple bounded actions; never enlarge the
            # existing 20s per-interaction watchdog to cover this entire campaign.
            entry["exercise"] = {}
            exercise(fixture, owner, observer, full=index < 2, retained=retained,
                     initial_used=previous_used, detail=entry["exercise"])
            retained = entry["exercise"]["retained"]
            previous_used = entry["exercise"]["used"]
            for client in clients:
                fixture.close(client)
            wait_predicate(server, "unless entity @a")
            server.save_flush()
            fixture.stop()
            entry["saved"] = saved(fixture, role, retained, entry["exercise"]["used"])
            entry.update(result="passed", finished=now())
            completed.append(role)
            previous = server.directory / "world"
        for path, hashes in immutable:
            require(world_hashes(path) == hashes, f"Transferred source changed: {path}")
    except Exception as error:
        errors.append(f"{type(error).__name__}: {error}")
    finally:
        for fixture in reversed(fixtures):
            errors.extend(fixture.cleanup())
        report["client_artifacts"] = [{"path": str(path), "sha256": digest(path), "bytes": path.stat().st_size}
            for fixture in fixtures for path in sorted(fixture.server.directory.glob("*-FerraloomTest.jsonl")) + sorted(fixture.server.directory.glob("*-HornObserver.jsonl"))]
        alive = [p.pid for f in fixtures for p in (f.server.process, *(c.process for c in f.clients)) if p is not None and p.poll() is None]
        if alive:
            errors.append(f"Owned processes still alive: {alive}")
        report["completed_boots"], report["runtime_errors"] = completed, errors
        report["cases"] = campaign_rows(completed, errors)
    require(not errors and completed == list(BOOTS), report["runtime_errors"])


def manifest() -> dict:
    return {**inherited_manifest(), **{path: digest(oracle.ROOT / path) for path in
        ("tools/check_goat_horns.py", "tools/check_item_interchange.py", "tests/test_goat_horns.py")}}


def primary_contract(args: argparse.Namespace) -> dict:
    evidence = {name: verify_artifacts(args.primary_root / Path(name).parent,
                {Path(name).name: pin})[Path(name).name] for name, pin in PRIMARY.items()}
    reports = verify_artifacts(args.reference_cache / "generated/reports", REPORT_PINS)
    registries = json.loads(read_bytes(args.reference_cache / "generated/reports/registries.json"), object_pairs_hook=unique_object)
    entries = lambda name: registries["minecraft:" + name]["entries"]
    # Instrument order is deliberately absent here; only received configuration IDs count.
    sound_names = dict(zip((PONDER, SING, BUILTINS[5], BUILTINS[3], BUILTINS[0], BUILTINS[1], BUILTINS[7], BUILTINS[2]), range(8)))
    sound_ids = {name: entries("sound_event")[f"minecraft:item.goat_horn.sound.{index}"]["protocol_id"] for name, index in sound_names.items()}
    return {"primary": evidence, "reports": reports,
        "horn_item": entries("item")["minecraft:goat_horn"]["protocol_id"],
        "used_category": entries("stat_type")["minecraft:used"]["protocol_id"],
        "sound_ids": sound_ids,
        "sound_registry": {f"minecraft:item.goat_horn.sound.{index}": sound_ids[name] for name, index in sound_names.items()},
        "sound_mapping": "Actual captured instrument sound_event names resolve through the pinned static BuiltInRegistries.SOUND_EVENT report; no dynamic sound-event configuration packet is assumed."}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--current-clean-commit", required=True)
    parser.add_argument("--rust-source-commit", required=True)
    parser.add_argument("--rust-binary", type=Path, required=True)
    parser.add_argument("--java", required=True)
    parser.add_argument("--reference-cache", type=Path, required=True)
    parser.add_argument("--primary-root", type=Path, required=True)
    parser.add_argument("--accept-eula", action="store_true")
    parser.add_argument("--workers", type=int, choices=(2,), default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(args.accept_eula, "Explicit isolated reference EULA acceptance required")
    require(args.root.resolve() == oracle.ROOT.resolve(), "--root must be this harness checkout")
    require(0 < args.startup_timeout <= 180 and all(re.fullmatch(r"[0-9a-f]{40}", v) for v in (args.current_clean_commit, args.rust_source_commit)), "Invalid startup bound/commit")
    cache = (oracle.ROOT / ".cache").resolve()
    require(args.output.resolve().is_relative_to(cache) and re.fullmatch(r"goat-horns-[A-Za-z0-9_-]+\.json", args.output.name), "Use fresh .cache/goat-horns-*.json")
    require(not args.output.is_symlink() and not any(p.is_symlink() for p in args.output.absolute().parents if p.is_relative_to(cache)), "Linked output refused")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output = args.output.open("x")
    runs = cache / "runs"
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="goat-horns-", dir=runs))
    report = {"schema_version": 1, "kind": "goat-horns", "result": "failed", "created_at": now(),
        "scope": __doc__, "target": {"version": "26.3", "protocol": 777, "data_version": 5023},
        "environment": {"host": socket.gethostname(), "checkout": str(oracle.ROOT), "fixture": str(run),
            "binding": "127.0.0.1 isolated", "workers": 2, "startup_seconds": args.startup_timeout, "interaction_seconds": 20},
        "identities": {"owner": NAME, "owner_offline_uuid": str(PLAYER_ID), "observer": OBSERVER,
            "observer_offline_uuid": str(uuid.UUID(bytes=hashlib.md5(("OfflinePlayer:" + OBSERVER).encode(), usedforsecurity=False).digest(), version=3))},
        "gaps": GAPS, "processes": {}, "cases": campaign_rows([], ["not executed"])}
    began = time.monotonic()
    try:
        report["source"] = suite.git_state()
        require(report["source"] == {"commit": args.current_clean_commit, "dirty": False}, "Requires exact current clean commit")
        report["manifest"] = manifest()
        oracle.CACHE = args.reference_cache.resolve()
        oracle.acquire = suite.acquire = cached_reference
        official = cached_reference()
        require(digest(oracle.CACHE / "minecraft.jar") == INNER_JAR_SHA256, "Changed pinned inner JAR")
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        require(digest(Path(args.java)) == JDK_SHA256, "Requires pinned cached JDK25")
        version = subprocess.run([args.java, "-version"], capture_output=True, text=True, check=True, timeout=15)
        report["java"] = {"path": args.java, "sha256": JDK_SHA256, "version": version.stdout + version.stderr}
        report["official"] = {"path": str(official), "sha256": digest(official), "inner_sha256": INNER_JAR_SHA256}
        report["contract"] = primary_contract(args)
        original = args.rust_binary.resolve()
        require(original.is_file() and os.access(original, os.X_OK), "Existing Rust executable required")
        binary_hash = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        require(digest(original) == digest(args.rust_binary) == binary_hash, "Binary changed during pin")
        report["binary"] = {"source": str(original), "pinned": str(args.rust_binary), "sha256": binary_hash,
            "source_commit": args.rust_source_commit, "association": "caller attestation, not embedded provenance"}
        run_campaign(args, run, report)
        require(suite.git_state() == report["source"] and manifest() == report["manifest"], "Source changed during run")
        require(primary_contract(args) == report["contract"], "Primary changed during run")
        require(digest(original) == digest(args.rust_binary) == binary_hash and digest(official) == report["official"]["sha256"]
                and digest(oracle.CACHE / "minecraft.jar") == INNER_JAR_SHA256 and digest(Path(args.java)) == JDK_SHA256, "Runtime inputs changed")
        report["result"] = "passed"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        report["cases"] = campaign_rows(report.get("completed_boots", []), [report["error"]])
    finally:
        report.update(finished=now(), duration_seconds=round(time.monotonic() - began, 3))
        raw = json.dumps(report, indent=2, allow_nan=False) + "\n"
        with (run / "report.json").open("x") as retained:
            retained.write(raw)
        with output:
            output.write(raw)
    print(f"goat horns {report['result']}: {args.output}; artifacts {run}", flush=True)
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
