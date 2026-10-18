"""Lossless prefix/suffix edit encoding of Java's full per-operation orders.

The Rust tests reconstruct and compare every key/value pair, without hashes or
an external decompression dependency. The text output stays in a temporary dir.
"""
import struct
import sys

ACTIONS = "put remove trim clear first last getfirst getlast popfirst poplast iterremove trimto get".split()
with open(sys.argv[1]) as source, open(sys.argv[2], "wb") as target:
    target.write(b"FLC1")
    previous = []
    for line in source:
        if line.startswith("CASE "):
            header = line[5:].strip().encode()
            target.write(bytes([255, len(header)]))
            target.write(header)
            previous = []
            continue
        action, key, value, capacity, *order = line.split()
        entries = [tuple(map(int, item.split(":"))) if ":" in item else (int(item), 0)
                   for item in (order[0].split(",") if order else []) if item]
        prefix = 0
        while prefix < min(len(entries), len(previous)) and entries[prefix] == previous[prefix]:
            prefix += 1
        suffix = 0
        while suffix < min(len(entries), len(previous)) - prefix and entries[-suffix-1] == previous[-suffix-1]:
            suffix += 1
        inserted = entries[prefix:len(entries)-suffix]
        target.write(struct.pack("<BBBHBBB", ACTIONS.index(action), int(key), int(value), int(capacity),
                                 prefix, len(previous)-prefix-suffix, len(inserted)))
        for k, v in inserted:
            target.write(bytes([k, v]))
        previous = entries
