#!/usr/bin/env python3
"""Rebuild FEATURES.md from the inventory tables, or validate it with --check."""
from collections import Counter
from pathlib import Path
import argparse
import re

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
header = '| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |'
intro = (
    '# Vanilla feature inventory\n\n'
    'Minecraft Java dedicated server 26.3 (protocol 777, data version 5023). '
    'Use the linked files as the porting checklist: stable IDs identify concrete behaviors or registry entries, '
    'Vanilla source identifies the implementation or built-in resource, and Test links the focused parity scenario. '
    'An empty Test cell, Vanilla `-` and Ferraloom `todo` mean untested, regardless of existing Rust code. '
    'Run each scenario against vanilla before recording the Ferraloom result; use `pass`, `partial` or `fail` for measured results. '
    'Ferraloom passes require agreement in single-region and parallel-region modes. '
    'Optional bundled datapacks and integrated-only command entries are labeled. '
    'Registry rows were extracted from the official reports and bootstrapped runtime; resource rows enumerate the bundled datapack; '
    'mechanics rows come from reading the decompiled source. '
    'Run `python3 features/counts.py` after edits, or add `--check` to validate the index.\n\n'
    '| Area | Rows | Ferraloom todo | Ferraloom partial | Ferraloom pass | Ferraloom fail |\n'
    '|---|---:|---:|---:|---:|---:|\n'
)
lines = [intro.rstrip()]
seen = set()
totals = Counter()
for file in sorted((root / 'features').glob('*.md')):
    text = file.read_text()
    assert text.count(header) == 1, f'{file}: expected one inventory table'
    title = text.splitlines()[0].removeprefix('# ')
    counts = Counter()
    for line in text.splitlines():
        if not line.startswith('| ') or line == header:
            continue
        cells = [cell.strip() for cell in line.split('|')[1:-1]]
        assert len(cells) == 6, f'{file}: expected six columns'
        feature_id, feature, source, test, vanilla, status = cells
        assert re.fullmatch(r'[a-z0-9]+(?:[.-][a-z0-9]+)*', feature_id), feature_id
        assert feature_id.startswith(file.stem + '.'), feature_id
        assert feature_id not in seen, f'duplicate ID: {feature_id}'
        assert feature and source, feature_id
        assert vanilla in {'-', 'pass', 'fail'}, feature_id
        assert status in {'todo', 'partial', 'pass', 'fail'}, feature_id
        seen.add(feature_id)
        counts[status] += 1
    count = counts.total()
    assert count, f'{file}: empty inventory'
    totals.update(counts)
    lines.append(f'| [{title}](features/{file.name}) | {count} | {counts["todo"]} | {counts["partial"]} | {counts["pass"]} | {counts["fail"]} |')
lines.append(f'| **Total** | **{totals.total()}** | **{totals["todo"]}** | **{totals["partial"]}** | **{totals["pass"]}** | **{totals["fail"]}** |')
expected = '\n'.join(lines) + '\n'
index = root / 'FEATURES.md'
if args.check:
    assert index.read_text() == expected, 'FEATURES.md is stale; run python3 features/counts.py'
else:
    index.write_text(expected)
print(f'{len(seen)} rows across {len(list((root / "features").glob("*.md")))} areas')
