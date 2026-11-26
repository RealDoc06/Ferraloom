"""A tool or harness that starts the engine must not write into a real player's folders.

The engine logs into HORIZON_HOME and falls back to the player's own game folder when it is unset, so every
spawn goes through lab.game_env, which sets fresh scratch folders. This test fails for a script in tools/
that starts the engine or horizon-server without it."""

import ast
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from tools import lab

TOOLS = Path(lab.ROOT) / "tools"

# Strings that mean "this script starts the engine": a binary a harness runs.
SPAWN_MARKERS = {
    "horizon-server", "horizon-server.exe",
    "horizon-client.exe", "release/horizon-client", "fast/horizon-client", "debug/horizon-client",
}

# Scripts that mention those strings without starting a game themselves, and why.
EXEMPT = {
    "lab.py": "defines game_env",
    "parity.py": "runs the harnesses and gives each its own HORIZON_HOME",
    "release_train.py": "release bookkeeping",
    "pr_ready.py": "runs cargo checks and parity.py",
    "port_harness.py": "ports Java code, starts no game",
    "plugins_e2e.py": "gives each server it starts scratch HORIZON_HOME and XDG folders of its own",
    "rig.py": "runs on the builder without the checkout's tools; gives its server scratch folders under ~/client-rig",
    "test_server_combat_parity.py": "names the engine binary it builds",
    "test_server_entity_data_parity.py": "names the engine binary it builds",
    "test_server_items_parity.py": "names the engine binary it builds",
    "test_server_player_handlers_parity.py": "names the engine binary it builds",
}


def constants(tree):
    """Every string in a script that is a whole literal or a keyword name (docstrings are longer text)."""
    for node in ast.walk(tree):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            yield node.value
        elif isinstance(node, ast.keyword) and node.arg:
            yield node.arg


def uses_game_env(tree):
    """The script calls lab.game_env, or reaches it through integration_env.game_environment or
    test_dedicated_server_parity.Server."""
    return any(isinstance(node, ast.Attribute) and node.attr in ("game_env", "game_environment")
               or isinstance(node, ast.Name) and node.id in ("game_env", "game_environment")
               or isinstance(node, ast.ImportFrom) and (node.module or "").endswith("test_dedicated_server_parity")
               for node in ast.walk(tree))


class GameEnvTests(unittest.TestCase):
    def test_gives_every_game_scratch_folders(self):
        with mock.patch.dict(os.environ, {}, clear=False):
            for key in ("HORIZON_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME"):
                os.environ.pop(key, None)
            first, second = lab.game_env(), lab.game_env()
        for key in ("HORIZON_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME"):
            self.assertTrue(Path(first[key]).is_dir(), key)
            self.assertNotEqual(first[key], second[key], key)

    def test_keeps_the_callers_folders(self):
        with tempfile.TemporaryDirectory() as temp:
            env = lab.game_env({"PATH": "x", "HORIZON_HOME": "/from/base"}, XDG_DATA_HOME=Path(temp), A=1)
        self.assertEqual(env["HORIZON_HOME"], "/from/base")
        self.assertEqual(env["XDG_DATA_HOME"], temp)
        self.assertEqual((env["PATH"], env["A"]), ("x", "1"))

    def test_does_not_touch_the_process_environment(self):
        before = dict(os.environ)
        lab.game_env()
        self.assertEqual(dict(os.environ), before)


class SpawnerTests(unittest.TestCase):
    def test_every_script_that_starts_the_game_uses_game_env(self):
        offenders = []
        for path in sorted(TOOLS.rglob("*.py")):
            if path.name in EXEMPT:
                continue
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
            if SPAWN_MARKERS & set(constants(tree)) and not uses_game_env(tree):
                offenders.append(str(path.relative_to(lab.ROOT)))
        self.assertEqual(offenders, [], "these start the game without lab.game_env (HORIZON_HOME falls back to a "
                                        "real player's folder); use it, or list the script in EXEMPT")


if __name__ == "__main__":
    unittest.main()
