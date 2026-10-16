"""Source-only helper checks; no server, Java, Node, downloads or build required.

These do not constitute paired player-continuity evidence.
"""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import check_player_data as player_data


class WorldCopyTests(unittest.TestCase):
    def test_changed_source_and_corrupted_copy_are_rejected_with_evidence(self):
        for changed in ("source", "destination"):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as directory:
                source = Path(directory) / "source"
                destination = Path(directory) / "destination"
                source.mkdir()
                (source / "level.dat").write_bytes(b"original")
                real_copy = player_data.shutil.copytree

                def damaged_copy(src, dst):
                    result = real_copy(src, dst)
                    ((source if changed == "source" else destination) / "level.dat").write_bytes(b"changed")
                    return result

                evidence = {}
                with patch.object(player_data.shutil, "copytree", side_effect=damaged_copy):
                    with self.assertRaises(AssertionError):
                        player_data.clone_world(source, destination, evidence)
                self.assertIn("source_before_copy", evidence)
                self.assertIn("source_after_copy", evidence)
                self.assertIn("destination_after_copy", evidence)
                if changed == "destination":
                    self.assertEqual((source / "level.dat").read_bytes(), b"original")

    def test_world_symlink_cannot_copy_an_external_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (root / "external").write_bytes(b"outside fixture")
            (source / "level.dat").symlink_to(root / "external")
            destination = root / "destination"
            with self.assertRaisesRegex(AssertionError, "symlink"):
                player_data.clone_world(source, destination, {})
            self.assertFalse(destination.exists())


class TypedNBTTests(unittest.TestCase):
    def test_empty_list_and_array_widths_survive_projection(self):
        # Handwritten named-root NBT: UUID IntArray, another LongArray and two
        # empty lists with different element tags. A plain JSON projection loses
        # these distinctions and would incorrectly accept a changed save type.
        raw = bytes.fromhex(
            "0a 0000 "
            "0b 0004 55554944 00000004 e7ee076d be5b31df a4143e90 7115f4a3 "
            "0c 0001 4c 00000001 0000000000000001 "
            "09 0001 41 0a 00000000 "
            "09 0001 42 03 00000000 "
            "00"
        )
        reader = player_data.PlayerReader(raw)
        self.assertEqual(reader.number("B"), 10)
        reader.string()
        typed = reader.value(10)
        self.assertEqual(reader.offset, len(raw))
        self.assertEqual(typed["UUID"], {"nbt_type": "IntArray", "value": [-403830931, -1101319713, -1542177136, 1897264291]})
        self.assertEqual(typed["L"], {"nbt_type": "LongArray", "value": [1]})
        self.assertEqual(typed["A"], {"nbt_type": "List", "element_type": 10, "value": []})
        self.assertEqual(typed["B"], {"nbt_type": "List", "element_type": 3, "value": []})


class AdvancementContinuationTests(unittest.TestCase):
    def test_untouched_timestamp_and_new_criterion_are_both_required(self):
        old_time, new_time = "2026-10-16 03:00:00 +0200", "2026-10-16 03:01:00 +0200"
        source = {"path": "source.json", "data": {"DataVersion": 5023,
            player_data.STORY: {"criteria": {"crafting_table": old_time}, "done": True},
            player_data.ADVENTURE: {"criteria": {player_data.PLAIN: old_time,
                player_data.UNTOUCHED_CRITERION: old_time}, "done": False}}}
        destination = deepcopy(source)
        destination["path"] = "destination.json"
        destination["data"][player_data.STORY]["criteria"]["crafting_table"] = new_time
        destination["data"][player_data.ADVENTURE]["criteria"][player_data.PLAIN] = new_time
        added = "minecraft:badlands"
        destination["data"][player_data.ADVENTURE]["criteria"][added] = new_time
        result = player_data.compare_advancements(source, destination, added)
        self.assertEqual(result["preserved_criteria"], [[player_data.ADVENTURE, player_data.UNTOUCHED_CRITERION, old_time]])
        for defect in ("rewritten_original", "missing_mutation"):
            with self.subTest(defect=defect):
                broken = deepcopy(destination)
                criteria = broken["data"][player_data.ADVENTURE]["criteria"]
                if defect == "rewritten_original":
                    criteria[player_data.UNTOUCHED_CRITERION] = new_time
                else:
                    del criteria[added]
                with self.assertRaises(AssertionError):
                    player_data.compare_advancements(source, broken, added)


if __name__ == "__main__":
    unittest.main()
