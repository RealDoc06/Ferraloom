"""Bounded saved-NBT observations and explicit expectations for furnace fixtures."""
from __future__ import annotations

import copy
import hashlib
from pathlib import Path
import time

from parity import suite, worlds

POSITIONS = {
    "furnace": (4097, 200, 1),
    "blast_furnace": (4099, 200, 1),
    "smoker": (4101, 200, 1),
    "wet_sponge_lava_unstarted": (4103, 200, 1),
}
TIMERS = ("cooking_time_spent", "cooking_total_time", "lit_time_remaining", "lit_total_time")
RECIPES = {
    "furnace": "minecraft:iron_ingot_from_smelting_raw_iron",
    "blast_furnace": "minecraft:iron_ingot_from_blasting_raw_iron",
    "smoker": "minecraft:cooked_porkchop_from_smoking",
}


class TypedReader(worlds.NBTReader):
    """Retain numeric wire types without weakening the shared reader's bounds."""
    def value(self, tag: int, depth: int = 0) -> object:
        value = super().value(tag, depth)
        if tag in (1, 2, 3, 4, 5, 6):
            return {"nbt_type": ("Byte", "Short", "Int", "Long", "Float", "Double")[tag - 1], "value": value}
        return value


def digest(path: Path, algorithm: str = "sha256") -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, algorithm).hexdigest()


def read_saved(world: Path) -> dict:
    """Inspect chunk (256, 0), rejecting external, oversized or invalid records."""
    directories = [path for path in (world / "region", world / "dimensions/minecraft/overworld/region") if path.is_dir()]
    if len(directories) != 1:
        raise ValueError("Expected exactly one overworld region directory")
    region = directories[0] / "r.8.0.mca"
    with region.open("rb") as source:
        header = source.read(worlds.SECTOR_BYTES * 2)
        if len(header) != worlds.SECTOR_BYTES * 2:
            raise ValueError("Truncated Anvil header")
        location = int.from_bytes(header[:4], "big")
        sector, count = location >> 8, location & 255
        if sector < 2 or count == 0 or (sector + count) * worlds.SECTOR_BYTES > region.stat().st_size:
            raise ValueError("Invalid fixture chunk allocation")
        source.seek(sector * worlds.SECTOR_BYTES)
        length = int.from_bytes(source.read(4), "big")
        if not 2 <= length <= min(count * worlds.SECTOR_BYTES - 4, worlds.MAX_NBT_BYTES):
            raise ValueError("Invalid or oversized fixture chunk record")
        record = source.read(length)
        if len(record) != length:
            raise ValueError("Truncated fixture chunk record")
    raw = worlds.decompress_nbt(record[1:], record[0])
    chunk = worlds.decode_nbt(raw)
    if (chunk.get("xPos"), chunk.get("zPos")) != (256, 0) or chunk.get("Status") not in ("full", "minecraft:full"):
        raise ValueError("Fixture region does not contain the expected full chunk")
    reader = TypedReader(raw)
    if reader.number("B") != 10:
        raise ValueError("Fixture root is not a compound")
    reader.string()
    typed = reader.value(10)
    if reader.offset != len(raw) or not isinstance(typed, dict):
        raise ValueError("Invalid typed fixture root")
    entities, typed_entities = {}, {}
    for be, typed_be in zip(chunk["block_entities"], typed["block_entities"], strict=True):
        position = tuple(be.get(axis) for axis in ("x", "y", "z"))
        if position in entities:
            raise ValueError("Duplicate block entity position")
        entities[position], typed_entities[position] = be, typed_be
    return {
        "region_path": str(region), "region_sha256": digest(region),
        "raw_chunk_nbt_sha256": hashlib.sha256(raw).hexdigest(),
        "chunk_data_version": chunk.get("DataVersion"),
        "block_entities": {name: entities[pos] for name, pos in POSITIONS.items() if pos in entities},
        "typed_block_entities": {name: typed_entities[pos] for name, pos in POSITIONS.items() if pos in typed_entities},
    }


def inventory(be: dict, input_id: str, inputs: int, fuel_id: str | None = None,
              output_id: str | None = None, outputs: int = 0) -> None:
    be["Items"] = [{"Slot": 0, "id": "minecraft:" + input_id, "count": inputs}]
    if fuel_id:
        be["Items"].append({"Slot": 1, "id": "minecraft:" + fuel_id, "count": 1})
    if output_id:
        be["Items"].append({"Slot": 2, "id": "minecraft:" + output_id, "count": outputs})


def expected_reference() -> dict:
    """Known official 26.3 observations after exactly 237 seeded ticks."""
    expected = {}
    for name, position in POSITIONS.items():
        be = {"id": "minecraft:" + ("furnace" if name.startswith("wet_sponge") else name),
              **dict(zip(("x", "y", "z"), position)), "RecipesUsed": {}}
        if name in RECIPES:
            fast = name != "furnace"
            be.update(dict(zip(TIMERS, (37, 100 if fast else 200, 564 if fast else 1364, 800 if fast else 1600))))
            be["speed_multiplier"] = 2.0 if fast else 1.0
            outputs = 2 if fast else 1
            inventory(be, "porkchop" if name == "smoker" else "raw_iron", 16 - outputs,
                      output_id="cooked_porkchop" if name == "smoker" else "iron_ingot", outputs=outputs)
            be["RecipesUsed"] = {RECIPES[name]: outputs}
        else:
            be.update(dict(zip(TIMERS, (0, 200, 0, 0))))
            be["speed_multiplier"] = 1.0
            inventory(be, "wet_sponge", 16, "lava_bucket")
        expected[name] = be
    return expected


def expected_continuation(source: dict, mode: str, elapsed: int) -> dict:
    """Derive from the validated Java save and literal observations, never Rust."""
    expected = copy.deepcopy(source["block_entities"])
    lava = expected["wet_sponge_lava_unstarted"]
    if mode == "lava":
        if elapsed == 1:
            lava.update(dict(zip(TIMERS, (1, 200, 20000, 20000))))
            inventory(lava, "wet_sponge", 16, "bucket")
        elif elapsed == 200:
            lava.update(dict(zip(TIMERS, (0, 200, 19801, 20000))))
            inventory(lava, "wet_sponge", 15, "water_bucket", "sponge", 1)
            lava["RecipesUsed"] = {"minecraft:sponge": 1}
        else:
            raise ValueError(f"Unexpected lava phase: {elapsed}")
        return {"wet_sponge_lava_unstarted": lava}
    if mode != "main" or elapsed not in (0, 63, 163):
        raise ValueError(f"Unexpected continuation: {mode}/{elapsed}")
    if elapsed == 0:
        return expected
    phases = {
        63: {"furnace": (100, 1301, 15, 1), "blast_furnace": (0, 501, 13, 3), "smoker": (0, 501, 13, 3)},
        163: {"furnace": (0, 1201, 14, 2), "blast_furnace": (0, 401, 12, 4), "smoker": (0, 401, 12, 4)},
    }
    for name, (spent, remaining, inputs, outputs) in phases[elapsed].items():
        expected[name].update(cooking_time_spent=spent, lit_time_remaining=remaining)
        inventory(expected[name], "porkchop" if name == "smoker" else "raw_iron", inputs,
                  output_id="cooked_porkchop" if name == "smoker" else "iron_ingot", outputs=outputs)
        expected[name]["RecipesUsed"] = {RECIPES[name]: outputs}
    lava.update(dict(zip(TIMERS, (elapsed, 200, {63: 19938, 163: 19838}[elapsed], 20000))))
    inventory(lava, "wet_sponge", 16, "bucket")
    return expected


def semantic_state(be: dict) -> dict:
    """Normalize empty components and numeric tag widths, retaining exact keys."""
    return {
        **{key: be.get(key) for key in ("id", "x", "y", "z")},
        **{key: be.get(key, 0) for key in TIMERS}, "speed_multiplier": be.get("speed_multiplier", 1.0),
        "RecipesUsed": be.get("RecipesUsed", {}),
        "Items": sorted(({"Slot": item["Slot"], "id": item["id"], "count": item["count"],
                          "components": item.get("components", {})} for item in be.get("Items", [])), key=lambda item: item["Slot"]),
    }


def compare_snapshot(saved: dict, expected: dict) -> dict:
    semantic, encoding = [], []
    for name, wanted in expected.items():
        actual, typed = saved["block_entities"].get(name), saved["typed_block_entities"].get(name, {})
        if actual is None:
            semantic.append({"fixture": name, "key": "block_entity", "expected": "present", "actual": None, "passed": False})
            continue
        left = semantic_state(actual)
        for key, value in semantic_state(wanted).items():
            semantic.append({"fixture": name, "key": key, "expected": value, "actual": left[key], "passed": left[key] == value})
        fields = [(key, typed.get(key), "Int") for key in TIMERS] + [("speed_multiplier", typed.get("speed_multiplier"), "Float")]
        for item in typed.get("Items", []):
            slot = item.get("Slot", {}).get("value")
            fields += [(f"Items.{slot}.{key}", item.get(key), kind) for key, kind in (("Slot", "Byte"), ("count", "Int"))]
        fields += [(f"RecipesUsed.{recipe}", count, "Int") for recipe, count in typed.get("RecipesUsed", {}).items()]
        for key, value, kind in fields:
            actual_type = value.get("nbt_type") if isinstance(value, dict) else None
            encoding.append({"fixture": name, "key": key, "expected": kind, "actual": actual_type, "passed": actual_type == kind})
    semantic_ok, encoding_ok = all(check["passed"] for check in semantic), all(check["passed"] for check in encoding)
    return {"result": "passed" if semantic_ok and encoding_ok else "failed",
            "semantic_result": "passed" if semantic_ok else "failed",
            "canonical_storage_result": "passed" if encoding_ok else "failed",
            "semantic_checks": semantic, "canonical_storage_checks": encoding,
            "failures": [check for check in semantic + encoding if not check["passed"]]}


def require_frozen(server: suite.Server) -> None:
    output = server.command("tick query")
    if not any(line.endswith("The game is frozen") for line in output):
        raise RuntimeError(f"Server is not frozen: {output}")


def step_exact(server: suite.Server, ticks: int) -> dict:
    require_frozen(server)
    before = suite.game_time(server)
    if ticks:
        output = server.command(f"tick step {ticks}")
        if not any(line.endswith(f"Stepping {ticks} tick(s)") for line in output):
            raise RuntimeError(f"Tick step was not acknowledged: {output}")
    deadline = time.monotonic() + max(30, ticks / 10)
    after = suite.game_time(server)
    while after < before + ticks and time.monotonic() < deadline:
        time.sleep(0.1)
        after = suite.game_time(server)
    require_frozen(server)
    if after != before + ticks:
        raise RuntimeError(f"Expected {ticks} ticks, observed {after - before}: {before} -> {after}")
    return {"before": before, "after": after, "delta": after - before, "frozen_after": True}


def save_frozen(server: suite.Server) -> dict:
    require_frozen(server)
    before = suite.game_time(server)
    server.save_flush()
    require_frozen(server)
    after = suite.game_time(server)
    if after != before:
        raise RuntimeError(f"Game advanced during frozen save: {before} -> {after}")
    return {"before": before, "after": after}
