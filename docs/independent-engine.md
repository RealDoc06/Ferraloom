# Independent engine

Ferraloom's server is implemented here directly in Rust. It uses Tokio for
network execution, ordinary serialization/cryptography crates, and official
Minecraft26.3 data as interoperability inputs. It has no dependency on another
Minecraft server engine. The prior prototype is preserved separately on
`archive/pumpkin-prototype-ee7`.

The first implementation covers bounded framing, status/ping, offline player
identity, login/configuration transitions, initial flat chunks, teleport
acknowledgment, chunk streaming as the player moves, and keepalive handling.
Connections run on the multithreaded runtime. Chunk serialization runs in its
blocking work pool. World simulation parallelism and performance are not proved
by these worker counts.

No independent-engine feature family is fully verified. The existing200-family
inventory remains the intended scope. Its old status counts and all old result
reports describe the archived engine. A successful socket test is not proof of
graphical-client compatibility, vanilla gameplay, or a complete server.

Current gaps include account authentication, full registry-NBT synchronization
for clients without the exact core pack, vanilla generation, dynamic lighting,
collision/physics, interactions, shared multiplayer entities, inventory,
commands, persistence, and the rest of the vanilla feature inventory. Players
currently enter a temporary flat development world in creative mode. Unhandled
gameplay packets have no gameplay effect. No plugin support is implemented.

## Reference contracts

The target is Java26.3, protocol777, data version5023. Static protocol and data
research uses the official inner JAR with SHA256
`a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6`.
No existing server implementation is used as implementation source.

`tools/generate_reference_data.py` checks that JAR and the official registry
report before extracting identifier lists and tag membership. It copies no
server code or asset payloads. The generated metadata records explicit excluded
tag domains. Dynamic raw IDs follow Ferraloom's transmitted entry lists.

Configuration supports entries supplied by an explicitly acknowledged
`minecraft/core/26.3` known pack. `RegistrySynchronization.packRegistry` and
`NetworkRegistryLoadTask` in the exact official server establish absent-NBT
entries for that path. Clients declining the pack receive a deliberate refusal.

Selected exact bytecode contracts:

- `ClientboundLoginFinishedPacket`: GameProfile followed by a session UUID.
- `ClientboundLoginPacket`/`CommonPlayerSpawnInfo`: current game mode is VarInt.
- `ServerboundAcceptTeleportationPacket`: ID, position doubles, rotation floats.
- `LevelChunkSection.write`: nonempty-block count, fluid count, two palettes.
- `PalettedContainer$Data.write`: fixed packed-long count, no length prefix.
- `ByteBufCodecs$15`: light masks use little-endian byte arrays.
- `ClientboundLevelChunkPacketData`: typed heightmap map, not an NBT compound.

New results must bind the independent engine's exact source/executable. Future
differential and REA campaigns must exercise that executable; historical
prototype passes cannot be reused as its results.
