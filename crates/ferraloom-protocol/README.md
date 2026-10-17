# Ferraloom protocol 777

`codec::Reader` borrows an unframed packet; `Writer` owns its output. Fixed-width
numbers are big endian. VarInts and VarLongs use explicit methods or the typed
`VarInt` / `VarLong` wrappers. `Wire` describes a type's default representation;
`StreamCodec<T>` selects an alternate representation without changing `T`.

Individual packet structs encode their body. Each state/direction's packet enum
also writes the generated packet ID. `Wire::from_bytes` rejects trailing bytes;
`Wire::decode` supports composition inside another structure.

```rust
use ferraloom_protocol::{Wire, handshake::{ClientIntent, ClientIntention, ServerboundPacket}};

let packet = ServerboundPacket::Intention(ClientIntention {
    protocol_version: 777,
    host_name: "localhost".into(),
    port: 25565,
    intention: ClientIntent::Login,
});
let bytes = packet.to_bytes()?;
# Ok::<(), ferraloom_protocol::Error>(())
```

Apply transport layers in this order: packet bytes, optional compression,
21-bit length framing, optional encryption. Reverse the order on receive.
Cipher state persists across chunks and packets; use separate encoder/decoder
instances. FrameDecoder accepts partial transport chunks and multiple frames.
Compression thresholds below zero bypass the compression envelope, matching an
absent vanilla compression handler.

Dynamic registry references use `Reader::with_context` and
`Writer::with_context`. `RegistryContext` maps connection-local IDs to canonical
identifiers and validates named tags. Game/server own the registry contents and
order. No global registry state is held by this crate. Registry-independent
codecs work without a context. References are checked rather than silently
accepted as arbitrary IDs.

`chat::Component` and `chat::Style` are the canonical chat types. Their `Wire`
representation is nameless network NBT with vanilla's default 2 MiB accounting
quota and 512-level NBT depth limit. `decode_trusted` lifts the heap quota while
retaining the depth limit. `encode_ops` preserves boolean, numeric, array and
list operations for `ops::{NbtOps, JsonOps, HashOps}`. JSON methods distinguish
strict JSON syntax from the legacy lenient syntax used by login disconnects.

`Reader::nbt_with_accounter` and `optional_nbt_with_accounter` support packets
with their own quotas. Nullable NBT uses TAG_End, unlike `Option<Tag>` which
writes a boolean followed by a non-null tag.

The login custom-query answer intentionally retains its entire wire suffix,
including the nullable discriminator. Vanilla 26.3's reader consumes all
remaining bytes without reading that discriminator, and accepts an empty suffix.

Run `tests/java/generate.sh` on devbox to regenerate fixture bytes with the
shared vanilla jar and JDK 25. Rust builds/tests run only through the committed
builder workflow in the repository's AGENTS.md.
