"""Focused source-helper tests. No server execution or Java hash observations."""
from copy import deepcopy
from pathlib import Path
from unittest.mock import patch
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import check_goat_horns as horns


def inventory(*entries: dict) -> dict:
    return {"nbt_type": "List", "element_type": 10, "value": list(entries)}


class HornCampaignTests(unittest.TestCase):
    def test_active_offhand_uses_current_equipment_shape_and_keeps_exact_field_checks(self):
        value = horns.stack(horns.SING)
        for slot, path, field_count in ((-106, "equipment.offhand", len(value)),
                                        (3, "Inventory[{Slot:3b}]", len(value) + 1)):
            with self.subTest(slot=slot), patch.object(horns, "typed_predicate") as predicate, \
                    patch.object(horns, "command_result") as result:
                observed = horns.live_active_stack(object(), slot, value)
                self.assertEqual(observed["nbt_path"], path)
                self.assertEqual(result.call_args_list[0].args[1:],
                                 (f"data get entity {horns.NAME} {path}", field_count))
                self.assertEqual(result.call_args_list[1].args[1:],
                                 (f"data get entity {horns.NAME} {path}.components", 1))
                query = predicate.call_args.args[1]
                if slot == -106:
                    self.assertIn('"equipment":{"offhand":', query)
                    self.assertNotIn("Slot", query)
                else:
                    self.assertIn('"Inventory":[{"Slot":3b', query)

    def test_campaign_requires_exact_four_boot_order_and_no_cleanup_errors(self):
        for completed in ([], list(horns.BOOTS[:3]), list(reversed(horns.BOOTS)),
                          [*horns.BOOTS, horns.BOOTS[-1]], [horns.BOOTS[0]] * 4):
            with self.subTest(completed=completed):
                rows = horns.campaign_rows(completed, [])
                self.assertEqual(len(rows), 10)
                self.assertEqual({row["result"] for row in rows}, {"failed"})
        self.assertTrue(all(row["result"] == "failed" for row in horns.campaign_rows(list(horns.BOOTS), ["forced cleanup"])))
        self.assertTrue(all(row["result"] == "passed" for row in horns.campaign_rows(list(horns.BOOTS), [])))

    def test_default_equivalence_keeps_removal_and_opaque_instrument_distinct(self):
        default = {"Slot": horns.atom("Byte", 0), "id": "minecraft:goat_horn", "count": horns.atom("Int", 1)}
        explicit = {**default, "components": {"minecraft:instrument": "minecraft:ponder_goat_horn"}}
        self.assertEqual(horns.item_slots(inventory(default), 0, 8), horns.item_slots(inventory(explicit), 0, 8))
        removal = {**default, "components": {"!minecraft:instrument": {}}}
        opaque = {**default, "components": {"minecraft:custom_data": {"instrument": "minecraft:sing_goat_horn"}}}
        for entry in (removal, opaque):
            before = deepcopy(entry)
            self.assertNotEqual(horns.item_slots(inventory(default), 0, 8), horns.item_slots(inventory(entry), 0, 8))
            self.assertEqual(entry, before)
        self.assertEqual(horns.item_slots(inventory(opaque), 0, 8)[0]["components"], opaque["components"])
        with self.assertRaises(AssertionError):
            horns.item_slots(inventory({**explicit, "components": {**explicit["components"], "!minecraft:instrument": {}}}), 0, 8)

    def test_inventory_rejects_duplicate_truncated_wrong_width_and_wrong_count(self):
        valid = {"Slot": horns.atom("Byte", 0), "id": "minecraft:goat_horn", "count": horns.atom("Int", 1)}
        for entries in ([valid, valid], [{**valid, "Slot": horns.atom("Int", 0)}],
                        [{**valid, "Slot": horns.atom("Byte", 9)}], [{**valid, "count": horns.atom("Int", True)}],
                        [{**valid, "count": horns.atom("Int", 0)}], [{"Slot": horns.atom("Byte", 0)}],
                        [{**valid, "unknown": {}}]):
            with self.subTest(entries=entries), self.assertRaises(AssertionError):
                horns.item_slots(inventory(*entries), 0, 8)
        with self.assertRaises(AssertionError):
            horns.item_slots({"nbt_type": "List", "element_type": 8, "value": [valid]}, 0, 8)

    def test_saved_controls_are_typed_and_selected_reference_is_not_opaque(self):
        chest, player = horns.expected(False)
        self.assertEqual(len(chest), 9)
        self.assertEqual(set(player), {1, 2, 3, 4})
        self.assertEqual(player[1]["components"], {"minecraft:custom_data": {"instrument": "minecraft:sing_goat_horn"}})
        self.assertEqual(player[2]["components"], {"!minecraft:instrument": {}})
        self.assertEqual(player[3]["components"]["minecraft:use_cooldown"],
                         {"seconds": {"nbt_type": "Float", "value": 1.0}, "cooldown_group": "test:horn_group"})
        retained_chest, retained_player = horns.expected(True)
        self.assertNotIn(6, retained_chest)
        self.assertEqual(retained_player[0], chest[6])
        self.assertEqual(sum(len(x) for x in (chest, player)), sum(len(x) for x in (retained_chest, retained_player)))

    def test_live_cooldown_predicate_preserves_float_width_and_rejects_invalid_values(self):
        # Exercise the real campaign control that reached Java setup before the
        # shared predicate encoder rejected its nested cooldown Float.
        _, player = horns.expected(False)
        control = {"Slot": horns.atom("Byte", 3), **player[3]}
        before = deepcopy(control)
        predicate = horns.typed_snbt(control)
        self.assertIn('"seconds":1.0f,"cooldown_group":"test:horn_group"', predicate)
        self.assertNotIn("nbt_type", predicate)
        self.assertEqual(control, before)
        invalid = [
            {"nbt_type": "Float", "value": value}
            for value in (True, 1, "1.0", float("nan"), float("inf"), -float("inf"), 1e40)
        ] + [{"nbt_type": "Float"}, {"nbt_type": "Float", "value": 1.0, "extra": 1}]
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(AssertionError):
                horns.typed_snbt({"seconds": value})

    def test_authoritative_menu_expectation_keeps_chest_and_hotbar_indices_separate(self):
        initial, retained = horns.menu_slots(False), horns.menu_slots(True)
        self.assertEqual(len(initial), 63)
        self.assertEqual(initial[6], {"item_id": 1504, "count": 1, "components": {"minecraft:instrument": "minecraft:sing_goat_horn"}})
        self.assertIsNone(initial[54])
        self.assertIsNone(retained[6])
        self.assertEqual(retained[54], initial[6])
        self.assertEqual(initial[56], {"item_id": 1504, "count": 1, "removed_components": ["minecraft:instrument"]})
        self.assertEqual(initial[55]["components"], {"minecraft:custom_data": {"instrument": "minecraft:sing_goat_horn"}})
        self.assertTrue(all(slot is None for slot in initial[9:54]))

    def test_positive_sound_requires_entity_transport_owner_exclusion_and_single_effects(self):
        # Synthetic validator inputs, not runtime sound IDs or Java observations.
        sound = {"packet_id": 118, "registry_id": 123, "entity_id": 7, "source": 2, "volume": 16, "pitch": 1}
        cooldown = {"group": "minecraft:goat_horn", "ticks": 140}
        horns.validate_effects(5, 6, [sound], [], [cooldown], 123, "minecraft:goat_horn", 7)
        for defect in ("coordinate_transport", "wrong_source", "wrong_pitch", "duplicate_sound", "owner_receives",
                       "duplicate_stat", "duplicate_cooldown", "seconds_duration", "instrument_group", "wrong_entity"):
            sounds, own, cooldowns, after = [deepcopy(sound)], [], [deepcopy(cooldown)], 6
            if defect == "coordinate_transport":
                sounds[0]["packet_id"] = 119
            elif defect == "wrong_source":
                sounds[0]["source"] = 7
            elif defect == "wrong_pitch":
                sounds[0]["pitch"] = 0
            elif defect == "duplicate_sound":
                sounds.append(sound)
            elif defect == "owner_receives":
                own.append(sound)
            elif defect == "duplicate_stat":
                after = 7
            elif defect == "duplicate_cooldown":
                cooldowns.append(cooldown)
            elif defect == "seconds_duration":
                cooldowns[0]["ticks"] = 20
            elif defect == "wrong_entity":
                sounds[0]["entity_id"] = 8
            else:
                cooldowns[0]["group"] = horns.SING
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                horns.validate_effects(5, after, sounds, own, cooldowns, 123, "minecraft:goat_horn", 7)
        horns.validate_effects(5, 5, [], [], [], None, "minecraft:goat_horn", 7)
        with self.assertRaises(AssertionError):
            horns.validate_effects(5, 5, [sound], [], [], None, "minecraft:goat_horn", 7)
        for entity_id in (None, True, -1, 2147483648):
            with self.subTest(entity_id=entity_id), self.assertRaises(AssertionError):
                horns.validate_effects(5, 6, [sound], [], [cooldown], 123, "minecraft:goat_horn", entity_id)

    def test_wire_ponder_normalization_is_narrow_and_never_discards_removal(self):
        explicit = {"item_id": 1504, "count": 1, "components": {"minecraft:instrument": horns.PONDER}}
        removed = {"item_id": 1504, "count": 1, "removed_components": ["minecraft:instrument"]}
        opaque = {"item_id": 1504, "count": 1, "components": {"minecraft:custom_data": {"instrument": horns.PONDER}}}
        other = {**explicit, "item_id": 1}
        original = deepcopy([explicit, removed, opaque, other])
        self.assertEqual(horns.menu_semantics(original), [{"item_id": 1504, "count": 1}, removed, opaque, other])
        self.assertEqual(original, [explicit, removed, opaque, other])
        with self.assertRaises(AssertionError):
            horns.menu_semantics([{**explicit, "removed_components": ["minecraft:instrument"]}])


if __name__ == "__main__":
    unittest.main()
