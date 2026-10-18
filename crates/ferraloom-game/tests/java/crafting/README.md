`bash crates/ferraloom-game/tests/java/crafting/run-primitives.sh` regenerates the
protocol component primitive fixtures using real DFU/Gson/NBT codecs. It covers
integer narrowing and NumberLimits, direct float rounding, all numeric NBT kinds,
and the shared compound-or-SNBT alternative used by custom and typed entity data.
