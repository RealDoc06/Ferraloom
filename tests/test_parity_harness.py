"""Guard against a broken test harness claiming successful compatibility."""
from pathlib import Path
import argparse
import os
import signal
import socket
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from parity import protocol
from parity.suite import Server, check_signal_exit, compare_command_trees, is_server_say, pin_binary, recorded, stop_by_signal


class HarnessTests(unittest.TestCase):
    def test_command_syntax_comparison_normalizes_arguments_but_retains_missing_branches(self):
        def tree(roots, paths):
            return [{"id": "network.command_tree", "result": "passed", "detail": {"roots": roots, "literal_paths": paths}}]

        vanilla = tree(["execute", "scoreboard"], ["execute as <targets> run", "scoreboard objectives setdisplay"])
        rust = tree(["execute", "scoreboard", "pumpkin"], ["execute as <entities> run", "scoreboard objectives setdisplay"])
        result = compare_command_trees({"vanilla": vanilla, "rust": rust})
        self.assertEqual(result["result"], "passed")
        self.assertEqual(result["extra_roots"], ["pumpkin"])
        rust[0]["detail"]["literal_paths"].pop()
        result = recorded("commands.advertised_syntax", (), lambda: compare_command_trees({"vanilla": vanilla, "rust": rust}))
        self.assertEqual(result["result"], "failed")
        self.assertEqual(result["detail"]["missing_literal_paths"], ["scoreboard objectives setdisplay"])
        rust[0]["result"] = "failed"
        with self.assertRaises(AssertionError):
            compare_command_trees({"vanilla": vanilla, "rust": rust})

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

    @unittest.skipUnless(os.name == 'posix', 'Unix executable replacement and process signals')
    def test_cargo_replacement_cannot_change_the_pinned_executable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'server'
            source.write_text('#!/bin/sh\necho original\n')
            source.chmod(0o700)
            pinned = pin_binary(source, root)
            replacement = root / 'replacement'
            replacement.write_text('#!/bin/sh\necho replacement\n')
            replacement.chmod(0o700)
            replacement.replace(source)
            self.assertEqual(subprocess.check_output([pinned], text=True).strip(), 'original')
            self.assertEqual(subprocess.check_output([source], text=True).strip(), 'replacement')

    @unittest.skipUnless(os.name == 'posix', 'Unix process groups and SIGKILL')
    def test_ignored_sigterm_cannot_receive_a_console_cleanup_save(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / 'console-save'
            script = ('import pathlib,signal,sys\n'
                      'signal.signal(signal.SIGTERM, signal.SIG_IGN)\n'
                      'print("ready",flush=True)\n'
                      'for line in sys.stdin:\n'
                      ' if line.strip()=="stop":\n'
                      '  pathlib.Path(sys.argv[1]).touch();break\n')
            server = Server('rust', root, argparse.Namespace())
            process = subprocess.Popen([sys.executable, '-c', script, str(marker)],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, start_new_session=True)
            server.process = process
            try:
                self.assertEqual(process.stdout.readline().strip(), 'ready')
                result = stop_by_signal(server, timeout=0.05)
                self.assertEqual(result['result'], 'failed')
                self.assertEqual(process.returncode, -signal.SIGKILL)
                self.assertFalse(marker.exists())
                self.assertTrue(server.forced_stop)
            finally:
                if process.poll() is None:
                    server.signal(signal.SIGKILL)
                    process.wait(timeout=5)
                process.stdin.close()
                process.stdout.close()


if __name__ == "__main__":
    unittest.main()
