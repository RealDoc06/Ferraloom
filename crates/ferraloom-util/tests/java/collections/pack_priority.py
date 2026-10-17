"""Lossless heap-array edits plus full iterator visitation sequences."""
import struct
import sys

ACTIONS = "initial offer poll remove clear iterremove removeat peek".split()
with open(sys.argv[1]) as source, open(sys.argv[2], "wb") as target:
    target.write(b"FLQ1")
    previous = []
    for line in source:
        fields = line.split()
        if fields[0] == "CASE":
            flags = sum((fields[i+2] == "true") << i for i in range(3))
            target.write(struct.pack("<BHB", 255, int(fields[1]), flags))
            previous = []
            continue
        action, key, step, capacity, result, *sequences = fields
        heap = [int(k) for k in sequences[0].split(",") if k] if sequences[0] != "-" else []
        visited = [int(k) for k in sequences[1].split(",") if k] if sequences[1] != "-" else []
        prefix = 0
        while prefix < min(len(heap), len(previous)) and heap[prefix] == previous[prefix]:
            prefix += 1
        suffix = 0
        while suffix < min(len(heap), len(previous))-prefix and heap[-suffix-1] == previous[-suffix-1]:
            suffix += 1
        inserted = heap[prefix:len(heap)-suffix]
        target.write(struct.pack("<BBHhHHHH", ACTIONS.index(action), int(key), int(capacity), int(result),
                                 prefix, len(previous)-prefix-suffix, len(inserted), len(visited)))
        target.write(bytes(inserted))
        target.write(bytes(visited))
        previous = heap
