"""One row per build and bot count from run_explore results: MSPT, TPS, chunks, fully generated chunks, pull-backs."""
import json, sys
from pathlib import Path
for d in sys.argv[1:]:
    for f in sorted(Path(d).glob('horizon-*-11.json'), key=lambda p: int(p.stem.split('-')[1])):
        r = json.loads(f.read_text())['runs'][0]
        full = json.loads((f.parent / (f.stem + '.chunks.json')).read_text())
        m = r.get('mspt') or {}
        print(f"{Path(d).name:16} {r['bots']:>4} mspt {m.get('average')!s:>7} p99 {m.get('p99')!s:>7} tps {r.get('tps')!s:>6} "
              f"chunks/s {r.get('chunks_per_second')!s:>7} full {full.get('overworld:minecraft:full', 0):>6} pulled {r.get('pulled_back')!s:>7} "
              f"cpu {round((r.get('cpu_percent') or {}).get('mean') or 0)}% busiest {round((r.get('busiest_thread_percent') or {}).get('mean') or 0)}%")
