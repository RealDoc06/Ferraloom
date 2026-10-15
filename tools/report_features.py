#!/usr/bin/env python3
"""Generate the human-readable feature checklist from the evidence ledger."""
from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
CATEGORY_NAMES = {
    'network': 'Networking',
    'world': 'World generation and data packs',
    'persistence': 'Persistence',
    'blocks': 'Blocks and simulation',
    'entities': 'Entities',
    'items': 'Items and inventories',
    'players': 'Players and commands',
    'admin': 'Administration',
    'lifecycle': 'Lifecycle',
    'concurrency': 'Concurrency and performance',
    'plugins': 'Plugins (deferred)',
}


def cell(value: str) -> str:
    return value.replace('|', '\\|').replace('\r', ' ').replace('\n', ' ')


def observation_counts(feature: dict, implementation: str) -> str:
    results = Counter(record[implementation]['result'] for record in feature['evidence'])
    if set(results) - {'passed', 'failed', 'skipped'}:
        raise ValueError(f'{feature["id"]}: invalid {implementation} observation result')
    return '/'.join(str(results[result]) for result in ('passed', 'failed', 'skipped'))


def render(document: dict) -> str:
    features = document['features']
    identifiers = [feature['id'] for feature in features]
    if len(set(identifiers)) != len(identifiers):
        raise ValueError('Feature IDs must be unique')
    lines = [
        '# Feature checklist', '',
        f'Minecraft Java **{document["target"]["version"]}**. {len(features)} feature families, generated from '
        '[the evidence ledger](../compatibility/features.json).', '',
        '**Counts are selected recorded observations, not full feature coverage.** '
        'Vanilla and Rust columns show **pass/fail/skip** counts. `0/0/0` means no recorded observations. '
        'Retained observations can refer to different builds; consult their build IDs and artifacts in the ledger. '
        'A passing observation does not verify the whole family.', '',
        'Concurrency and performance rows are additional Ferraloom requirements. '
        'Plugin integration is deferred until vanilla server integration is complete.', '',
        'Regenerate with `python3 tools/report_features.py`.', '',
    ]
    categories = dict.fromkeys(feature['category'] for feature in features)
    for category in categories:
        title = CATEGORY_NAMES.get(category, category.replace('_', ' ').title())
        lines.extend([
            f'## {title}', '',
            '| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |',
            '| --- | --- | --- | --- | --- |',
        ])
        for feature in features:
            if feature['category'] != category:
                continue
            status = feature['rust_status']
            if feature['scope'] == 'deferred_plugin':
                status = f'deferred ({status})'
            lines.append(f'| `{cell(feature["id"])}` | {cell(feature["vanilla_expectation"])} | '
                         f'{observation_counts(feature, "vanilla")} | {observation_counts(feature, "rust")} | {cell(status)} |')
        lines.append('')
    return '\n'.join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', type=Path, default=ROOT / 'compatibility/features.json')
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/features.md')
    args = parser.parse_args(argv)
    try:
        document = json.loads(args.inventory.read_text())
        rendered = render(document)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f'feature report failed: {error}', file=sys.stderr)
        return 2
    print(f'Wrote {len(document["features"])} feature families to {args.output}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
