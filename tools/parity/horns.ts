/** Opt-in horn fixture only. No generic component decoder and no inferred IDs.
 * Instrument reference IDs are resolved against this connection's captured
 * configuration registry. Unknown, inline and custom profiles fail explicitly. */
// Exact 26.3 HornPackets.javap.txt SHA256:
// 2794ae66b19ab782ded6528debe06fb2eb05cbdb1c1d1dc838f54e3655072ee5.
import type { ContainerPacket, PlainStack } from './client.ts';

export const HORN_NAMES = ['admire', 'call', 'dream', 'feel', 'ponder', 'seek', 'sing', 'yearn'] as const;
export type HornName = `minecraft:${typeof HORN_NAMES[number]}_goat_horn`;
export const HORN_ITEM = 1504;
const INSTRUMENT = 63;
const names: readonly string[] = HORN_NAMES.map(name => `minecraft:${name}_goat_horn`);

export function hornName(value: unknown): HornName {
  if (typeof value !== 'string' || !names.includes(value)) throw new Error('Unsupported instrument reference/profile');
  return value as HornName;
}

/** Standard Castagnoli CRC32C, not IEEE CRC32. */
export function crc32c(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0x82f63b78 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

/** HashOps.createString: tag12, UTF16 code-unit length LE32, UTF16LE units.
 * Identifier.CODEC canonical ASCII input is not a bare-ASCII CRC preimage.
 * This is a source-derived prediction; Java acceptance must be observed live. */
export function instrumentHashPreimage(name: HornName): Buffer {
  hornName(name);
  const prefix = Buffer.alloc(5);
  prefix[0] = 12;
  prefix.writeUInt32LE(name.length, 1);
  return Buffer.concat([prefix, Buffer.from(name, 'utf16le')]);
}

export function instrumentHash(name: HornName): number {
  return crc32c(instrumentHashPreimage(name));
}

type HornEntry = { registry_id: number; reference_id: number; identifier: HornName; data: Record<string, unknown> };
export type HornRegistry = { registry: 'minecraft:instrument'; entries: HornEntry[]; payload_bytes: number; decoded_bytes: number };

/** Validate names and all builtin properties, while preserving received order. */
export function hornRegistryEntries(values: { identifier: string; data: unknown }[]): HornEntry[] {
  if (values.length !== 8) throw new Error('Expected exactly eight builtin instruments');
  const seen = new Set<string>();
  const sounds: Record<HornName, number> = {
    'minecraft:ponder_goat_horn': 0, 'minecraft:sing_goat_horn': 1,
    'minecraft:seek_goat_horn': 2, 'minecraft:feel_goat_horn': 3,
    'minecraft:admire_goat_horn': 4, 'minecraft:call_goat_horn': 5,
    'minecraft:yearn_goat_horn': 6, 'minecraft:dream_goat_horn': 7,
  };
  return values.map(({ identifier: raw, data }, registry_id) => {
    const identifier = hornName(raw);
    if (seen.has(identifier)) throw new Error('Duplicate instrument registry entry');
    seen.add(identifier);
    if (!record(data) || Object.keys(data).some(key => !['sound_event', 'use_duration', 'range', 'durability_damage', 'description'].includes(key)) ||
        data.sound_event !== `minecraft:item.goat_horn.sound.${sounds[identifier]}` || data.use_duration !== 7 || data.range !== 256 ||
        (data.durability_damage !== undefined && data.durability_damage !== 0)) throw new Error('Unsupported custom instrument profile');
    const description = data.description;
    if (!record(description) || Object.keys(description).some(key => !['translate', 'type'].includes(key)) ||
        description.translate !== `instrument.minecraft.${identifier.slice('minecraft:'.length)}` ||
        (description.type !== undefined && description.type !== 'translatable')) throw new Error('Unsupported instrument description');
    return { registry_id, reference_id: registry_id + 1, identifier, data };
  });
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function referenceName(registry: HornRegistry, reference: number): HornName {
  if (!Number.isInteger(reference) || reference < 1 || reference > 8) throw new Error('Unknown/inline instrument holder unsupported');
  const entry = registry.entries[reference - 1];
  if (!entry || entry.registry_id !== reference - 1 || entry.reference_id !== reference) throw new Error('Unbound instrument holder');
  return hornName(entry.identifier);
}

/** Dedicated small reader: identifiers/fixture text are ASCII, NBT is limited to
 * the scalar/compound types actually used by these named instrument profiles. */
class HornReader {
  offset = 0;
  nodes = 0;
  readonly buffer: Buffer;
  constructor(buffer: Buffer) {
    if (buffer.length > 65536) throw new Error('Horn packet budget exceeded');
    this.buffer = buffer;
  }
  take(count: number): Buffer {
    if (!Number.isSafeInteger(count) || count < 0 || count > this.buffer.length - this.offset) throw new Error('Truncated horn packet');
    const value = this.buffer.subarray(this.offset, this.offset + count);
    this.offset += count;
    return value;
  }
  byte() { return this.take(1).readUInt8(); }
  int() { return this.take(4).readInt32BE(); }
  float() {
    const value = this.take(4).readFloatBE();
    if (!Number.isFinite(value)) throw new Error('Nonfinite horn float');
    return value;
  }
  varInt() {
    let value = 0;
    for (let index = 0; index < 5; index++) {
      const byte = this.byte();
      if (index === 4 && byte > 15) throw new Error('Horn VarInt overflow');
      value |= (byte & 127) << (index * 7);
      if (!(byte & 128)) return value;
    }
    throw new Error('Horn VarInt exceeds five bytes');
  }
  count(maximum: number) {
    const value = this.varInt();
    if (value < 0 || value > maximum) throw new Error('Invalid horn count/ID');
    return value;
  }
  ascii(length: number) {
    if (length < 0 || length > 256) throw new Error('Horn string budget exceeded');
    const bytes = this.take(length);
    if (bytes.some(byte => byte < 32 || byte > 126)) throw new Error('Unsupported horn string encoding');
    return bytes.toString('ascii');
  }
  string() { return this.ascii(this.varInt()); }
  identifier() {
    const value = this.string();
    if (!/^[a-z0-9_.-]+:[a-z0-9_./-]+$/u.test(value)) throw new Error('Noncanonical horn identifier');
    return value;
  }
  nbt(type = this.byte(), depth = 0): unknown {
    if (++this.nodes > 512 || depth > 4) throw new Error('Horn NBT budget exceeded');
    if (type === 1) return this.take(1).readInt8();
    if (type === 3) return this.int();
    if (type === 5) return this.float();
    if (type === 6) {
      const value = this.take(8).readDoubleBE();
      if (!Number.isFinite(value)) throw new Error('Nonfinite horn double');
      return value;
    }
    if (type === 8) return this.ascii(this.take(2).readUInt16BE());
    if (type !== 10) throw new Error(`Unsupported horn NBT type ${type}`);
    const result: Record<string, unknown> = {};
    for (let count = 0; count <= 16; count++) {
      const child = this.byte();
      if (child === 0) return result;
      if (count === 16) throw new Error('Horn compound field budget exceeded');
      const key = this.ascii(this.take(2).readUInt16BE());
      if (Object.hasOwn(result, key)) throw new Error('Duplicate horn NBT key');
      Object.defineProperty(result, key, { value: this.nbt(child, depth + 1), enumerable: true });
    }
    throw new Error('Unterminated horn compound');
  }
  end() {
    if (this.offset !== this.buffer.length) throw new Error('Trailing horn packet bytes');
  }
}

/** PackedRegistryEntry: Identifier + optional TAG; known-packs response is empty,
 * so every one of these eight entries must carry its actual profile NBT. */
export function decodeHornRegistryPacket(payload: Buffer): HornRegistry {
  const reader = new HornReader(payload);
  if (reader.varInt() !== 7 || reader.identifier() !== 'minecraft:instrument') throw new Error('Expected instrument registry packet');
  if (reader.count(8) !== 8) throw new Error('Expected eight sent instrument entries');
  const entries = Array.from({ length: 8 }, () => {
    const identifier = reader.identifier();
    if (reader.byte() !== 1) throw new Error('Missing/invalid instrument registry data optional');
    return { identifier, data: reader.nbt() };
  });
  reader.end();
  return { registry: 'minecraft:instrument', entries: hornRegistryEntries(entries), payload_bytes: payload.length, decoded_bytes: reader.offset };
}

function hornStack(reader: HornReader, registry: HornRegistry): PlainStack {
  const count = reader.count(1);
  if (count === 0) return null;
  const item_id = reader.varInt(), added = reader.count(3), removed = reader.count(1);
  if (item_id !== HORN_ITEM) throw new Error('Horn fixture received unsupported nonempty item');
  const components: Record<string, unknown> = {};
  const seen = new Set<number>();
  for (let index = 0; index < added; index++) {
    const id = reader.varInt();
    if (seen.has(id)) throw new Error('Duplicate horn component');
    seen.add(id);
    if (id === INSTRUMENT) components['minecraft:instrument'] = referenceName(registry, reader.varInt());
    else if (id === 3) components['minecraft:damage'] = reader.count(2147483647);
    else if (id === 0) {
      const data = reader.nbt();
      if (!record(data) || Object.keys(data).length !== 1 || data.instrument !== 'minecraft:sing_goat_horn') throw new Error('Unsupported custom_data control');
      components['minecraft:custom_data'] = data; // Opaque, never consulted for selection.
    } else if (id === 26) {
      const seconds = reader.float(), present = reader.byte();
      if (seconds !== 1 || present !== 1) throw new Error('Unsupported use_cooldown control');
      const cooldown_group = reader.identifier();
      if (cooldown_group !== 'test:horn_group') throw new Error('Unsupported explicit cooldown group');
      components['minecraft:use_cooldown'] = { seconds, cooldown_group };
    } else throw new Error(`Unsupported horn component ${id}`);
  }
  const removed_components: string[] = [];
  for (let index = 0; index < removed; index++) {
    const id = reader.varInt();
    if (id !== INSTRUMENT || seen.has(id)) throw new Error('Unsupported/conflicting horn removal');
    seen.add(id);
    removed_components.push('minecraft:instrument');
  }
  return { item_id, count, ...(added ? { components } : {}), ...(removed ? { removed_components } : {}) };
}

/** Player46 and single chest63 only. Open-screen remains the existing text probe. */
export function decodeHornContainerPacket(payload: Buffer, registry: HornRegistry): ContainerPacket {
  const reader = new HornReader(payload);
  const packet_id = reader.varInt(), container_id = reader.count(255), state_id = reader.count(32767);
  let result: ContainerPacket;
  const size = container_id === 0 ? 46 : 63;
  if (packet_id === 18) {
    if (reader.count(63) !== size) throw new Error('Wrong horn inventory size');
    const slots = Array.from({ length: size }, () => hornStack(reader, registry));
    result = { kind: 'content', packet_id, container_id, state_id, slots, carried: hornStack(reader, registry) };
  } else if (packet_id === 20) {
    const slot = reader.take(2).readInt16BE();
    if (slot < 0 || slot >= size) throw new Error('Horn slot outside inventory');
    result = { kind: 'slot', packet_id, container_id, state_id, slot, stack: hornStack(reader, registry) };
  } else throw new Error('Expected horn content/slot packet');
  reader.end();
  return result;
}

export function decodeHornCooldownPacket(payload: Buffer) {
  const reader = new HornReader(payload);
  if (reader.varInt() !== 22) throw new Error('Expected horn cooldown packet');
  const group = reader.identifier(), ticks = reader.count(1200);
  if (!['minecraft:goat_horn', 'test:horn_group'].includes(group)) throw new Error('Unsupported horn cooldown group');
  reader.end();
  return { packet_id: 22, group, ticks, payload_bytes: payload.length, decoded_bytes: reader.offset };
}

/** Exact ClientboundSoundEntityPacket read/write, EntitySound.javap.txt SHA256
 * 7984142429e6b28b3c036b66876d9d03d75781352bd319c2868be463ab47338b.
 * Reference SoundEvent only. Resolve the numeric ID using the pinned static
 * sound registry and the actual sent instrument profile's sound_event name. */
export function decodeHornEntitySoundPacket(payload: Buffer) {
  const reader = new HornReader(payload);
  if (reader.varInt() !== 118) throw new Error('Expected entity sound packet');
  const reference = reader.count(65536);
  if (reference === 0) throw new Error('Inline entity sound unsupported');
  const source = reader.count(9), entity_id = reader.count(2147483647), volume = reader.float(), pitch = reader.float();
  if (volume < 0 || pitch < 0) throw new Error('Invalid entity sound volume/pitch');
  const seed = reader.take(8).toString('hex');
  reader.end();
  return { packet_id: 118, registry_id: reference - 1, source, entity_id, volume, pitch, seed,
    payload_bytes: payload.length, decoded_bytes: reader.offset };
}

/** Header only: Login.javap.txt d08d3a1429c2494b1aaf8e7f4aef39bc731d4692e3db9cafd1cb8cba6e932ee0
 * STREAM_CODEC first pair is ByteBufCodecs.INT / playerId (bootstrap1).
 * Deliberately retain, rather than decode, the remaining SpawnInfo/transitive
 * fields. This associates the source entity; it is not full Login validation. */
export function decodeHornLoginHeader(payload: Buffer) {
  const reader = new HornReader(payload);
  if (reader.varInt() !== 50) throw new Error('Expected PLAY Login packet');
  const entity_id = reader.int();
  if (entity_id < 0) throw new Error('Unsupported negative Login entity ID');
  return { packet_id: 50, entity_id, header_only: true, header_bytes: reader.offset,
    payload_bytes: payload.length, undecoded_bytes: payload.length - reader.offset,
    undecoded_base64: payload.subarray(reader.offset).toString('base64'), payload_base64: payload.toString('base64') };
}

function vint(value: number): Buffer {
  if (!Number.isInteger(value) || value < 0 || value > 2147483647) throw new Error('Invalid horn integer');
  const bytes = [];
  do {
    const byte = value & 127;
    value >>>= 7;
    bytes.push(byte | (value ? 128 : 0));
  } while (value);
  return Buffer.from(bytes);
}

function short(value: number): Buffer {
  const bytes = Buffer.alloc(2);
  bytes.writeInt16BE(value);
  return bytes;
}

export function encodeHornSelect(hotbar: number): Buffer {
  if (!Number.isInteger(hotbar) || hotbar < 0 || hotbar > 8) throw new Error('Invalid horn hotbar slot');
  return short(hotbar);
}

export function encodeHornUse(hand: number, sequence: number, yaw: number, pitch: number): Buffer {
  if (hand !== 0 && hand !== 1) throw new Error('Invalid horn hand');
  if (![yaw, pitch].every(Number.isFinite) || Math.abs(yaw) > 360 || Math.abs(pitch) > 90) throw new Error('Invalid horn rotation');
  const angles = Buffer.alloc(8);
  angles.writeFloatBE(yaw, 0);
  angles.writeFloatBE(pitch, 4);
  return Buffer.concat([vint(hand), vint(sequence), angles]);
}

/** HashedStack optional, raw item registry ID, count, added map, removed set.
 * Only instrument hashes are implemented. Opaque/cooldown controls never click. */
export function encodeHornStackHash(stack: PlainStack): Buffer {
  if (stack === null) return Buffer.from([0]);
  if (stack.item_id !== HORN_ITEM || stack.count !== 1) throw new Error('Unsupported horn hash stack');
  const added = Object.entries(stack.components ?? {}), removed = stack.removed_components ?? [];
  if (added.length > 1 || removed.length > 1 || (added.length && removed.length) ||
      removed.some(name => name !== 'minecraft:instrument')) throw new Error('Unsupported horn hash patch');
  const hashes: Buffer[] = [];
  for (const [name, value] of added) {
    if (name !== 'minecraft:instrument') throw new Error('Only instrument component hashes are supported');
    const bytes = Buffer.alloc(4);
    bytes.writeUInt32BE(instrumentHash(hornName(value)));
    hashes.push(vint(INSTRUMENT), bytes);
  }
  return Buffer.concat([Buffer.from([1]), vint(stack.item_id), vint(stack.count), vint(added.length), ...hashes,
    vint(removed.length), ...removed.map(() => vint(INSTRUMENT))]);
}

/** Predict only the two changed slots. Never mutate observed menu/player state. */
export function encodeHornSwap(container: number, state: number, slot: number, hotbar: number, slots: PlainStack[]): Buffer {
  if (!Number.isInteger(container) || container < 1 || container > 255 || !Number.isInteger(state) || state < 0 || state > 32767 ||
      !Number.isInteger(slot) || slot < 0 || slot > 26 || slots.length !== 63) throw new Error('Invalid horn SWAP menu');
  encodeHornSelect(hotbar);
  const target = 54 + hotbar;
  return Buffer.concat([vint(container), vint(state), short(slot), Buffer.from([hotbar]), vint(2), vint(2),
    short(slot), encodeHornStackHash(slots[target]!), short(target), encodeHornStackHash(slots[slot]!), Buffer.from([0])]);
}
