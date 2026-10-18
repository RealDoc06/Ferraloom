`bash crates/ferraloom-game/tests/java/crafting/run-primitives.sh` regenerates the
protocol component primitive fixtures using real DFU/Gson/NBT codecs. It covers
integer narrowing and NumberLimits, direct float rounding, all numeric NBT kinds,
and the shared compound-or-SNBT alternative used by custom and typed entity data.

The primitive harness constructs Gson lazy numeric values directly for codec
limit probes; vanilla's strict resource reader separately rejects number tokens
of 1,024 characters. It also compares native NBT encoding and HashOps CRC32C,
including raw negative zero and NaN payloads. NbtOps canonicalizes signed zero;
HashOps preserves raw bits.
