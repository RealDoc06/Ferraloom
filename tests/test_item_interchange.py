"""Source helper tests, not actual Java/Rust interchange evidence.

Small typed JSON inputs are codec-derived; none is an actual Java save or a
decoded item packet. No server, Java, Node, Cargo, download or child execution.
"""
from copy import deepcopy
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import check_item_interchange as interchange


class ItemSemanticsTests(unittest.TestCase):
    def test_nested_defaults_stop_at_opaque_boundaries(self):
        source = {
            "id": "minecraft:shulker_box", "count": {"nbt_type": "Int", "value": 1},
            "components": {
                "minecraft:container": {"nbt_type": "List", "element_type": 10, "value": [{
                    "slot": {"nbt_type": "Int", "value": 0}, "item": {"id": "minecraft:cobblestone"}}]},
                "minecraft:custom_data": {"components": {}, "nested": {
                    "item": {"id": "opaque", "count": {"nbt_type": "Int", "value": 0}, "components": {}},
                    "keep": {"nbt_type": "IntArray", "value": [1, -2, 3]}}},
            },
        }
        before = deepcopy(source)
        explicit = deepcopy(source)
        explicit["components"]["minecraft:container"]["value"][0]["item"].update(
            count={"nbt_type": "Int", "value": 1}, components={})
        self.assertEqual(interchange.semantic_stack(source), interchange.semantic_stack(explicit))
        self.assertEqual(source, before)
        for defect in ("removed_opaque_empty_compound", "changed_array_width", "unknown_nested_field"):
            broken = deepcopy(explicit)
            opaque = broken["components"]["minecraft:custom_data"]
            if defect == "removed_opaque_empty_compound":
                del opaque["components"]
            elif defect == "changed_array_width":
                opaque["nested"]["keep"]["nbt_type"] = "LongArray"
            else:
                broken["components"]["minecraft:container"]["value"][0]["item"]["extension"] = {}
            with self.subTest(defect=defect):
                self.assertNotEqual(interchange.semantic_stack(source), interchange.semantic_stack(broken))

    def test_effect_flags_unsigned_bytes_and_firework_defaults_are_typed(self):
        source = {"id": "minecraft:potion", "count": {"nbt_type": "Int", "value": 1}, "components": {
            "minecraft:potion_contents": {"custom_effects": {"nbt_type": "List", "element_type": 10, "value": [{
                "id": "minecraft:speed", "amplifier": {"nbt_type": "Byte", "value": -1},
                "show_particles": {"nbt_type": "Byte", "value": 0}}]}}}}
        actual = interchange.semantic_stack(source)
        effect = actual["components"]["minecraft:potion_contents"]["custom_effects"]["value"][0]
        self.assertEqual(effect, {"id": "minecraft:speed", "amplifier": {"nbt_type": "Byte", "value": -1},
            "duration": {"nbt_type": "Int", "value": 0}, "ambient": {"nbt_type": "Byte", "value": 0},
            "show_particles": {"nbt_type": "Byte", "value": 0}, "show_icon": {"nbt_type": "Byte", "value": 0}})
        wrong_icon = deepcopy(actual)
        wrong_icon["components"]["minecraft:potion_contents"]["custom_effects"]["value"][0]["show_icon"]["value"] = 1
        self.assertNotEqual(actual, interchange.semantic_stack(wrong_icon))
        rocket = {"id": "minecraft:firework_rocket", "count": {"nbt_type": "Int", "value": 1},
                  "components": {"minecraft:fireworks": {}}}
        explicit = deepcopy(rocket)
        explicit["components"]["minecraft:fireworks"] = {"flight_duration": {"nbt_type": "Byte", "value": 0},
            "explosions": {"nbt_type": "List", "element_type": 0, "value": []}}
        self.assertEqual(interchange.semantic_stack(rocket), interchange.semantic_stack(explicit))
        for field in ("amplifier", "flight_duration"):
            broken = deepcopy(source if field == "amplifier" else explicit)
            node = (broken["components"]["minecraft:potion_contents"]["custom_effects"]["value"][0]
                    if field == "amplifier" else broken["components"]["minecraft:fireworks"])
            node[field] = {"nbt_type": "Int", "value": 255 if field == "amplifier" else 0}
            with self.subTest(field=field), self.assertRaises(AssertionError):
                interchange.semantic_stack(broken)

    def test_duplicate_slots_and_malformed_typed_values_are_rejected(self):
        valid = {"Inventory": {"nbt_type": "List", "element_type": 10,
                               "value": list(interchange.expected_inventory().values())}}
        for defect in ("duplicate_slot", "wrong_slot_width", "wrong_list_type", "duplicate_nested_slot", "noninteger_count"):
            broken = deepcopy(valid)
            entries = broken["Inventory"]["value"]
            if defect == "duplicate_slot":
                entries[1]["Slot"]["value"] = 0
            elif defect == "wrong_slot_width":
                entries[0]["Slot"]["nbt_type"] = "Int"
            elif defect == "wrong_list_type":
                broken["Inventory"]["element_type"] = 3
            elif defect == "duplicate_nested_slot":
                entries[3]["components"]["minecraft:container"]["value"][1]["slot"]["value"] = 0
            else:
                entries[0]["count"]["value"] = True
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                interchange.inventory_view(broken)


class ArtifactGuardTests(unittest.TestCase):
    def test_primary_hash_traversal_and_link_guards(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory = root / "primary"
            directory.mkdir()
            artifact = directory / "fixture.json"
            artifact.write_bytes(b'{"codec_derived":true}')
            expected_hash = hashlib.sha256(artifact.read_bytes()).hexdigest()
            self.assertEqual(interchange.verify_artifacts(directory, {artifact.name: expected_hash})
                             [artifact.name]["sha256"], expected_hash)
            artifact.write_bytes(b'{"codec_derived":false}')
            with self.assertRaisesRegex(AssertionError, "changed"):
                interchange.verify_artifacts(directory, {artifact.name: expected_hash})
            artifact.unlink()
            outside = root / "outside"
            outside.write_bytes(b'{"codec_derived":true}')
            artifact.symlink_to(outside)
            for name in (artifact.name, "../outside"):
                with self.subTest(name=name), self.assertRaises(AssertionError):
                    interchange.verify_artifacts(directory, {name: expected_hash})

    def test_reports_are_exclusive_and_cannot_target_ledgers_or_linked_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cache = root / ".cache"
            cache.mkdir()
            report = cache / "item-interchange-unit.json"
            with interchange.reserve_output(report, cache) as output:
                output.write("owned")
            with self.assertRaises(FileExistsError):
                interchange.reserve_output(report, cache)
            self.assertEqual(report.read_text(), "owned")
            (cache / "linked").symlink_to(root, target_is_directory=True)
            for refused in (cache / "latest.json", root / "item-interchange-outside.json",
                            cache / "linked/item-interchange-link.json"):
                with self.subTest(path=refused), self.assertRaises(AssertionError):
                    interchange.reserve_output(refused, cache)


if __name__ == "__main__":
    unittest.main()
