"""Sums HORIZON_SPIKES slow-tick lines from a server log: total and mean ms per span, share of slow-tick time."""
import collections, re, sys
ticks, total, sums = 0, 0.0, collections.Counter()
for line in open(sys.argv[1], errors='replace'):
    m = re.search(r'slow tick ([\d.]+) ms.*?\): (.*)$', line)
    if not m:
        continue
    ticks += 1
    total += float(m.group(1))
    for part in m.group(2).split(', '):
        name, _, value = part.rpartition(' ')
        try:
            sums[name] += float(value.split('/')[0])
        except ValueError:
            pass
print(f'{ticks} slow ticks, mean {total / max(ticks, 1):.1f} ms')
for name, ms in sums.most_common(16):
    print(f'{name:24} {ms / max(ticks, 1):7.2f} ms/tick  {100 * ms / max(total, 1):5.1f} %')
