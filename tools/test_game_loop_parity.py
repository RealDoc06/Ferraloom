#!/usr/bin/env python3
"""Compare the server's tick rate manager with vanilla's: ServerTickRateManager
(set rate, freeze, step, sprint), driven with identical synthetic sequences."""
import json
import random

try:
    from . import lab, oracle_harness as harness
except ImportError:
    import lab, oracle_harness as harness


def rate_cases(rng):
    rates = [20.0, 1.0, 0.5, 0.0, -3.0, 100.0, 10000.0, 3.3, 1e-3, 33.333, 7.0, float('nan'), float('inf'), 59.99]
    for n in range(500):
        ops = []
        clock = 0
        for _ in range(rng.randrange(1, 40)):
            kind = rng.choice(['rate', 'freeze', 'step', 'stopstep', 'sprint', 'check', 'check', 'check', 'end', 'stopsprint',
                               'tick', 'tick', 'tick'])
            if kind == 'rate':
                ops.append(['rate', lab.fbits(rng.choice(rates))])
            elif kind == 'freeze':
                ops.append(['freeze', rng.random() < 0.5])
            elif kind == 'step':
                ops.append(['step', rng.choice([1, 2, 5, 20, 0, -1])])
            elif kind == 'sprint':
                ops.append(['sprint', rng.choice([1, 2, 3, 10, 100, 0])])
            elif kind in ('check', 'end'):
                clock += rng.choice([0, 1, 1000, 50_000_000, 123_456_789, 3_000_000_000])
                ops.append([kind, clock])
            else:
                ops.append([kind])
        yield dict(t='rate', ops=ops)


def make_cases(build):
    rng = random.Random(0x4c4f4f50)
    path = build / 'cases.jsonl'
    count = 0
    with path.open('w') as out:
        for generator in (rate_cases,):
            for case in generator(rng):
                out.write(json.dumps(case, separators=(',', ':')) + '\n')
                count += 1
    return path, count


def main():
    scope = 'ServerTickRateManager (set rate, freeze, step, sprint including the sprint report text).'
    harness.differential(
        'rust-game-loop-parity', 'VanillaGameLoopOracle', 'game_loop_probe', make_cases, scope,
        sources=[lab.ROOT / 'crates/horizon-client/src/clock.rs'])


if __name__ == '__main__':
    main()
