# Binary data, geometry and randomness

Binary codecs, geometry, random sources and noise primitives are prerequisites for gameplay parity. Generic random-value provider registry entries are included here.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| foundations.nbt.binary-limits | Enforce NBT depth and byte accounting limits and reject malformed binary tags | `NbtAccounter; NbtIo` | | - | todo |
| foundations.nbt.snbt-parser | Parse and print SNBT compounds, lists, arrays, numbers, escapes and typed values | `TagParser; SnbtPrinterTagVisitor` | | - | todo |
| foundations.nbt.path | Apply NBT path index, wildcard and match selectors for get, set, insert and remove | `NbtPathArgument` | | - | todo |
| foundations.nbt.ops-codec | Map NBT values through codec operations with correct numeric and list behavior | `NbtOps` | | - | todo |
| foundations.buffers.primitives | Encode VarInt, VarLong, UTF-8 strings, UUIDs, positions, bitsets and NBT with bounds | `FriendlyByteBuf; VarInt; VarLong` | | - | todo |
| foundations.buffers.components | Encode registry-aware item components and item stacks using registry ids | `RegistryFriendlyByteBuf; ItemStack#STREAM_CODEC` | | - | todo |
| foundations.geometry.block-positions | Pack signed block and section coordinates with vanilla bit layout and limits | `BlockPos; SectionPos; ChunkPos` | | - | todo |
| foundations.geometry.shapes | Compute voxel shape unions, intersections, face occlusion and ray clipping | `Shapes; VoxelShape` | | - | todo |
| foundations.geometry.collision | Apply AABB clipping and axis ordering for swept collisions | `AABB; VoxelShape` | | - | todo |
| foundations.math.rounding | Match floor, ceil, angle wrapping, trigonometry and interpolation semantics | `Mth` | | - | todo |
| foundations.random.legacy | Match legacy Java-compatible random sequences, bounded integers and Gaussian cache | `LegacyRandomSource; MarsagliaPolarGaussian` | | - | todo |
| foundations.random.xoroshiro | Match xoroshiro128++ state updates and primitive sampling | `Xoroshiro128PlusPlus; XoroshiroRandomSource` | | - | todo |
| foundations.random.positional | Derive coordinate and identifier random sources with vanilla seed mixing | `PositionalRandomFactory; RandomSupport` | | - | todo |
| foundations.random.worldgen-seeding | Apply decoration, feature, large-feature and slime-chunk seed formulas | `WorldgenRandom` | | - | todo |
| foundations.noise.primitives | Match gradient, simplex, Perlin and normal noise sampling and stack normalization | `GradientNoise; SimplexNoise; PerlinNoise; NormalNoise` | | - | todo |
| foundations.noise.perlin-derivatives | Evaluate float Perlin noise samples and analytic derivative output | `PerlinNoise#noiseWithDerivative` | | - | todo |
| foundations.noise.simplex | Match simplex permutation, skewing, gradients and float accumulation | `SimplexNoise` | | - | todo |
| foundations.noise.normal-stack | Evaluate normal noise stacks and configured amplitude normalization | `NormalNoise; NoiseStack` | | - | todo |
| foundations.noise.smeared-perlin | Evaluate smeared Perlin sampling used by the registered density functions | `SmearedPerlinNoise` | | - | todo |
| foundations.noise.volume-sampling | Accumulate noise into density volumes using the bulk sampling implementation | `PerlinNoise#addToVolume; DensityBuffer; DensityVolume` | | - | todo |
| foundations.float-provider-type.clamped-normal | `minecraft:clamped_normal` (float provider type) | `net.minecraft.util.valueproviders.FloatProviders#bootstrap` | | - | todo |
| foundations.float-provider-type.constant | `minecraft:constant` (float provider type) | `net.minecraft.util.valueproviders.FloatProviders#bootstrap` | | - | todo |
| foundations.float-provider-type.trapezoid | `minecraft:trapezoid` (float provider type) | `net.minecraft.util.valueproviders.FloatProviders#bootstrap` | | - | todo |
| foundations.float-provider-type.uniform | `minecraft:uniform` (float provider type) | `net.minecraft.util.valueproviders.FloatProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.biased-to-bottom | `minecraft:biased_to_bottom` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.clamped | `minecraft:clamped` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.clamped-normal | `minecraft:clamped_normal` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.constant | `minecraft:constant` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.trapezoid | `minecraft:trapezoid` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.uniform | `minecraft:uniform` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.very-biased-to-bottom | `minecraft:very_biased_to_bottom` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.int-provider-type.weighted-list | `minecraft:weighted_list` (int provider type) | `net.minecraft.util.valueproviders.IntProviders#bootstrap` | | - | todo |
| foundations.nbt.byte-array-tag | Encode, decode, copy and SNBT round-trip `ByteArrayTag` | `net.minecraft.nbt.ByteArrayTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.byte-tag | Encode, decode, copy and SNBT round-trip `ByteTag` | `net.minecraft.nbt.ByteTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.compound-tag | Encode, decode, copy and SNBT round-trip `CompoundTag` | `net.minecraft.nbt.CompoundTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.double-tag | Encode, decode, copy and SNBT round-trip `DoubleTag` | `net.minecraft.nbt.DoubleTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.end-tag | Encode, decode, copy and SNBT round-trip `EndTag` | `net.minecraft.nbt.EndTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.float-tag | Encode, decode, copy and SNBT round-trip `FloatTag` | `net.minecraft.nbt.FloatTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.int-array-tag | Encode, decode, copy and SNBT round-trip `IntArrayTag` | `net.minecraft.nbt.IntArrayTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.int-tag | Encode, decode, copy and SNBT round-trip `IntTag` | `net.minecraft.nbt.IntTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.list-tag | Encode, decode, copy and SNBT round-trip `ListTag` | `net.minecraft.nbt.ListTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.long-array-tag | Encode, decode, copy and SNBT round-trip `LongArrayTag` | `net.minecraft.nbt.LongArrayTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.long-tag | Encode, decode, copy and SNBT round-trip `LongTag` | `net.minecraft.nbt.LongTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.short-tag | Encode, decode, copy and SNBT round-trip `ShortTag` | `net.minecraft.nbt.ShortTag; NbtIo; TagParser` | | - | todo |
| foundations.nbt.string-tag | Encode, decode, copy and SNBT round-trip `StringTag` | `net.minecraft.nbt.StringTag; NbtIo; TagParser` | | - | todo |
