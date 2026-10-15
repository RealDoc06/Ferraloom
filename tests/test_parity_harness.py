"""Guard against a broken test harness claiming successful compatibility."""
from pathlib import Path
import socket
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from parity import protocol
from parity.suite import check_signal_exit, is_server_say, recorded


class HarnessTests(unittest.TestCase):
    def test_command_echo_is_never_success(self):
        token = "FERRALOOM_PASS_deadbeef"
        self.assertFalse(is_server_say(f"Unknown command execute if block 0 0 0 air run say {token}", token))
        self.assertFalse(is_server_say(f"[Server] {token} extra text", token))
        self.assertTrue(is_server_say(f"[15:00:00] [Server thread/INFO]: [Not Secure] [Server] {token}", token))

    def test_truncated_frame_raises_instead_of_passing(self):
        sender, receiver = socket.socketpair()
        try:
            sender.sendall(b"\x01")
            sender.close()
            with self.assertRaises(EOFError):
                protocol.read_exact(receiver, 2)
        finally:
            sender.close()
            receiver.close()

    def test_varint_overflow_is_rejected(self):
        sender, receiver = socket.socketpair()
        try:
            sender.sendall(b"\xff\xff\xff\xff\x7f")
            with self.assertRaises(ValueError):
                protocol.read_varint(receiver)
        finally:
            sender.close()
            receiver.close()

    def test_failed_requirement_always_raises(self):
        with self.assertRaises(AssertionError):
            protocol.require(False, "expected failure even with Python -O")

    def test_reported_comparison_failure_is_retained(self):
        result = recorded("worldgen.sample", (), lambda: {"result": "failed", "mismatched_blocks": 7})
        self.assertEqual(result["result"], "failed")
        self.assertEqual(result["detail"]["mismatched_blocks"], 7)

    def test_signal_exit_matches_native_process_conventions(self):
        self.assertEqual(check_signal_exit("vanilla", 143, False)["exit_code"], 143)
        self.assertEqual(check_signal_exit("rust", 0, False)["exit_code"], 0)
        for kind, code, forced in [("rust", 143, False), ("vanilla", 137, False), ("vanilla", 143, True)]:
            with self.subTest(kind=kind, code=code, forced=forced), self.assertRaises(AssertionError):
                check_signal_exit(kind, code, forced)


if __name__ == "__main__":
    unittest.main()
