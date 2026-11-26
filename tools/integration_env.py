"""Per-process scratch data for game integration tests (never a player's home)."""
from pathlib import Path
import os
import subprocess
import tempfile

try:
    from . import lab
except ImportError:
    import lab


def game_environment(base, root):
    root = Path(root)
    env = dict(base, HORIZON_TELEMETRY_URL='off')
    for key, folder in [('HORIZON_HOME', 'home'), ('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config')]:
        path = root / folder
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    return lab.game_env(env)


def run_game(command, **kwargs):
    """Run an engine fixture (HORIZON_DISPENSE_FIXTURE, HORIZON_SCULK_FIXTURE) in fresh scratch homes."""
    with tempfile.TemporaryDirectory(prefix='hz-fixture-') as root:
        kwargs['env'] = game_environment(kwargs.pop('env', os.environ), root)
        return subprocess.run(command, **kwargs)


def require_server_reports(resources):
    """Match Builtin.find: a registry report shadows later fallback directories."""
    resources = Path(resources)
    reports = next((p for p in (resources, resources / '../../datagen/out/reports',
                                resources / '../datagen/out/reports')
                    if (p / 'registries.json').is_file()), None)
    if reports is None or not (reports / 'commands.json').is_file():
        raise ValueError(f'incomplete prepared resources in {resources}: registries.json and '
                         'commands.json must be installed together; prepare the pinned resources again')
    return reports
