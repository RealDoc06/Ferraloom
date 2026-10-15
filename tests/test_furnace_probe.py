"""Check acceptance boundaries without running Minecraft or creating world files."""
from __future__ import annotations

import copy
from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from parity import furnaces, worlds
from check_furnaces import paired_results


def text(value: str) -> bytes:
    encoded = value.encode()
    return struct.pack(">H", len(encoded)) + encoded


def named(kind: int, name: str, payload: bytes) -> bytes:
    return bytes([kind]) + text(name) + payload


EXPECTED = {"furnace": {
    "id": "minecraft:furnace", "x": 4097, "y": 200, "z": 1,
    "cooking_time_spent": 37, "cooking_total_time": 200,
    "lit_time_remaining": 1364, "lit_total_time": 1600, "speed_multiplier": 1.0,
    "RecipesUsed": {"minecraft:iron_ingot_from_smelting_raw_iron": 1},
    "Items": [{"Slot": 0, "id": "minecraft:raw_iron", "count": 15,
               "components": {"minecraft:custom_name": '{"text":"Keep me"}'}}],
}}


def snapshot(timer_kind: int = 3) -> dict:
    """Independent byte fixture exercises both plain and typed NBT decoders."""
    item = (named(1, "Slot", b"\0") + named(8, "id", text("minecraft:raw_iron"))
            + named(3, "count", struct.pack(">i", 15))
            + named(10, "components", named(8, "minecraft:custom_name", text('{"text":"Keep me"}')) + b"\0") + b"\0")
    payload = (named(8, "id", text("minecraft:furnace"))
               + named(3, "x", struct.pack(">i", 4097))
               + named(3, "y", struct.pack(">i", 200))
               + named(3, "z", struct.pack(">i", 1))
               + named(timer_kind, "cooking_time_spent", struct.pack(">h" if timer_kind == 2 else ">i", 37))
               + named(3, "cooking_total_time", struct.pack(">i", 200))
               + named(3, "lit_time_remaining", struct.pack(">i", 1364))
               + named(3, "lit_total_time", struct.pack(">i", 1600))
               + named(5, "speed_multiplier", struct.pack(">f", 1.0))
               + named(10, "RecipesUsed", named(3, "minecraft:iron_ingot_from_smelting_raw_iron", struct.pack(">i", 1)) + b"\0")
               + named(9, "Items", b"\x0a" + struct.pack(">i", 1) + item) + b"\0")
    raw = named(10, "", payload)
    reader = furnaces.TypedReader(raw)
    reader.number("B")
    reader.string()
    typed = reader.value(10)
    return {"block_entities": {"furnace": worlds.decode_nbt(raw)}, "typed_block_entities": {"furnace": typed}}


class FurnaceProbeTests(unittest.TestCase):
    def test_paired_observations_require_clean_processes_and_fixture_integrity(self):
        report = {"source_fixture": {"result": "passed"}, "source_integrity_checks": [{"passed": True}], "cases": []}
        for kind in ("vanilla", "rust"):
            for mode, names in (("main", ("zero-step", "after-63", "after-163")),
                                ("lava", ("lava-after-1", "lava-after-200"))):
                report["cases"].append({"runtime": kind, "mode": mode, "exit": {"passed": True},
                                        "phases": [{"name": name, "result": "passed",
                                                    **({"storage_observation": {"raw_chunk_unchanged": False}}
                                                       if name == "zero-step" else {})} for name in names]})
        for entries in paired_results(report).values():
            self.assertEqual([case["result"] for case in entries], ["skipped", "passed", "passed"])
        report["cases"][2]["exit"]["passed"] = False
        results = paired_results(report)
        self.assertEqual([case["result"] for case in results["rust"]], ["failed", "failed", "passed"])
        self.assertEqual([case["result"] for case in results["vanilla"]], ["skipped", "passed", "passed"])
        report["source_integrity_checks"][0]["passed"] = False
        self.assertTrue(all(case["result"] == "failed" for entries in paired_results(report).values() for case in entries))

    def test_missing_continuation_phase_cannot_be_reported_as_a_pass(self):
        report = {"source_fixture": {"result": "passed"}, "source_integrity_checks": [{"passed": True}],
                  "cases": [{"runtime": "rust", "mode": "main", "exit": {"passed": True},
                             "phases": [{"name": "zero-step", "result": "passed",
                                         "storage_observation": {"raw_chunk_unchanged": False}},
                                        {"name": "after-63", "result": "passed"}]}]}
        results = paired_results(report)
        self.assertEqual([case["result"] for case in results["rust"]], ["skipped", "failed", "failed"])
        self.assertTrue(all(case["result"] == "failed" for case in results["vanilla"]))

    def test_zero_step_preservation_is_not_imported_as_an_encoding_pass(self):
        observation = {"raw_chunk_unchanged": True, "scope": "Preservation only"}
        report = {"source_fixture": {"result": "passed"}, "source_integrity_checks": [{"passed": True}],
                  "cases": [{"runtime": "rust", "mode": "main", "exit": {"passed": True},
                             "phases": [{"name": "zero-step", "result": "passed", "storage_observation": observation},
                                        {"name": "after-63", "result": "passed"},
                                        {"name": "after-163", "result": "passed"}]}]}
        entries = paired_results(report)["rust"]
        self.assertEqual([entry["result"] for entry in entries], ["skipped", "passed", "failed"])
        self.assertEqual(entries[0]["detail"]["storage_observations"], {"zero-step": observation})
        self.assertIn("does not directly observe furnace decoding or encoding", entries[0]["detail"]["reason"])
        for marker in ({}, {"raw_chunk_unchanged": None}, {"raw_chunk_unchanged": False}):
            report["cases"][0]["phases"][0]["storage_observation"] = marker
            self.assertEqual(paired_results(report)["rust"][0]["result"], "skipped")

    def test_failed_run_keeps_its_diagnostic_in_every_paired_observation(self):
        report = {"error": "ValueError: Official server checksum mismatch", "cases": [],
                  "source_integrity_checks": []}
        for entries in paired_results(report).values():
            for entry in entries:
                self.assertEqual(entry["result"], "failed")
                self.assertEqual(entry["error"], report["error"])

    def test_numeric_short_is_a_canonical_difference_without_semantic_failure(self):
        self.assertEqual(furnaces.compare_snapshot(snapshot(), EXPECTED)["result"], "passed")
        result = furnaces.compare_snapshot(snapshot(timer_kind=2), EXPECTED)
        self.assertEqual(result["semantic_result"], "passed")
        self.assertEqual(result["canonical_storage_result"], "failed")
        self.assertEqual(result["failures"], [{"fixture": "furnace", "key": "cooking_time_spent",
                                             "expected": "Int", "actual": "Short", "passed": False}])

    def test_item_components_survive_normalization_and_changes_fail(self):
        saved = snapshot()
        components = saved["block_entities"]["furnace"]["Items"][0]["components"]
        self.assertEqual(furnaces.semantic_state(saved["block_entities"]["furnace"])["Items"][0]["components"], components)
        for replacement in ({}, {"minecraft:custom_name": '{"text":"Changed"}'}):
            with self.subTest(replacement=replacement):
                changed = copy.deepcopy(saved)
                changed["block_entities"]["furnace"]["Items"][0]["components"] = replacement
                result = furnaces.compare_snapshot(changed, EXPECTED)
                self.assertEqual(result["semantic_result"], "failed")
                self.assertEqual(result["canonical_storage_result"], "passed")
                self.assertEqual([check["key"] for check in result["failures"]], ["Items"])

    def test_case_sensitive_timer_and_recipe_changes_fail_semantics(self):
        for field in ("cooking_time_spent", "RecipesUsed"):
            with self.subTest(field=field):
                saved = snapshot()
                be = saved["block_entities"]["furnace"]
                be[field.upper()] = be.pop(field)
                result = furnaces.compare_snapshot(saved, EXPECTED)
                self.assertEqual(result["semantic_result"], "failed")
                self.assertEqual([check["key"] for check in result["semantic_checks"] if not check["passed"]], [field])


if __name__ == "__main__":
    unittest.main()
