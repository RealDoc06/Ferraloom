"""Source-only helper checks; no server, Java, Node, downloads or build required.

These do not constitute paired player-continuity evidence.
"""
from copy import deepcopy
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import check_player_data as player_data
from check_container_metadata import failure_labels


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


class EnchantmentSeedTests(unittest.TestCase):
    def test_seed_requires_exact_int_width_and_nonzero_reload_prerequisite(self):
        for value in (-2147483648, -123456789, 123456789, 2147483647):
            saved = {"typed": {"XpSeed": {"nbt_type": "Int", "value": value}}}
            self.assertEqual(player_data.enchantment_seed(saved, nonzero=True), saved["typed"]["XpSeed"])
        zero = {"typed": {"XpSeed": {"nbt_type": "Int", "value": 0}}}
        self.assertEqual(player_data.enchantment_seed(zero), zero["typed"]["XpSeed"])
        with self.assertRaisesRegex(AssertionError, "nonzero"):
            player_data.enchantment_seed(zero, nonzero=True)
        for seed in (None, {"nbt_type": "Long", "value": 17}, {"nbt_type": "Int", "value": True},
                     {"nbt_type": "Int", "value": 17.0}, {"nbt_type": "Int", "value": "17"},
                     {"nbt_type": "Int", "value": -2147483649}, {"nbt_type": "Int", "value": 2147483648}):
            with self.subTest(seed=seed), self.assertRaisesRegex(AssertionError, "invalid_enchantment_seed"):
                player_data.enchantment_seed({"typed": {"XpSeed": seed}})

    def test_reload_witness_requires_initial_zero_and_observed_exact_live_seed(self):
        # Synthetic observations exercise the real helper, never runtime evidence.
        initial = {"typed": {"XpSeed": {"nbt_type": "Int", "value": 0}}}
        reloaded = {"typed": {"XpSeed": {"nbt_type": "Int", "value": -123456789}}}
        server = Mock()
        with patch.object(player_data, "typed_predicate", return_value={"observed": True}) as predicate:
            result = player_data.seed_reload_witness(server, initial, reloaded)
            predicate.assert_called_once_with(server, f"if data entity {player_data.NAME} {{XpSeed:-123456789}}")
            self.assertEqual(result["initial_saved"], initial["typed"]["XpSeed"])
            self.assertEqual(result["reloaded_saved"], reloaded["typed"]["XpSeed"])
            with self.assertRaisesRegex(AssertionError, "initial_seed_must_be_zero"):
                player_data.seed_reload_witness(server, reloaded, reloaded)
            with self.assertRaisesRegex(AssertionError, "nonzero"):
                player_data.seed_reload_witness(server, initial, initial)
            # A separately observed positive seed is equally valid; no fixed RNG expectation.
            positive = {"typed": {"XpSeed": {"nbt_type": "Int", "value": 987654321}}}
            player_data.seed_reload_witness(server, initial, positive)
            predicate.assert_called_with(server, f"if data entity {player_data.NAME} {{XpSeed:987654321}}")
        with patch.object(player_data, "typed_predicate", side_effect=AssertionError("wrong live seed")):
            with self.assertRaisesRegex(AssertionError, "wrong live seed"):
                player_data.seed_reload_witness(server, initial, reloaded)


class ExperienceReferenceTests(unittest.TestCase):
    def test_exact_saved_fraction_total_and_type_are_required(self):
        # Explicit primary values, not a second implementation of XP arithmetic.
        # Only the first fraction has prior stopped-Java-save corroboration.
        for reference, total, progress, bits in (
                (player_data.SOURCE_XP, 223, 0.22580629587173462, "3e6739c4"),
                (player_data.CONTINUED_XP, 227, 0.3548385500907898, "3eb5ad66")):
            data = {"XpLevel": {"nbt_type": "Int", "value": 12},
                    "XpP": {"nbt_type": "Float", "value": progress},
                    "XpTotal": {"nbt_type": "Int", "value": total}}
            self.assertEqual(player_data.validate_experience(data, reference), {"typed": data, "progress_bits": "0x" + bits})
            for defect in ("fraction_one_ulp", "query_floor_as_remainder", "wrong_level", "double_tag"):
                broken = deepcopy(data)
                if defect == "fraction_one_ulp":
                    broken["XpP"]["value"] = struct.unpack(">f", (int(bits, 16) + 1).to_bytes(4, "big"))[0]
                elif defect == "query_floor_as_remainder":
                    broken["XpTotal"]["value"] = total - 1
                elif defect == "wrong_level":
                    broken["XpLevel"]["value"] = 13
                else:
                    broken["XpP"]["nbt_type"] = "Double"
                with self.subTest(total=total, defect=defect), self.assertRaises(AssertionError):
                    player_data.validate_experience(broken, reference)

    def test_live_query_uses_pinned_floor_separately_from_exact_fraction_and_total(self):
        for reference, total, progress, query_points in (
                (player_data.SOURCE_XP, 223, 0.22580629587173462, 6),
                (player_data.CONTINUED_XP, 227, 0.3548385500907898, 10)):
            server, client = Mock(), Mock()
            server.command.return_value = []
            client.wait_state.return_value = {"health": 14, "food": 20, "server_position": player_data.POSITION}

            def observed_predicate(_server, predicate):
                if predicate.startswith("if data entity "):
                    self.assertIn(f"XpLevel:12,XpP:{progress}f,XpTotal:{total},", predicate)
                else:
                    # Exercise the real command_result score predicate against
                    # independent synthetic query results, never server evidence.
                    query = server.command.call_args.args[0].rsplit(" ", 1)[-1]
                    self.assertIn(query, ("levels", "points"))
                    score = 12 if query == "levels" else observed_points
                    self.assertEqual(predicate, f"if score Observation pdprobe matches {score}")
                return {"predicate": predicate}

            with patch.object(player_data, "typed_predicate", side_effect=observed_predicate):
                observed_points = query_points
                result = player_data.vitals_witness(server, client, reference, player_data.POSITION)
                self.assertEqual(result["query_points"]["expected"], query_points)
                observed_points = query_points + 1  # Ideal 7/11 is wrong even with correct XpP and XpTotal.
                with self.subTest(total=total, observed_points=observed_points), self.assertRaises(AssertionError):
                    player_data.vitals_witness(server, client, reference, player_data.POSITION)


class TeleportExpectationTests(unittest.TestCase):
    def test_boundary_keeps_positive_command_and_requires_exact_negative_packet(self):
        server, client = Mock(), Mock()
        client.state.return_value = {"teleport_confirmations": 1}
        expected = {"x": 4099.5, "y": 200.0, "z": 3.5, "yaw": -180.0, "pitch": -10.0}

        def received(predicate):
            # Synthetic predicate inputs exercise the helper, not a real client.
            self.assertFalse(predicate({"teleport_confirmations": 1, "server_position": expected}))
            for axis, wrong in (("yaw", 180.0), ("yaw", -540.0), ("pitch", 10.0), ("x", 4099.0)):
                self.assertFalse(predicate({"teleport_confirmations": 2,
                                            "server_position": {**expected, axis: wrong}}))
            state = {"teleport_confirmations": 2, "server_position": expected}
            self.assertTrue(predicate(state))
            return state

        client.wait_state.side_effect = received
        result = player_data.teleport_exact(server, client, player_data.CONTINUED_TELEPORT,
                                            expected_server_rotation=(-180.0, -10.0))
        command = f"tp {player_data.NAME} 4099.5 200.0 3.5 180.0 -10.0"
        server.command.assert_called_once_with(command)
        self.assertEqual(result["command"], command)
        self.assertEqual(result["received_position"], expected)
        self.assertEqual(result["expected_position"], expected)
        self.assertEqual(player_data.CONTINUED_POSITION, expected)

    def test_unchanged_ninety_degree_control_uses_exact_default_expectation(self):
        server, client = Mock(), Mock()
        client.state.return_value = {"teleport_confirmations": 1}

        def received(predicate):
            state = {"teleport_confirmations": 2, "server_position": player_data.POSITION}
            self.assertTrue(predicate(state))
            self.assertFalse(predicate({**state, "server_position": {**player_data.POSITION, "yaw": 450.0}}))
            return state

        client.wait_state.side_effect = received
        result = player_data.teleport_exact(server, client, player_data.POSITION)
        server.command.assert_called_once_with(f"tp {player_data.NAME} 4097.5 200.0 3.5 90.0 15.0")
        self.assertEqual(result["expected_position"], player_data.POSITION)


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
        result = player_data.compare_advancements(source, destination, added, destination_kind="rust")
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
                    player_data.compare_advancements(source, broken, added, destination_kind="rust")

    def test_timezone_rendering_preserves_instants_but_rust_must_preserve_text(self):
        utc, local = "2026-10-16 01:00:00 +0000", "2026-10-16 03:00:00 +0200"
        source = {"path": "source.json", "data": {"DataVersion": 5023,
            player_data.STORY: {"criteria": {"crafting_table": utc}, "done": True},
            player_data.ADVENTURE: {"criteria": {player_data.PLAIN: utc,
                player_data.UNTOUCHED_CRITERION: utc}, "done": False}}}
        destination = deepcopy(source)
        destination["path"] = "destination.json"
        destination["data"][player_data.STORY]["criteria"]["crafting_table"] = local
        criteria = destination["data"][player_data.ADVENTURE]["criteria"]
        criteria[player_data.PLAIN] = local
        criteria[player_data.UNTOUCHED_CRITERION] = local
        added = "minecraft:badlands"
        criteria[added] = local
        self.assertEqual(player_data.compare_advancements(source, destination, added, destination_kind="vanilla")
                         ["timestamp_preservation"], "instant")
        with self.assertRaisesRegex(AssertionError, "rewritten_criterion_text"):
            player_data.compare_advancements(source, destination, added, destination_kind="rust")
        criteria[player_data.UNTOUCHED_CRITERION] = utc
        self.assertEqual(player_data.compare_advancements(source, destination, added, destination_kind="rust")
                         ["timestamp_preservation"], "instant_and_exact_text")
        criteria[player_data.UNTOUCHED_CRITERION] = "2026-10-16 03:00:01 +0200"
        for kind in ("vanilla", "rust"):
            with self.subTest(kind=kind), self.assertRaisesRegex(AssertionError, "changed_criterion_instant"):
                player_data.compare_advancements(source, destination, added, destination_kind=kind)


class StatisticsContinuityTests(unittest.TestCase):
    def test_json_requires_both_continued_counts_not_an_unchanged_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            world = Path(directory)
            path = world / "players/stats" / f"{player_data.PLAYER_ID}.json"
            path.parent.mkdir(parents=True)
            source_data = {"DataVersion": 5023, "stats": {"minecraft:custom": {
                "minecraft:open_barrel": 3, "minecraft:leave_game": 2, "future:unknown_stat": 17}}}
            path.write_text(json.dumps(source_data))
            source = player_data.read_statistics(world)
            self.assertEqual(player_data.statistics_counts(source, 3, 2)["counts"]["minecraft:open_barrel"], 3)
            self.assertEqual(source["data"]["stats"]["minecraft:custom"]["future:unknown_stat"], 17)
            with self.assertRaises(AssertionError):
                player_data.statistics_counts(source, 5, 3)
            for opens, leaves in ((5, 3), (5, 2), (3, 3)):
                continued = deepcopy(source_data)
                continued["stats"]["minecraft:custom"].update({"minecraft:open_barrel": opens, "minecraft:leave_game": leaves})
                path.write_text(json.dumps(continued))
                saved = player_data.read_statistics(world)
                if (opens, leaves) == (5, 3):
                    self.assertEqual(player_data.statistics_counts(saved, 5, 3)["counts"]["minecraft:leave_game"], 3)
                else:
                    with self.subTest(opens=opens, leaves=leaves), self.assertRaises(AssertionError):
                        player_data.statistics_counts(saved, 5, 3)

    def test_canonical_json_rejects_duplicate_keys_versions_and_noninteger_counts(self):
        invalid = [
            '{"DataVersion":5023,"DataVersion":5023,"stats":{}}',
            '{"DataVersion":5022,"stats":{}}', '{"stats":{}}', '{"DataVersion":5023,"stats":[]}',
            '{"DataVersion":5023,"stats":{"8":{"63":3}}}',
            *['{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":' + value + '}}}'
              for value in ("true", "3.0", "-1", "2147483648", '"3"', "NaN")],
            '{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":3,"minecraft:open_barrel":5}}}',
        ]
        with tempfile.TemporaryDirectory() as directory:
            world = Path(directory)
            path = world / "players/stats" / f"{player_data.PLAYER_ID}.json"
            path.parent.mkdir(parents=True)
            for raw in invalid:
                with self.subTest(raw=raw):
                    path.write_text(raw)
                    with self.assertRaises(AssertionError):
                        player_data.read_statistics(world)

    def test_stats_request_rejects_unwritten_ack_and_requires_fresh_received_value(self):
        contract = {"request_packet_id": 12, "request_action_id": 1, "award_stats_packet_id": 3,
                    "stat_types": {"minecraft:custom": {"protocol_id": 8}},
                    "custom_stats": {"minecraft:open_barrel": {"protocol_id": 63}},
                    "reports": {"registries": {"sha256": player_data.REGISTRIES_SHA256}}}
        client = Mock()
        client.request.return_value = {"packet_id": 12, "action_id": 1, "written": False, "after_sequence": 10}
        with self.assertRaises(AssertionError):
            player_data.request_statistics(client, contract, 3)
        client.wait_state.assert_not_called()
        client.request.return_value["written"] = True
        packet = {"packet_id": 3, "sequence": 11, "decoded_bytes": 5, "payload_bytes": 5,
                  "entries": [{"category_id": 8, "statistic_id": 63, "value": 3}]}

        def received(predicate):
            self.assertFalse(predicate({"statistics_packet": {**packet, "sequence": 10}}))
            self.assertTrue(predicate({"statistics_packet": packet}))
            return {"statistics_packet": packet}

        client.wait_state.side_effect = received
        self.assertEqual(player_data.request_statistics(client, contract, 3)["named_observation"]["value"], 3)
        packet["entries"][0]["value"] = 2
        with self.assertRaises(AssertionError):
            player_data.request_statistics(client, contract, 3)


class CompositeFailureLabelTests(unittest.TestCase):
    def test_only_recorded_transfers_use_journey_labels(self):
        rows = [{"id": "fixture.failed", "result": "failed"}, {"id": "fixture.passed", "result": "passed"}]
        ordinary = {"servers": {"vanilla": rows, "rust": rows}}
        self.assertEqual(failure_labels(ordinary), ["vanilla:fixture.failed", "rust:fixture.failed"])
        composite = {**ordinary, "transfers": {"vanilla": {"direction": "vanilla-to-rust"},
                                               "rust": {"direction": "rust-to-vanilla"}}}
        self.assertEqual(failure_labels(composite), ["vanilla-to-rust:fixture.failed", "rust-to-vanilla:fixture.failed"])


if __name__ == "__main__":
    unittest.main()
