"""Counts chunks in a world's region files by generation status: `full` is a finished chunk, the rest are proto-chunks."""
import collections, json, re, struct, sys, zlib, gzip
from pathlib import Path

STATUS = re.compile(rb'\x08\x00\x06Status\x00(.)', re.S)

def statuses(region):
    data = region.read_bytes()
    for i in range(1024):
        loc = struct.unpack('>I', data[i * 4:i * 4 + 4])[0]
        offset, sectors = (loc >> 8) * 4096, loc & 0xFF
        if not offset or not sectors:
            continue
        length, kind = struct.unpack('>IB', data[offset:offset + 5])
        body = data[offset + 5:offset + 4 + length]
        try:
            raw = zlib.decompress(body) if kind == 2 else gzip.decompress(body) if kind == 1 else body if kind == 3 else None
        except Exception:
            raw = None
        if raw is None:
            yield 'unreadable'
            continue
        m = STATUS.search(raw)
        if not m:
            yield 'no-status'
            continue
        n = m.group(1)[0]
        start = m.end()
        yield raw[start:start + n].decode(errors='replace')

world = Path(sys.argv[1])
counts = collections.Counter()
for region in world.glob('**/region/*.mca'):
    if region.stat().st_size >= 8192:
        counts.update(f'{region.parent.parent.name}:{s}' for s in statuses(region))
print(json.dumps(dict(sorted(counts.items()))))
