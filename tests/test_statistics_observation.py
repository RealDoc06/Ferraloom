"""Exercise the real client's selected-counter observation without a server."""
from pathlib import Path
import queue
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from parity.bot import Client


def client() -> Client:
    observed = Client.__new__(Client)
    observed.messages = queue.Queue()
    observed.failure = None
    observed.pending = {}
    observed.events = []
    observed.statistics_values = {}
    observed.jukeboxes = False
    observed.crafting = False
    observed.dragon_parts = False
    return observed


class StatisticObservationTests(unittest.TestCase):
    def test_omitted_updates_retain_the_raw_witness_and_explicit_zero_replaces_it(self):
        observed = client()
        observed.watch_statistic(2, 1504)
        first = {"event": "statistics_packet", "sequence": 10, "payload_base64": "synthetic-first",
                 "entries": [{"category_id": 2, "statistic_id": 1504, "value": 1}]}
        omitted = {"event": "statistics_packet", "sequence": 20,
                   "entries": [{"category_id": 8, "statistic_id": 4, "value": 400}]}
        for packet in (first, omitted):
            observed.messages.put(packet)
            observed.receive(1)
        self.assertEqual(observed.statistics_values[(2, 1504)],
                         {"entry": first["entries"][0], "packet": first})
        for _ in range(101):
            observed.messages.put({"event": "wire_packet"})
            observed.receive(1)
        self.assertNotIn(first, observed.events)
        self.assertIs(observed.statistics_values[(2, 1504)]["packet"], first)
        zero = {"event": "statistics_packet", "sequence": 30,
                "entries": [{"category_id": 2, "statistic_id": 1504, "value": 0}]}
        observed.messages.put(zero)
        observed.receive(1)
        self.assertEqual(observed.statistics_values[(2, 1504)]["entry"]["value"], 0)
        self.assertIsNone(client().statistics_values.get((2, 1504)))

    def test_counter_subscriptions_are_bounded_and_rewatching_preserves_values(self):
        observed = client()
        for index in range(64):
            observed.watch_statistic(2, index)
        observed.statistics_values[(2, 0)] = {"entry": {"value": 7}}
        observed.watch_statistic(2, 0)
        self.assertEqual(observed.statistics_values[(2, 0)]["entry"]["value"], 7)
        with self.assertRaises(ValueError):
            observed.watch_statistic(2, 64)
        for invalid in (-1, True, "2", None):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                observed.watch_statistic(invalid, 0)
