/** Opt-in builtin22 projection for exact 26.3. Configuration order is received,
 * never borrowed from Java bootstrap order or Rust's generated registry IDs. */
import type { ContainerPacket, PlainStack } from './client.ts';
import { crc32c, HornReader } from './horns.ts';

// JukeboxSongs.bootstrap, Jukebox.javap.txt SHA256
// a5e00dc33c14550b1f70c88c525d7fad45eb702f0b70f217ead439e814f7fa6c.
export const SONG_PROFILES = {
  '11': [71, 11], '13': [178, 1], '5': [178, 15], blocks: [345, 3],
  bounce: [234, 8], cat: [185, 2], chirp: [185, 4], creator: [176, 12],
  creator_music_box: [73, 11], far: [174, 5], lava_chicken: [134, 9],
  mall: [197, 6], mellohi: [96, 7], otherside: [195, 14], pigstep: [149, 13],
  precipice: [299, 13], relic: [218, 14], stal: [150, 8], strad: [188, 9],
  tears: [175, 10], wait: [238, 12], ward: [251, 10],
} as const;
export type SongName = `minecraft:${keyof typeof SONG_PROFILES}`;
export const JUKEBOX_ITEM = 1474; // music_disc_11, static official item registry.
export const JUKEBOX_COMPONENT = 66;
export const DEFAULT_SONG: SongName = 'minecraft:11';
const builtinNames = new Set<string>(Object.keys(SONG_PROFILES).map(name => `minecraft:${name}`));

/** Identifier.CODEC canonical aliases only. Opaque custom_data never calls this. */
export function songName(value: unknown): SongName {
  if (typeof value !== 'string') throw new Error('Expected builtin song Identifier string');
  const canonical = value.startsWith(':') ? `minecraft${value}` : value.includes(':') ? value : `minecraft:${value}`;
  if (!builtinNames.has(canonical)) throw new Error('Unsupported song namespace/reference');
  return canonical as SongName;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

type SongEntry = { registry_id: number; reference_id: number; identifier: SongName; data: Record<string, unknown> };
export type SongRegistry = { registry: 'minecraft:jukebox_song'; entries: SongEntry[]; payload_bytes: number; decoded_bytes: number };

export function songRegistryEntries(values: { identifier: string; data: unknown }[]): SongEntry[] {
  if (values.length !== 22) throw new Error('Expected exactly 22 builtin songs');
  const seen = new Set<string>();
  return values.map(({ identifier: raw, data }, registry_id) => {
    const identifier = songName(raw);
    if (identifier !== raw || seen.has(identifier)) throw new Error('Noncanonical/duplicate song registry entry');
    seen.add(identifier);
    const key = identifier.slice(10) as keyof typeof SONG_PROFILES;
    const [seconds, comparator] = SONG_PROFILES[key];
    if (!record(data) || Object.keys(data).length !== 4 ||
        data.sound_event !== `minecraft:music_disc.${key}` || data.length_in_seconds !== seconds ||
        data.comparator_output !== comparator || !record(data.description)) throw new Error('Unsupported custom song profile');
    const description = data.description;
    if (Object.keys(description).some(key => !['translate', 'type'].includes(key)) ||
        description.translate !== `jukebox_song.minecraft.${key}` ||
        (description.type !== undefined && description.type !== 'translatable')) throw new Error('Unsupported song description');
    return { registry_id, reference_id: registry_id + 1, identifier, data };
  });
}

export function songReference(registry: SongRegistry, reference: number): SongName {
  if (!Number.isInteger(reference) || reference < 1 || reference > 22) throw new Error('Unknown/direct song holder unsupported');
  const entry = registry.entries[reference - 1];
  if (registry.entries.length !== 22 || !entry || entry.registry_id !== reference - 1 || entry.reference_id !== reference) {
    throw new Error('Unbound song holder');
  }
  return songName(entry.identifier);
}

/** Same PackedRegistryEntry codec as horns; known-packs response stays empty. */
export function decodeSongRegistryPacket(payload: Buffer): SongRegistry {
  const reader = new HornReader(payload);
  if (reader.varInt() !== 7 || reader.identifier() !== 'minecraft:jukebox_song' || reader.count(22) !== 22) {
    throw new Error('Expected 22 sent jukebox_song entries');
  }
  const entries = Array.from({ length: 22 }, () => {
    const identifier = reader.identifier();
    if (reader.byte() !== 1) throw new Error('Missing/invalid song registry data optional');
    return { identifier, data: reader.nbt() };
  });
  reader.end();
  return { registry: 'minecraft:jukebox_song', entries: songRegistryEntries(entries), payload_bytes: payload.length, decoded_bytes: reader.offset };
}

function discStack(reader: HornReader, registry: SongRegistry): PlainStack {
  const count = reader.count(1);
  if (!count) return null;
  const item_id = reader.varInt(), added = reader.count(2), removed = reader.count(1);
  if (item_id !== JUKEBOX_ITEM) throw new Error('Unsupported nonempty jukebox fixture item');
  const components: Record<string, unknown> = {}, seen = new Set<number>();
  for (let index = 0; index < added; index++) {
    const id = reader.varInt();
    if (seen.has(id)) throw new Error('Duplicate jukebox component');
    seen.add(id);
    if (id === JUKEBOX_COMPONENT) components['minecraft:jukebox_playable'] = songReference(registry, reader.varInt());
    else if (id === 0) {
      const data = reader.nbt();
      if (!record(data) || Object.keys(data).length !== 1 || data.jukebox_playable !== 'minecraft:cat') {
        throw new Error('Unsupported opaque jukebox control');
      }
      components['minecraft:custom_data'] = data;
    } else throw new Error(`Unsupported jukebox component ${id}`);
  }
  const removed_components: string[] = [];
  for (let index = 0; index < removed; index++) {
    const id = reader.varInt();
    if (id !== JUKEBOX_COMPONENT || seen.has(id)) throw new Error('Unsupported/conflicting jukebox removal');
    removed_components.push('minecraft:jukebox_playable');
  }
  return { item_id, count, ...(added ? { components } : {}), ...(removed ? { removed_components } : {}) };
}

export function decodeJukeboxContainerPacket(payload: Buffer, registry: SongRegistry): ContainerPacket {
  const reader = new HornReader(payload);
  const packet_id = reader.varInt(), container_id = reader.count(255), state_id = reader.count(32767);
  const size = container_id === 0 ? 46 : 63;
  let result: ContainerPacket;
  if (packet_id === 18) {
    if (reader.count(63) !== size) throw new Error('Wrong jukebox inventory size');
    const slots = Array.from({ length: size }, () => discStack(reader, registry));
    result = { kind: 'content', packet_id, container_id, state_id, slots, carried: discStack(reader, registry) };
  } else if (packet_id === 20) {
    const slot = reader.take(2).readInt16BE();
    if (slot < 0 || slot >= size) throw new Error('Jukebox slot outside inventory');
    result = { kind: 'slot', packet_id, container_id, state_id, slot, stack: discStack(reader, registry) };
  } else throw new Error('Expected jukebox content/slot packet');
  reader.end();
  return result;
}

/** HashOps string preimage is source-derived. Numeric Java hashes are NOT observed. */
export function songHashPreimage(value: unknown): Buffer {
  const name = songName(value), prefix = Buffer.alloc(5);
  prefix[0] = 12;
  prefix.writeUInt32LE(name.length, 1);
  return Buffer.concat([prefix, Buffer.from(name, 'utf16le')]);
}

function vint(value: number): Buffer {
  if (!Number.isInteger(value) || value < 0 || value > 2147483647) throw new Error('Invalid jukebox integer');
  const bytes: number[] = [];
  do { const byte = value & 127; value >>>= 7; bytes.push(byte | (value ? 128 : 0)); } while (value);
  return Buffer.from(bytes);
}

function short(value: number): Buffer {
  const bytes = Buffer.alloc(2);
  bytes.writeInt16BE(value);
  return bytes;
}

export function encodeJukeboxSelect(hotbar: number): Buffer {
  if (!Number.isInteger(hotbar) || hotbar < 0 || hotbar > 8) throw new Error('Invalid jukebox hotbar slot');
  return short(hotbar);
}

export function encodeJukeboxStackHash(stack: PlainStack): Buffer {
  if (stack === null) return Buffer.from([0]);
  if (stack.item_id !== JUKEBOX_ITEM || stack.count !== 1) throw new Error('Unsupported jukebox hash stack');
  const added = Object.entries(stack.components ?? {}), removed = stack.removed_components ?? [];
  if (added.length > 1 || removed.length > 1 || (added.length && removed.length) ||
      removed.some(name => name !== 'minecraft:jukebox_playable')) throw new Error('Unsupported jukebox hash patch');
  const hashes: Buffer[] = [];
  for (const [name, value] of added) {
    if (name !== 'minecraft:jukebox_playable') throw new Error('Opaque component hashing is unsupported');
    const bytes = Buffer.alloc(4);
    bytes.writeUInt32BE(crc32c(songHashPreimage(value)));
    hashes.push(vint(JUKEBOX_COMPONENT), bytes);
  }
  return Buffer.concat([Buffer.from([1]), vint(stack.item_id), vint(stack.count), vint(added.length), ...hashes,
    vint(removed.length), ...removed.map(() => vint(JUKEBOX_COMPONENT))]);
}

/** Predict the two actual changed slots, leaving the received menu untouched. */
export function encodeJukeboxSwap(container: number, state: number, slot: number, hotbar: number, slots: PlainStack[]): Buffer {
  if (!Number.isInteger(container) || container < 1 || container > 255 || !Number.isInteger(state) || state < 0 || state > 32767 ||
      !Number.isInteger(slot) || slot < 0 || slot > 26 || slots.length !== 63) throw new Error('Invalid jukebox SWAP menu');
  encodeJukeboxSelect(hotbar);
  const target = 54 + hotbar;
  return Buffer.concat([vint(container), vint(state), short(slot), Buffer.from([hotbar]), vint(2), vint(2),
    short(slot), encodeJukeboxStackHash(slots[target]!), short(target), encodeJukeboxStackHash(slots[slot]!), Buffer.from([0])]);
}

function blockPosition(reader: HornReader) {
  const packed = reader.take(8).readBigUInt64BE();
  return { x: Number(BigInt.asIntN(26, packed >> 38n)), y: Number(BigInt.asIntN(12, packed)),
    z: Number(BigInt.asIntN(26, packed >> 12n)) };
}

/** Exact ClientboundLevelEventPacket read/write, JukeboxPackets.javap.txt SHA256
 * 6beb2933a97084fe16e3b60269826d69b4e5b5b544e5792de0ced4d16607468d.
 * data is the RAW song registry ID for 1010, not holder ID+1 or a sound ID. */
export function decodeJukeboxLevelEvent(payload: Buffer, registry: SongRegistry) {
  const reader = new HornReader(payload);
  if (reader.varInt() !== 47) throw new Error('Expected level_event packet');
  const event = reader.int(), position = blockPosition(reader), data = reader.int(), global = reader.byte();
  if (global !== 0 && global !== 1) throw new Error('Invalid level_event boolean');
  reader.end();
  return { packet_id: 47, event, ...position, data, global_event: global === 1,
    ...(event === 1010 ? { song: songReference(registry, data + 1) } : {}),
    payload_bytes: payload.length, decoded_bytes: reader.offset };
}

/** UseItemOn composite: hand, BlockHitResult, sequence. Anonymous codec leaves
 * are pinned in JukeboxHit.javap.txt SHA256
 * 7d3da4caf09a68f9f60d90801fed6b6f24f0397b5ba3855e66f9d02cfa677a18.
 * Both hand and Direction use VarInt idMapper; hit offsets are BE floats. */
export function encodeJukeboxUse(hand: 0 | 1, x: number, y: number, z: number, sequence: number): Buffer {
  if (hand !== 0 && hand !== 1) throw new Error('Jukebox use requires actual main/offhand');
  if (![x, y, z].every(Number.isInteger) || x < -33554432 || x > 33554431 ||
      z < -33554432 || z > 33554431 || y < -2048 || y > 2047) throw new Error('Invalid jukebox block position');
  const position = Buffer.alloc(8), hit = Buffer.alloc(12);
  position.writeBigUInt64BE((BigInt.asUintN(26, BigInt(x)) << 38n) |
    (BigInt.asUintN(26, BigInt(z)) << 12n) | BigInt.asUintN(12, BigInt(y)));
  hit.writeFloatBE(0.5, 0); hit.writeFloatBE(1, 4); hit.writeFloatBE(0.5, 8);
  return Buffer.concat([vint(hand), position, vint(1), hit, Buffer.from([0, 0]), vint(sequence)]);
}

// Retain leaked signed elapsed exactly before the campaign's privacy assertion.
// This extension is confined to jukebox BE updates; the horn reader is unchanged.
class JukeboxUpdateReader extends HornReader {
  override nbt(type = this.byte(), depth = 0): unknown {
    if (type !== 4) return super.nbt(type, depth);
    if (++this.nodes > 512 || depth > 4) throw new Error('Jukebox NBT budget exceeded');
    return { nbt_type: 'Long', value: this.take(8).readBigInt64BE().toString() };
  }
}

/** BEData composite: packed position, raw registry VarInt, non-null compound.
 * Chunk BE entries separately permit absent NBT. Never conflate the two. */
export function decodeJukeboxBlockEntityPacket(payload: Buffer) {
  const reader = new JukeboxUpdateReader(payload);
  if (reader.varInt() !== 6) throw new Error('Expected block_entity_data packet');
  const position = blockPosition(reader), type_id = reader.count(2147483647);
  if (type_id !== 4) return null; // Other BE codecs are outside this projection.
  if (reader.byte() !== 10) throw new Error('Jukebox update requires non-null compound');
  const data = reader.nbt(10);
  reader.end();
  return { packet_id: 6, ...position, block_entity_type: type_id, data,
    payload_bytes: payload.length, decoded_bytes: reader.offset };
}
