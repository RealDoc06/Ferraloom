/** A focused protocol 777 probe. IDs come from the official 26.3 packet report;
 * selected field layouts were checked against that exact server's JVM codecs.
 * Unhandled packets are counted, never interpreted using an older schema. */
import { createHash } from 'node:crypto';
import { createConnection } from 'node:net';
import { createInterface } from 'node:readline';
import { parseArgs } from 'node:util';
import { inflateSync, deflateSync } from 'node:zlib';

export const VERSION = '26.3';
export const PROTOCOL = 777;
const MAX_PACKET = 8 * 1024 * 1024;
const ids = {
  config: { finish: 3, keepAlive: 4, ping: 5, registry: 7, knownPacks: 15, conduct: 20 },
  play: { commands: 16, chunk: 46, login: 50, position: 73, health: 106, chat: 124, disguisedChat: 33, playerChat: 66, keepAlive: 45, ping: 62,
    inventory: 18, slot: 20, block: 8, chunkBatch: 11, disconnect: 32 },
} as const;

export function varInt(value: number): Buffer {
  let remaining = value >>> 0;
  const bytes: number[] = [];
  do {
    const byte = remaining & 127;
    remaining >>>= 7;
    bytes.push(byte | (remaining ? 128 : 0));
  } while (remaining);
  return Buffer.from(bytes);
}

export function readVarInt(buffer: Buffer, start = 0): { value: number; bytes: number } | undefined {
  let value = 0;
  for (let i = 0; i < 5; i++) {
    if (start + i >= buffer.length) return undefined;
    const byte = buffer[start + i]!;
    if (i === 4 && byte > 15) throw new Error('VarInt overflows 32 bits');
    value |= (byte & 127) << (i * 7);
    if (!(byte & 128)) return { value, bytes: i + 1 };
  }
  throw new Error('VarInt exceeds five bytes');
}

class Reader {
  offset = 0;
  nbtNodes = 0;
  buffer: Buffer;
  constructor(buffer: Buffer) { this.buffer = buffer; }
  take(count: number): Buffer {
    if (!Number.isSafeInteger(count) || count < 0 || count > this.buffer.length - this.offset) throw new Error('Truncated packet');
    const result = this.buffer.subarray(this.offset, this.offset + count);
    this.offset += count;
    return result;
  }
  int() { return this.take(4).readInt32BE(); }
  byte() { return this.take(1).readUInt8(); }
  float() { return this.take(4).readFloatBE(); }
  double() { return this.take(8).readDoubleBE(); }
  varInt() {
    const result = readVarInt(this.buffer, this.offset);
    if (!result) throw new Error('Truncated VarInt');
    this.offset += result.bytes;
    return result.value;
  }
  string() { return this.take(this.varInt()).toString('utf8'); }
}

export type PlainStack = { item_id: number; count: number; components?: Record<string, unknown> } | null;
export type ContainerPacket =
  | { kind: 'open'; packet_id: 60; container_id: number; menu_type: number; title: string;
      title_component?: unknown; title_nbt?: unknown }
  | { kind: 'content'; packet_id: 18; container_id: number; state_id: number; slots: PlainStack[]; carried: PlainStack }
  | { kind: 'slot'; packet_id: 20; container_id: number; state_id: number; slot: number; stack: PlainStack };

export type PlayerInventoryObservation = { state_id: number; sequence: number; slots: PlainStack[] };

/** Keep received player slots separate from container slots and local predictions. */
export function observePlayerInventory(current: PlayerInventoryObservation | null,
  packet: ContainerPacket & { sequence: number }): PlayerInventoryObservation | null {
  if (packet.container_id !== 0 || packet.kind === 'open') return current;
  if (packet.kind === 'content') {
    if (packet.slots.length !== 46) throw new Error('Expected 46 player inventory slots');
    return { state_id: packet.state_id, sequence: packet.sequence, slots: packet.slots.slice() };
  }
  if (current === null) return null; // A delta cannot establish the other slots.
  if (packet.slot >= current.slots.length) throw new Error('Update outside player inventory');
  const slots = current.slots.slice();
  slots[packet.slot] = packet.stack;
  return { state_id: packet.state_id, sequence: packet.sequence, slots };
}

function plainStack(reader: Reader, metadata = false): PlainStack {
  const count = reader.varInt();
  if (count === 0) return null;
  if (count < 0) throw new Error('Negative item count');
  const item_id = reader.varInt(), added = reader.varInt(), removed = reader.varInt();
  if (item_id <= 0) throw new Error('Invalid nonempty item ID');
  // The acceptance fixtures deliberately use plain stacks. Component payloads
  // are not length-prefixed in this trusted codec; guessing their size is unsafe.
  if (removed !== 0 || added < 0 || added > 2 || (!metadata && added !== 0)) throw new Error(`Unsupported item component patch: ${added}/${removed}`);
  const components: Record<string, unknown> = {};
  for (let index = 0; index < added; index++) {
    // Official 26.3 registries + DataComponents lambda$static${6,9}:
    // both names use ComponentSerialization.STREAM_CODEC (network NBT).
    const id = reader.varInt();
    const name = id === 6 ? 'minecraft:custom_name' : id === 9 ? 'minecraft:item_name' : null;
    if (!name || Object.hasOwn(components, name)) throw new Error(`Unsupported/duplicate item component ${id}; raw packet retained by caller`);
    components[name] = nbt(reader);
  }
  return { item_id, count, ...(added ? { components } : {}) };
}

/** Exact 26.3 menu codecs; unsupported item components fail the focused probe. */
export function decodeContainerPacket(payload: Buffer, metadata = false): ContainerPacket {
  const reader = new Reader(payload);
  const packet_id = reader.varInt(), container_id = reader.varInt();
  if (container_id < 0 || container_id > 255) throw new Error('Invalid container ID');
  let result: ContainerPacket;
  if (packet_id === 60) {
    const menu_type = reader.varInt();
    if (menu_type < 0) throw new Error('Invalid menu type');
    const title_nbt = nbt(reader);
    result = { kind: 'open', packet_id, container_id, menu_type, title: text(title_nbt),
      ...(metadata ? { title_component: decodeContainerTitle(title_nbt), title_nbt } : {}) };
  } else if (packet_id === 18) {
    const state_id = reader.varInt(), count = reader.varInt();
    if (state_id < 0 || count < 0 || count > 4096) throw new Error('Invalid content state or slot count');
    const slots = Array.from({ length: count }, () => plainStack(reader, metadata));
    result = { kind: 'content', packet_id, container_id, state_id, slots, carried: plainStack(reader, metadata) };
  } else if (packet_id === 20) {
    const state_id = reader.varInt(), slot = reader.take(2).readInt16BE();
    if (state_id < 0 || slot < 0 || slot >= 4096) throw new Error('Invalid slot state or index');
    result = { kind: 'slot', packet_id, container_id, state_id, slot, stack: plainStack(reader, metadata) };
  } else throw new Error(`Unsupported container packet ${packet_id}`);
  if (reader.offset !== payload.length) throw new Error('Trailing container packet bytes');
  return result;
}

function packedBlockPos(x: number, y: number, z: number): Buffer {
  if (![x, y, z].every(Number.isSafeInteger) || Math.abs(x) >= 33554432 || Math.abs(z) >= 33554432 || y < -2048 || y > 2047) {
    throw new Error('Block position outside packed coordinate bounds');
  }
  const packed = (BigInt.asUintN(26, BigInt(x)) << 38n) | (BigInt.asUintN(26, BigInt(z)) << 12n) | BigInt.asUintN(12, BigInt(y));
  const buffer = Buffer.alloc(8);
  buffer.writeBigUInt64BE(packed);
  return buffer;
}

export function encodeUseItemOn(x: number, y: number, z: number, sequence: number, face = 1): Buffer {
  if (!Number.isSafeInteger(sequence) || sequence < 0 || sequence > 2147483647) throw new Error('Invalid interaction sequence');
  if (![1, 2, 3].includes(face)) throw new Error('Probe supports UP/NORTH/SOUTH hit faces');
  return Buffer.concat([varInt(0), packedBlockPos(x, y, z), varInt(face),
    floats(0.5, face === 1 ? 1 : 0.5, face === 2 ? 0 : face === 3 ? 1 : 0.5), Buffer.from([0, 0]), varInt(sequence)]);
}

/** 26.3 PlayerAction SWAP_ITEM_WITH_OFFHAND is ordinal 7 (STAB is 8). */
export function encodeSwapHands(sequence: number): Buffer {
  if (!Number.isSafeInteger(sequence) || sequence < 0 || sequence > 2147483647) throw new Error('Invalid swap-hands sequence');
  return Buffer.concat([varInt(7), packedBlockPos(0, 0, 0), Buffer.from([0]), varInt(sequence)]);
}

export type SignSlot = 0 | 1; // 26.3 SignTextSlot: BACK, FRONT.
type BlockPosition = { x: number; y: number; z: number };
export type SignFace = { messages: string[]; color: string; has_glowing_text: boolean };
export type SignData = { front_text: SignFace; back_text: SignFace; is_waxed: boolean };
/** Selected literal/style/click fixtures only, not a renderer or general component codec. */
export type LiteralComponent = { text: string; type?: 'text'; bold?: boolean; color?: string;
  click_event?: { action: 'run_command'; command: string } };
export type ObservedComponent = string | LiteralComponent;
export type RichSignFace = { messages: ObservedComponent[]; filtered_messages: ObservedComponent[];
  color: string; has_glowing_text: boolean };
export type RichSignData = { front_text: RichSignFace; back_text: RichSignFace;
  is_waxed: boolean; allow_op_features: boolean };
export type SignObservation = BlockPosition & (
  | { kind: 'editor'; packet_id: 61; slot: SignSlot }
  | { kind: 'block_entity' | 'chunk'; packet_id: 6 | 46; block_entity_type: 7 | 8; data: SignData | null;
      rich_data?: RichSignData; component_nbt?: unknown }
);

function signLines(value: unknown): string[] {
  if (!Array.isArray(value) || value.length !== 4 || !value.every((line): line is string =>
    typeof line === 'string' && line.length <= 384 && !/[\u0000\r\n]/u.test(line))) {
    throw new Error('Sign probe supports exactly four bounded plain component strings; rich/filter components are unmodeled');
  }
  return value;
}

/** The official SignText codec uses ComponentSerialization.CODEC, whose plain
 * branch is a direct NBT string. Never JSON.parse a sign message. */
export function decodeSignData(value: unknown): SignData {
  if (!record(value)) throw new Error('Expected sign compound');
  const boolean = (value: unknown) => {
    if (value === undefined) return false;
    if (value !== 0 && value !== 1) throw new Error('Invalid sign boolean');
    return value === 1;
  };
  const face = (value: unknown): SignFace => {
    if (!record(value)) throw new Error('Missing sign face');
    const messages = signLines(value.messages);
    if (value.filtered_messages !== undefined && JSON.stringify(signLines(value.filtered_messages)) !== JSON.stringify(messages)) {
      throw new Error('Filtered sign messages are unmodeled');
    }
    const color = value.color ?? 'black';
    if (typeof color !== 'string' || !['white', 'orange', 'magenta', 'light_blue', 'yellow', 'lime', 'pink', 'gray',
      'light_gray', 'cyan', 'purple', 'blue', 'brown', 'green', 'red', 'black'].includes(color)) throw new Error('Invalid sign dye color');
    return { messages, color, has_glowing_text: boolean(value.has_glowing_text) };
  };
  return { front_text: face(value.front_text), back_text: face(value.back_text), is_waxed: boolean(value.is_waxed) };
}

function signBoolean(value: unknown, path: string): boolean {
  if (value === undefined) return false;
  if (value !== 0 && value !== 1) throw new Error(`${path}: expected NBT byte boolean`);
  return value === 1;
}

function knownFields(value: Record<string, unknown>, fields: string[], path: string) {
  for (const field of Object.keys(value)) if (!fields.includes(field)) throw new Error(`${path}.${field}: unsupported component field`);
}

/** Unwrap EXACTLY one ListTag.write scalar envelope, only at a component list entry.
 * Keep the untouched decoded NBT alongside this selected semantic observation. */
function observedComponent(entry: unknown, path: string): ObservedComponent {
  const value = record(entry) && Object.keys(entry).length === 1 && Object.hasOwn(entry, '') ? entry[''] : entry;
  if (typeof value === 'string') return value;
  if (!record(value)) throw new Error(`${path}: unsupported component kind`);
  knownFields(value, ['text', 'type', 'bold', 'color', 'click_event'], path);
  if (typeof value.text !== 'string' || (value.type !== undefined && value.type !== 'text')) throw new Error(`${path}: expected literal text component`);
  const result: LiteralComponent = { text: value.text };
  if (value.type === 'text') result.type = value.type;
  if (value.bold !== undefined) result.bold = signBoolean(value.bold, `${path}.bold`);
  if (value.color !== undefined) {
    if (typeof value.color !== 'string' || !/^(?:black|dark_blue|dark_green|dark_aqua|dark_red|dark_purple|gold|gray|dark_gray|blue|green|aqua|red|light_purple|yellow|white|#[0-9a-fA-F]{6})$/u.test(value.color)) throw new Error(`${path}.color: unsupported text color`);
    result.color = value.color;
  }
  if (value.click_event !== undefined) {
    const click = value.click_event;
    if (!record(click)) throw new Error(`${path}.click_event: expected compound`);
    knownFields(click, ['action', 'command'], `${path}.click_event`);
    if (click.action !== 'run_command' || typeof click.command !== 'string' || !click.command || click.command.length > 32767) throw new Error(`${path}.click_event: unsupported action/command`);
    result.click_event = { action: click.action, command: click.command };
  }
  return result;
}

export function decodeRichSignData(value: unknown): RichSignData {
  if (!record(value)) throw new Error('Expected rich sign compound');
  const face = (value: unknown, path: string): RichSignFace => {
    if (!record(value)) throw new Error(`${path}: missing face`);
    knownFields(value, ['messages', 'filtered_messages', 'color', 'has_glowing_text'], path);
    const lines = (value: unknown, path: string): ObservedComponent[] => {
      if (!Array.isArray(value) || value.length !== 4) throw new Error(`${path}: expected four components`);
      return value.map((line: unknown, index) => observedComponent(line, `${path}[${index}]`));
    };
    const messages = lines(value.messages, `${path}.messages`), color = value.color ?? 'black';
    if (typeof color !== 'string' || !['white', 'orange', 'magenta', 'light_blue', 'yellow', 'lime', 'pink', 'gray',
      'light_gray', 'cyan', 'purple', 'blue', 'brown', 'green', 'red', 'black'].includes(color)) throw new Error(`${path}.color: invalid dye`);
    return { messages, filtered_messages: value.filtered_messages === undefined ? messages : lines(value.filtered_messages, `${path}.filtered_messages`),
      color, has_glowing_text: signBoolean(value.has_glowing_text, `${path}.has_glowing_text`) };
  };
  return { front_text: face(value.front_text, 'front_text'), back_text: face(value.back_text, 'back_text'),
    is_waxed: signBoolean(value.is_waxed, 'is_waxed'), allow_op_features: signBoolean(value.allow_op_features, 'allow_op_features') };
}

function signObservationData(value: unknown, mode: 'plain' | 'rich', position: BlockPosition) {
  if (mode === 'plain') return { data: decodeSignData(value) };
  let rich_data: RichSignData;
  try { rich_data = decodeRichSignData(value); }
  catch (error) { throw new Error(`Sign (${position.x},${position.y},${position.z}): ${error instanceof Error ? error.message : String(error)}`); }
  // Existing plain observations retain their strict representation. Rich fixtures
  // expose no flattened substitute that could accidentally pass a plain assertion.
  let data: SignData | null = null;
  try { data = decodeSignData(value); } catch { /* Validated rich fixture has no plain representation. */ }
  return { data, rich_data, component_nbt: value };
}

function blockPosition(reader: Reader): BlockPosition {
  const packed = reader.take(8).readBigUInt64BE();
  return { x: Number(BigInt.asIntN(26, packed >> 38n)), y: Number(BigInt.asIntN(12, packed)),
    z: Number(BigInt.asIntN(26, packed >> 12n)) };
}

export function encodeSignUpdate(x: number, y: number, z: number, messages: string[], slot: SignSlot): Buffer {
  if (slot !== 0 && slot !== 1) throw new Error('Invalid SignTextSlot');
  return Buffer.concat([packedBlockPos(x, y, z), ...signLines(messages).map(string), varInt(slot)]);
}

/** Decode the complete editor/BEData packet. Non-sign BE data is consumed but
 * deliberately not interpreted as sign state. */
export function decodeSignPacket(payload: Buffer, mode: 'plain' | 'rich' = 'plain'): SignObservation | null {
  if (payload.length > MAX_PACKET) throw new Error('Sign packet exceeds probe budget');
  const reader = new Reader(payload), packet_id = reader.varInt(), position = blockPosition(reader);
  let result: SignObservation | null;
  if (packet_id === 61) {
    const slot = reader.varInt();
    if (slot !== 0 && slot !== 1) throw new Error('Invalid SignTextSlot');
    result = { kind: 'editor', packet_id, ...position, slot };
  } else if (packet_id === 6) {
    const block_entity_type = reader.varInt();
    if (block_entity_type < 0) throw new Error('Invalid block entity type');
    if (reader.byte() !== 10) throw new Error('BEData requires network compound NBT');
    const data = nbt(reader, 10);
    result = block_entity_type === 7 || block_entity_type === 8
      ? { kind: 'block_entity', packet_id, ...position, block_entity_type, ...signObservationData(data, mode, position) } : null;
  } else throw new Error(`Unsupported sign packet ${packet_id}`);
  if (reader.offset !== payload.length) throw new Error('Trailing sign packet bytes');
  return result;
}

function boundedCount(reader: Reader, maximum: number, label: string): number {
  const value = reader.varInt();
  if (value < 0 || value > maximum) throw new Error(`Invalid ${label} (probe budget ${maximum})`);
  return value;
}

/** Read chunk metadata only, skipping length-delimited terrain and light data.
 * Optional compound NBT is TAG_End or TAG_Compound, without a presence boolean. */
export function decodeChunkBlockEntities(payload: Buffer, mode: 'plain' | 'rich' = 'plain') {
  if (payload.length > MAX_PACKET) throw new Error('Chunk exceeds probe budget');
  const reader = new Reader(payload);
  if (reader.varInt() !== 46) throw new Error('Expected chunk packet');
  const x = reader.int(), z = reader.int();
  const heightmaps = boundedCount(reader, 6, 'heightmap count');
  const seen = new Set<number>();
  for (let index = 0; index < heightmaps; index++) {
    const type = boundedCount(reader, 5, 'heightmap type');
    if (seen.has(type)) throw new Error('Duplicate heightmap');
    seen.add(type);
    reader.take(boundedCount(reader, 1024, 'heightmap longs') * 8);
  }
  const section_bytes = boundedCount(reader, 2097152, 'chunk section bytes');
  reader.take(section_bytes);
  const block_entity_count = boundedCount(reader, 4096, 'chunk block entity count');
  const signs: SignObservation[] = [];
  const block_entities: (BlockPosition & { block_entity_type: number; data: unknown })[] = [];
  for (let index = 0; index < block_entity_count; index++) {
    const local = reader.byte(), y = reader.take(2).readInt16BE();
    const block_entity_type = boundedCount(reader, 65535, 'block entity type');
    const tag = reader.byte();
    if (tag !== 0 && tag !== 10) throw new Error('Chunk BE requires optional compound NBT');
    const data = tag === 0 ? null : nbt(reader, 10);
    const position = { x: x * 16 + (local >> 4), y, z: z * 16 + (local & 15) };
    block_entities.push({ ...position, block_entity_type, data });
    if (block_entity_type === 7 || block_entity_type === 8) {
      signs.push({ kind: 'chunk', packet_id: 46, ...position, block_entity_type, ...signObservationData(data, mode, position) });
    }
  }
  // 26.3 ByteBufCodecs$15 encodes BIT_SET using BitSet.toByteArray(), not LONG_ARRAY.
  // ClientboundLightUpdatePacketData contains four such masks, then two byte-array lists.
  for (let index = 0; index < 4; index++) reader.take(boundedCount(reader, 512, 'light bitset bytes'));
  for (let kind = 0; kind < 2; kind++) {
    const count = boundedCount(reader, 256, 'light array count');
    for (let index = 0; index < count; index++) reader.take(boundedCount(reader, 2048, 'light array bytes'));
  }
  if (reader.offset !== payload.length) throw new Error('Trailing chunk bytes outside supported metadata/light schema');
  return { x, z, section_bytes, block_entity_count, block_entities, signs, decoded_bytes: reader.offset };
}

function plainStackHash(stack: PlainStack): Buffer {
  if (stack === null) return Buffer.from([0]);
  if (stack.components) throw new Error('SWAP hashed components are outside this plain-stack fixture');
  if (!Number.isSafeInteger(stack.item_id) || stack.item_id <= 0 || stack.item_id > 2147483647 ||
      !Number.isSafeInteger(stack.count) || stack.count <= 0 || stack.count > 2147483647) throw new Error('Invalid plain stack hash');
  // HashedStack optional presence, registry item, count, added hash map, removed set.
  return Buffer.concat([Buffer.from([1]), varInt(stack.item_id), varInt(stack.count), varInt(0), varInt(0)]);
}

/** Predict a vanilla hotbar SWAP without treating that prediction as a received update. */
export function encodeContainerSwap(container_id: number, state_id: number, slot: number, hotbar: number,
  slots: PlainStack[]): Buffer {
  if (!Number.isInteger(container_id) || container_id < 1 || container_id > 255 ||
      !Number.isInteger(state_id) || state_id < 0 || state_id > 32767 ||
      slots.length !== 63 || !Number.isInteger(slot) || slot < 0 || slot >= 27 ||
      !Number.isInteger(hotbar) || hotbar < 0 || hotbar > 8) throw new Error('SWAP requires a synchronized single-container menu');
  const hotbarSlot = 54 + hotbar;
  const short = (value: number) => { const bytes = Buffer.alloc(2); bytes.writeInt16BE(value); return bytes; };
  return Buffer.concat([varInt(container_id), varInt(state_id), short(slot), Buffer.from([hotbar]), varInt(2), varInt(2),
    short(slot), plainStackHash(slots[hotbarSlot]!), short(hotbarSlot), plainStackHash(slots[slot]!), plainStackHash(null)]);
}

// Exact 26.3 generated/reports/registries.json minecraft:command_argument_type protocol IDs.
// Layouts: ClientboundCommandsPacket.{readNode,read,Entry.write}, ArgumentTypeInfos.bootstrap,
// the 13 non-singleton ArgumentTypeInfo serializers, and FriendlyByteBuf registry/identifier codecs.
// 26.3 inner server SHA256: a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6.
export const COMMAND_ARGUMENT_TYPES = [
  'brigadier:bool', 'brigadier:float', 'brigadier:double', 'brigadier:integer', 'brigadier:long', 'brigadier:string',
  'minecraft:entity', 'minecraft:game_profile', 'minecraft:block_pos', 'minecraft:column_pos', 'minecraft:vec3',
  'minecraft:vec2', 'minecraft:block_state', 'minecraft:block_predicate', 'minecraft:item_stack', 'minecraft:item_predicate',
  'minecraft:team_color', 'minecraft:hex_color', 'minecraft:component', 'minecraft:style', 'minecraft:message',
  'minecraft:nbt_compound_tag', 'minecraft:nbt_tag', 'minecraft:nbt_path', 'minecraft:objective',
  'minecraft:objective_criteria', 'minecraft:operation', 'minecraft:particle', 'minecraft:angle', 'minecraft:rotation',
  'minecraft:scoreboard_slot', 'minecraft:score_holder', 'minecraft:swizzle', 'minecraft:team', 'minecraft:item_slot',
  'minecraft:item_slots', 'minecraft:resource_location', 'minecraft:function', 'minecraft:entity_anchor',
  'minecraft:int_range', 'minecraft:float_range', 'minecraft:dimension', 'minecraft:gamemode', 'minecraft:time',
  'minecraft:resource_or_tag', 'minecraft:resource_or_tag_key', 'minecraft:resource', 'minecraft:resource_key',
  'minecraft:resource_selector', 'minecraft:template_mirror', 'minecraft:template_rotation', 'minecraft:heightmap',
  'minecraft:loot_table', 'minecraft:loot_predicate', 'minecraft:loot_modifier', 'minecraft:context_float_provider',
  'minecraft:context_int_provider', 'minecraft:slot_source', 'minecraft:dialog', 'minecraft:feature',
  'minecraft:swing_animation', 'minecraft:uuid',
] as const;

type ArgumentProperties =
  | { kind: 'none' }
  | { kind: 'number'; flags: number; min: number | string | null; max: number | string | null }
  | { kind: 'string'; mode: 'word' | 'phrase' | 'greedy' }
  | { kind: 'entity'; single: boolean; players_only: boolean }
  | { kind: 'score_holder'; multiple: boolean }
  | { kind: 'time'; minimum: number }
  | { kind: 'registry'; registry: string };

type CommandNode = {
  type: 'root' | 'literal' | 'argument'; flags: number; executable: boolean; restricted: boolean;
  children: number[]; redirect: number | null; name: string | null;
  parser_id: number | null; parser: typeof COMMAND_ARGUMENT_TYPES[number] | null;
  properties: ArgumentProperties | null; suggestion_provider: string | null;
};

const COMMAND_LIMITS = { nodes: 16384, edges: 131072, paths: 65536, path_characters: 2 * 1024 * 1024, depth: 128 } as const;

function commandCount(reader: Reader, maximum: number, label: string): number {
  const count = reader.varInt();
  if (count < 0 || count > maximum) throw new Error(`Invalid command ${label}: ${count}`);
  return count;
}

function commandString(reader: Reader): string {
  // FriendlyByteBuf.readUtf(): at most 32767 UTF-16 code units and 3*32767 UTF-8 bytes.
  const bytes = reader.take(commandCount(reader, 32767 * 3, 'string byte length'));
  // Java's UTF_8 decoder consumes a surrogate's ED A0..BF prefix as one malformed
  // sequence, including a missing/non-continuation third byte. Node replaces its
  // individual bytes instead, so normalize that case before using Node's decoder.
  const parts: string[] = [];
  let start = 0;
  for (let index = 0; index + 1 < bytes.length; index++) {
    if (bytes[index] === 0xed && bytes[index + 1]! >= 0xa0 && bytes[index + 1]! <= 0xbf) {
      parts.push(bytes.subarray(start, index).toString('utf8'), '\uFFFD');
      index += index + 2 < bytes.length && (bytes[index + 2]! & 0xc0) === 0x80 ? 2 : 1;
      start = index + 1;
    }
  }
  parts.push(bytes.subarray(start).toString('utf8'));
  const value = parts.join('');
  if (value.length > 32767) throw new Error('Command string exceeds UTF-16 length limit');
  return value;
}

function commandIdentifier(reader: Reader): string {
  const value = commandString(reader);
  // Identifier.parse permits the omitted/empty namespace and supplies minecraft.
  const separator = value.indexOf(':');
  const namespace = separator < 0 ? 'minecraft' : value.slice(0, separator) || 'minecraft';
  const path = separator < 0 ? value : value.slice(separator + 1);
  if (!/^[a-z0-9_.-]+$/u.test(namespace) || !/^[a-z0-9/._-]*$/u.test(path)) throw new Error('Invalid command resource identifier');
  return `${namespace}:${path}`;
}

function argumentProperties(reader: Reader, id: number): ArgumentProperties {
  if (id >= 1 && id <= 4) {
    const flags = reader.byte();
    const number = () => {
      if (id === 4) return reader.take(8).readBigInt64BE();
      return id === 1 ? reader.float() : id === 2 ? reader.double() : reader.int();
    };
    const min = flags & 1 ? number() : null;
    const max = flags & 2 ? number() : null;
    // Preserve signed-i64 limits and Java-accepted non-finite bounds across JSON IPC.
    return { kind: 'number', flags,
      min: typeof min === 'bigint' || (typeof min === 'number' && !Number.isFinite(min)) ? String(min) : min,
      max: typeof max === 'bigint' || (typeof max === 'number' && !Number.isFinite(max)) ? String(max) : max };
  }
  if (id === 5) {
    const mode = ['word', 'phrase', 'greedy'] as const;
    return { kind: 'string', mode: mode[commandCount(reader, 2, 'string mode')]! };
  }
  if (id === 6 || id === 31) {
    const flags = reader.byte();
    return id === 6 ? { kind: 'entity', single: Boolean(flags & 1), players_only: Boolean(flags & 2) }
      : { kind: 'score_holder', multiple: Boolean(flags & 1) };
  }
  if (id === 43) return { kind: 'time', minimum: reader.int() };
  if (id >= 44 && id <= 48) return { kind: 'registry', registry: commandIdentifier(reader) };
  // All remaining registered IDs use SingletonArgumentInfo, whose network serializer writes zero bytes.
  return { kind: 'none' };
}

/** Decode a complete commands packet body (without packet ID), with no parser or byte skipping.
 * Child and redirect graphs are validated separately, matching vanilla's validateEntries checks;
 * a combined cycle such as execute -> run -> root is valid and never expanded into paths. */
export function decodeCommandTree(payload: Buffer) {
  if (payload.length > MAX_PACKET) throw new Error('Command packet exceeds probe support budget');
  const reader = new Reader(payload);
  const count = commandCount(reader, COMMAND_LIMITS.nodes, 'node count (probe support budget)');
  if (!count) throw new Error('Empty command tree');
  const nodes: CommandNode[] = [];
  let edges = 0;
  for (let index = 0; index < count; index++) {
    const flags = reader.byte();
    const typeId = flags & 3;
    // Java ignores unused flag bits. read() returns a null stub for both type0 and type3,
    // and NodeResolver represents that stub as a root node.
    const childCount = commandCount(reader, COMMAND_LIMITS.edges - edges, 'child count (probe edge budget)');
    edges += childCount;
    const children = Array.from({ length: childCount }, () => commandCount(reader, count - 1, 'child index'));
    const redirect = flags & 8 ? commandCount(reader, count - 1, 'redirect index') : null;
    const type = (['root', 'literal', 'argument', 'root'] as const)[typeId]!;
    const name = type === 'root' ? null : commandString(reader);
    const parser_id = type === 'argument' ? reader.varInt() : null;
    const parser = parser_id === null ? null : COMMAND_ARGUMENT_TYPES[parser_id];
    if (parser_id !== null && parser === undefined) throw new Error(`Unsupported 26.3 command argument parser ID ${parser_id} at node ${index}; probe cannot infer its property layout`);
    const properties = parser_id === null ? null : argumentProperties(reader, parser_id);
    const suggestion_provider = type === 'argument' && flags & 16 ? commandIdentifier(reader) : null;
    nodes.push({ type, flags, executable: Boolean(flags & 4), restricted: Boolean(flags & 32),
      children, redirect, name, parser_id, parser: parser ?? null, properties, suggestion_provider });
  }
  const root_index = commandCount(reader, count - 1, 'root index');
  if (nodes[root_index]!.type !== 'root') throw new Error('Command root index does not name a root node');
  if (reader.offset !== payload.length) throw new Error(`Trailing command packet bytes: ${payload.length - reader.offset}`);

  let duplicate_child_indices = 0, duplicate_child_names = 0, root_child_edges = 0;
  for (const node of nodes) {
    duplicate_child_indices += node.children.length - new Set(node.children).size;
    const names = new Set<string | null>();
    for (const child of node.children) {
      const target = nodes[child]!;
      if (target.type === 'root') { root_child_edges++; continue; }
      if (names.has(target.name)) duplicate_child_names++;
      names.add(target.name);
    }
  }
  // Kahn's algorithm avoids recursion and quadratic rescans on hostile deep/large graphs.
  for (const kind of ['children', 'redirect'] as const) {
    const pending = new Uint32Array(count);
    const dependents = Array.from({ length: count }, () => [] as number[]);
    for (const [index, node] of nodes.entries()) {
      const dependencies = kind === 'children' ? node.children : node.redirect === null ? [] : [node.redirect];
      pending[index] = dependencies.length;
      for (const dependency of dependencies) dependents[dependency]!.push(index);
    }
    const ready: number[] = [];
    for (let index = 0; index < count; index++) if (!pending[index]) ready.push(index);
    for (let next = 0; next < ready.length; next++) {
      for (const dependent of dependents[ready[next]!]!) {
        pending[dependent] = pending[dependent]! - 1;
        if (!pending[dependent]) ready.push(dependent);
      }
    }
    if (ready.length !== count) throw new Error(`Cyclic command ${kind} graph`);
  }
  const reachable = new Set([root_index]);
  const visit = [root_index];
  for (let index = 0; index < visit.length; index++) {
    const node = nodes[visit[index]!]!;
    for (const target of [...node.children, ...(node.redirect === null ? [] : [node.redirect])]) {
      if (!reachable.has(target)) { reachable.add(target); visit.push(target); }
    }
  }

  const literalPaths = new Set<string>();
  const paths = [{ index: root_index, tokens: [] as string[] }];
  let pathCharacters = 0;
  for (let index = 0; index < paths.length; index++) {
    const { index: nodeIndex, tokens } = paths[index]!;
    for (const child of nodes[nodeIndex]!.children) {
      const node = nodes[child]!;
      if (node.type === 'root') continue; // NodeResolver resolves root children but never adds them.
      const next = [...tokens, node.type === 'argument' ? `<${node.name}>` : node.name!];
      const path = next.join(' ');
      pathCharacters += path.length;
      if (next.length > COMMAND_LIMITS.depth || paths.length >= COMMAND_LIMITS.paths || pathCharacters > COMMAND_LIMITS.path_characters) {
        throw new Error('Command path expansion exceeds probe support budget');
      }
      if (node.type === 'literal') literalPaths.add(path);
      paths.push({ index: child, tokens: next });
    }
  }
  const parser_counts: Record<string, number> = {};
  for (const node of nodes) if (node.parser) parser_counts[node.parser] = (parser_counts[node.parser] ?? 0) + 1;
  return {
    nodes,
    root_index, node_count: count, child_edge_count: edges,
    root_nodes: nodes.filter(node => node.type === 'root').length,
    literal_nodes: nodes.filter(node => node.type === 'literal').length,
    argument_nodes: nodes.filter(node => node.type === 'argument').length,
    executable_nodes: nodes.filter(node => node.executable).length,
    restricted_nodes: nodes.filter(node => node.restricted).length,
    redirect_nodes: nodes.filter(node => node.redirect !== null).length,
    suggestion_nodes: nodes.filter(node => node.suggestion_provider !== null).length,
    roots: [...new Set(nodes[root_index]!.children.filter(index => nodes[index]!.type !== 'root')
      .map(index => nodes[index]!.name!))].sort(),
    literal_paths: [...literalPaths].sort(),
    literal_path_scope: 'declared_child_edges_arguments_as_placeholders_root_children_ignored_no_redirect_or_brigadier_merge' as const,
    argument_parsers: Object.keys(parser_counts).sort(), parser_counts,
    unreachable_nodes: count - reachable.size,
    duplicate_child_indices, duplicate_child_names, root_child_edges,
    unused_flag_nodes: nodes.filter(node => Boolean(node.flags & ~63)).length,
    reserved_type_nodes: nodes.filter(node => (node.flags & 3) === 3).length,
    payload_bytes: payload.length, payload_sha256: createHash('sha256').update(payload).digest('hex'),
    decoded_bytes: reader.offset, probe_support_limits: COMMAND_LIMITS,
  };
}

function string(value: string): Buffer {
  const bytes = Buffer.from(value, 'utf8');
  return Buffer.concat([varInt(bytes.length), bytes]);
}

function floats(...values: number[]): Buffer {
  const output = Buffer.alloc(values.length * 4);
  values.forEach((value, index) => output.writeFloatBE(value, index * 4));
  return output;
}

function doubles(...values: number[]): Buffer {
  const output = Buffer.alloc(values.length * 8);
  values.forEach((value, index) => output.writeDoubleBE(value, index * 8));
  return output;
}

/** Decodes network NBT text without executing component data. */
function nbt(reader: Reader, type = reader.byte(), depth = 0): unknown {
  if (depth > 64) throw new Error('NBT depth limit exceeded');
  if (++reader.nbtNodes > 131072) throw new Error('NBT node budget exceeded');
  switch (type) {
    case 0: return null;
    case 1: return reader.take(1).readInt8();
    case 2: return reader.take(2).readInt16BE();
    case 3: return reader.int();
    case 4: return reader.take(8).readBigInt64BE().toString();
    case 5: return reader.float();
    case 6: return reader.double();
    case 7: return reader.take(reader.int());
    case 8: return reader.take(reader.take(2).readUInt16BE()).toString('utf8');
    case 9: {
      const elementType = reader.byte();
      const count = reader.int();
      if (count < 0 || count > 65536) throw new Error('NBT list limit exceeded');
      return Array.from({ length: count }, () => nbt(reader, elementType, depth + 1));
    }
    case 10: {
      const value: Record<string, unknown> = {};
      for (let count = 0; count < 65536; count++) {
        const childType = reader.byte();
        if (!childType) return value;
        const key = reader.take(reader.take(2).readUInt16BE()).toString('utf8');
        Object.defineProperty(value, key, { value: nbt(reader, childType, depth + 1), enumerable: true });
      }
      throw new Error('NBT compound limit exceeded');
    }
    case 11: return reader.take(reader.int() * 4);
    case 12: return reader.take(reader.int() * 8);
    default: throw new Error(`Unknown NBT tag ${type}`);
  }
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function text(component: unknown): string {
  if (typeof component === 'string') return component;
  if (Array.isArray(component)) return component.map(text).join('');
  if (!record(component)) return '';
  return [component.text, component.translate, component.with, component.extra].map(text).filter(Boolean).join(' ');
}

type Position = { x: number; y: number; z: number; yaw: number; pitch: number };
type ChatObservation = {
  text: string;
  packet_id: number;
  packet_type: 'system_chat' | 'disguised_chat' | 'player_chat';
  text_scope: 'component_text' | 'message_body';
  signature: 'absent' | 'present' | 'not_applicable';
  signature_verified: false;
  overlay?: boolean;
  component_nbt?: unknown;
};

/** Keep the actual overlay flag and untranslated component for permission controls. */
export function decodeSystemChat(payload: Buffer): ChatObservation {
  const reader = new Reader(payload);
  if (reader.varInt() !== ids.play.chat) throw new Error('Expected system chat packet');
  const component_nbt = nbt(reader), overlay = reader.byte();
  if (overlay > 1 || reader.offset !== payload.length) throw new Error('Invalid system chat boundary');
  return { text: text(component_nbt), packet_id: ids.play.chat, packet_type: 'system_chat',
    text_scope: 'component_text', signature: 'not_applicable', signature_verified: false,
    overlay: overlay === 1, component_nbt };
}

/** Selected literal container titles. Only list entries unwrap the NBT envelope. */
export function decodeContainerTitle(value: unknown, depth = 0): unknown {
  if (depth > 8) throw new Error('Container title nesting exceeds fixture scope');
  if (typeof value === 'string') return { text: value };
  if (!record(value)) throw new Error('Expected literal container title');
  knownFields(value, ['text', 'type', 'bold', 'italic', 'color', 'extra'], 'title');
  if (typeof value.text !== 'string' || (value.type !== undefined && value.type !== 'text')) throw new Error('Unsupported title component');
  const result: Record<string, unknown> = { text: value.text };
  for (const style of ['bold', 'italic']) if (value[style] !== undefined) result[style] = signBoolean(value[style], `title.${style}`);
  if (value.color !== undefined) {
    if (typeof value.color !== 'string' || !/^(?:black|dark_blue|dark_green|dark_aqua|dark_red|dark_purple|gold|gray|dark_gray|blue|green|aqua|red|light_purple|yellow|white|#[0-9a-fA-F]{6})$/u.test(value.color)) throw new Error('Unsupported title color');
    result.color = value.color;
  }
  if (value.extra !== undefined) {
    if (!Array.isArray(value.extra) || value.extra.length > 16) throw new Error('Unsupported title extras');
    result.extra = value.extra.map((entry: unknown) => decodeContainerTitle(
      record(entry) && Object.keys(entry).length === 1 && Object.hasOwn(entry, '') ? entry[''] : entry, depth + 1));
  }
  return result;
}

/** Official holder registry value is ID+1; zero carries SoundEvent's direct codec. */
export function decodeSoundPacket(payload: Buffer) {
  const reader = new Reader(payload);
  if (reader.varInt() !== 119) throw new Error('Expected Sound packet');
  const holder = reader.varInt();
  if (holder < 0) throw new Error('Invalid sound holder');
  let sound_id: string | null = null;
  let fixed_range: number | null = null;
  if (holder === 0) {
    sound_id = reader.string();
    const present = reader.byte();
    if (present > 1) throw new Error('Invalid sound range optional');
    if (present) fixed_range = reader.float();
  }
  const source = reader.varInt(), x = reader.int() / 8, y = reader.int() / 8, z = reader.int() / 8;
  const volume = reader.float(), pitch = reader.float(), seed = reader.take(8).toString('hex');
  if (source < 0 || source > 9 || ![x, y, z, volume, pitch, ...(fixed_range === null ? [] : [fixed_range])].every(Number.isFinite)
      || reader.offset !== payload.length) throw new Error('Invalid Sound packet boundary');
  return { packet_id: 119, registry_id: holder ? holder - 1 : null, sound_id, fixed_range, source, x, y, z, volume, pitch, seed };
}

export function decodeDisconnect(payload: Buffer) {
  const reader = new Reader(payload);
  if (reader.varInt() !== 32) throw new Error('Expected PLAY Disconnect');
  const component_nbt = nbt(reader);
  if (reader.offset !== payload.length) throw new Error('Trailing Disconnect bytes');
  return { packet_id: 32, component_nbt, text: text(component_nbt),
    translation: record(component_nbt) && typeof component_nbt.translate === 'string' ? component_nbt.translate : null,
    payload_base64: payload.toString('base64') };
}

/** Fresh unsigned offline sender only: no previous signed-message acknowledgments.
 * ServerboundChatPacket + LastSeenMessages.Update: string, instant(ms), salt,
 * absent signature, offset=0, fixed 20-bit set (3 bytes), IGNORE_CHECKSUM=0.
 */
export function encodeUnsignedChat(message: string, timestamp = Date.now()): Buffer {
  if (!message || message.length > 256 || /[\u0000\r\n]/u.test(message) || !Number.isSafeInteger(timestamp) || timestamp < 0) throw new Error('Invalid unsigned chat fixture');
  const instant = Buffer.alloc(8); instant.writeBigInt64BE(BigInt(timestamp));
  return Buffer.concat([string(message), instant, Buffer.alloc(8), Buffer.from([0]), varInt(0), Buffer.alloc(4)]);
}

/** Fully consume just the unsigned offline fixtures. Other signature/filter
 * branches fail closed; this is not secure-chat validation or a general codec.
 * Primary: Clientbound{Player,Disguised}ChatPacket, SignedMessageBody.Packed,
 * LastSeenMessages.Packed, FilterMask.Type, ChatType.Bound in pinned 26.3.
 */
export function decodeOfflineChatPacket(payload: Buffer): ChatObservation {
  const reader = new Reader(payload), packet_id = reader.varInt();
  const optionalComponent = () => {
    const present = reader.byte();
    if (present > 1) throw new Error('Invalid optional chat component');
    return present ? nbt(reader) : null;
  };
  let observation: ChatObservation;
  if (packet_id === 66) {
    reader.varInt(); reader.take(16); reader.varInt();
    if (reader.byte() !== 0) throw new Error('Signed chat outside fresh unsigned fixture scope');
    const message = reader.string();
    if (message.length > 256) throw new Error('Oversized player chat body');
    reader.take(16); // Body instant and salt.
    if (reader.varInt() !== 0) throw new Error('Signed last-seen list outside unsigned fixture scope');
    optionalComponent(); // Preserve the body observation, not a renderer claim.
    if (reader.varInt() !== 0) throw new Error('Filtered chat outside pass-through fixture scope');
    observation = { text: message, packet_id, packet_type: 'player_chat', text_scope: 'message_body', signature: 'absent', signature_verified: false };
  } else if (packet_id === 33) {
    const component_nbt = nbt(reader);
    observation = { text: text(component_nbt), component_nbt, packet_id, packet_type: 'disguised_chat', text_scope: 'component_text', signature: 'not_applicable', signature_verified: false };
  } else throw new Error('Expected offline PlayerChat or DisguisedChat');
  if (reader.varInt() <= 0) throw new Error('Inline/unregistered chat type outside fixture scope');
  nbt(reader); // ChatType.Bound sender display name.
  optionalComponent(); // Optional target name.
  if (reader.offset !== payload.length) throw new Error('Trailing offline chat bytes');
  return observation;
}

export function encodeTrigger(objective: string, value: number): Buffer {
  if (!/^[A-Za-z0-9_.+-]{1,16}$/u.test(objective) || !Number.isSafeInteger(value) || value < -2147483648 || value > 2147483647) throw new Error('Invalid trigger objective/value');
  return string(`trigger ${objective} set ${value}`);
}

type Command = { id: string | number } & (
  | { action: 'state' }
  | { action: 'quit' }
  | { action: 'expect_disconnect'; translation: 'disconnect.spam' }
  | { action: 'chat' | 'command'; text: string }
  | { action: 'trigger'; objective: string; value: number }
  | { action: 'move'; x: number; y: number; z: number; yaw?: number; pitch?: number }
  | { action: 'use_item_on'; x: number; y: number; z: number; face?: number }
  | { action: 'sign_update'; x: number; y: number; z: number; messages: string[]; slot: SignSlot }
  | { action: 'close_menu' }
  | { action: 'swap_hands' }
  | { action: 'swap'; slot: number; hotbar: number }
);

export function parseCommand(value: unknown): Command {
  if (!record(value) || (typeof value.id !== 'string' && !Number.isSafeInteger(value.id))) throw new Error('id must be a string or safe integer');
  const id = typeof value.id === 'string' ? value.id : Number(value.id);
  if (value.action === 'state' || value.action === 'quit') return { id, action: value.action };
  if (value.action === 'expect_disconnect') {
    if (value.translation !== 'disconnect.spam') throw new Error('Only the explicit spam negative fixture may expect a disconnect');
    return { id, action: value.action, translation: value.translation };
  }
  if (value.action === 'close_menu') return { id, action: value.action };
  if (value.action === 'swap_hands') return { id, action: value.action };
  if (value.action === 'trigger') {
    if (typeof value.objective !== 'string' || typeof value.value !== 'number') throw new Error('Trigger requires objective/value');
    encodeTrigger(value.objective, value.value);
    return { id, action: value.action, objective: value.objective, value: value.value };
  }
  if (value.action === 'use_item_on' || value.action === 'sign_update') {
    const { x, y, z } = value;
    if (typeof x !== 'number' || typeof y !== 'number' || typeof z !== 'number') throw new Error('Interaction requires block coordinates');
    packedBlockPos(x, y, z);
    if (value.action === 'sign_update') {
      const messages = signLines(value.messages), slot = value.slot;
      if (slot !== 0 && slot !== 1) throw new Error('Invalid SignTextSlot');
      return { id, action: value.action, x, y, z, messages, slot };
    }
    if (value.face !== undefined && (typeof value.face !== 'number' || ![1, 2, 3].includes(value.face))) throw new Error('Invalid interaction face');
    return { id, action: value.action, x, y, z, ...(typeof value.face === 'number' ? { face: value.face } : {}) };
  }
  if (value.action === 'swap') {
    const { slot, hotbar } = value;
    if (typeof slot !== 'number' || !Number.isInteger(slot) || slot < 0 || slot > 26 ||
        typeof hotbar !== 'number' || !Number.isInteger(hotbar) || hotbar < 0 || hotbar > 8) throw new Error('Invalid container SWAP slots');
    return { id, action: value.action, slot, hotbar };
  }
  if (value.action === 'chat' || value.action === 'command') {
    if (typeof value.text !== 'string' || !value.text || value.text.length > 256 || /[\r\n\u0000]/u.test(value.text)) throw new Error('text must contain 1 to 256 characters on one line');
    return { id, action: value.action, text: value.text };
  }
  if (value.action === 'move') {
    const { x, y, z, yaw, pitch } = value;
    if (typeof x !== 'number' || typeof y !== 'number' || typeof z !== 'number' ||
      ![x, y, z].every(Number.isFinite) || Math.abs(x) > 30_000_000 || Math.abs(z) > 30_000_000 || Math.abs(y) > 20_000_000) throw new Error('move requires finite world coordinates');
    if ((yaw !== undefined && (typeof yaw !== 'number' || !Number.isFinite(yaw))) ||
      (pitch !== undefined && (typeof pitch !== 'number' || !Number.isFinite(pitch) || Math.abs(pitch) > 90))) throw new Error('Invalid rotation');
    return { id, action: 'move', x, y, z, ...(typeof yaw === 'number' ? { yaw } : {}), ...(typeof pitch === 'number' ? { pitch } : {}) };
  }
  throw new Error('Unsupported action');
}

export function decodeCompressedPacket(frame: Buffer, threshold: number): Buffer {
  if (threshold < 0) return frame;
  const reader = new Reader(frame);
  const length = reader.varInt();
  const payload = reader.take(frame.length - reader.offset);
  if (!length) return payload;
  if (length < threshold || length > MAX_PACKET) throw new Error('Invalid compressed packet length');
  const inflated = inflateSync(payload, { maxOutputLength: MAX_PACKET });
  if (inflated.length !== length) throw new Error('Compressed packet length mismatch');
  return inflated;
}

function emit(value: unknown) {
  process.stdout.write(JSON.stringify(value) + '\n');
}

function main() {
  const { values } = parseArgs({ options: { host: { type: 'string', default: '127.0.0.1' }, port: { type: 'string', default: '25565' }, name: { type: 'string', default: 'FerraloomTest' }, 'block-data': { type: 'boolean', default: false },
    'container-metadata': { type: 'boolean', default: false }, 'raw-packets': { type: 'boolean', default: false },
    'strict-offline-chat': { type: 'boolean', default: false } } });
  const port = Number(values.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535 || !/^[A-Za-z0-9_]{1,16}$/u.test(values.name)) throw new Error('Invalid port or username');
  const socket = createConnection({ host: values.host, port });
  let buffer = Buffer.alloc(0);
  let phase: 'login' | 'configuration' | 'play' = 'login';
  let compression = -1;
  let ready = false;
  let quitting = false;
  let expectedDisconnect: 'disconnect.spam' | null = null;
  let firstPosition = true;
  const chunks = new Set<string>();
  let interactionSequence = 0;
  const state = {
    version: VERSION, protocol: PROTOCOL, phase: 'login', login: false, chunks: 0, registry_packets: 0,
    inventory_packets: 0, slot_packets: 0, health: null as number | null, food: null as number | null,
    position: null as Position | null, server_position: null as Position | null,
    teleport_confirmations: 0, keepalives: 0, packets: 0, block_updates: 0,
    last_block: null as { x: number; y: number; z: number; state_id: number } | null,
    commands_packets: 0,
    command_tree: null as Omit<ReturnType<typeof decodeCommandTree>, 'nodes'> | null,
    command_tree_error: null as { packet_id: number; payload_bytes: number; payload_sha256: string; message: string } | null,
    chats: [] as string[],
    chat_messages: [] as (ChatObservation & { sequence: number })[],
    menu: null as { container_id: number; menu_type: number; opened_sequence: number; state_id: number;
      slots: PlainStack[]; carried: PlainStack } | null,
    menu_packets: [] as (ContainerPacket & { sequence: number })[],
    player_inventory: null as PlayerInventoryObservation | null,
    sign_packets: [] as (SignObservation & { sequence: number })[],
    chunk_packets: [] as (Omit<ReturnType<typeof decodeChunkBlockEntities>, 'signs' | 'decoded_bytes'> & { sequence: number })[],
    sounds: [] as (ReturnType<typeof decodeSoundPacket> & { sequence: number; payload_base64: string })[],
    disconnect: null as ReturnType<typeof decodeDisconnect> | null,
    block_ack: -1,
  };

  function send(id: number, ...parts: Buffer[]) {
    if (quitting) return false;
    let packet = Buffer.concat([varInt(id), ...parts]);
    if (values['raw-packets'] && phase === 'play' && [7, 9, 18, 19, 41, 66].includes(id)) {
      emit({ event: 'wire_packet', direction: 'serverbound', packet_id: id, payload_base64: packet.toString('base64') });
    }
    if (compression >= 0) packet = packet.length >= compression
      ? Buffer.concat([varInt(packet.length), deflateSync(packet)])
      : Buffer.concat([varInt(0), packet]);
    socket.write(Buffer.concat([varInt(packet.length), packet]));
    return true;
  }

  function move(position: Position) {
    send(31, doubles(position.x, position.y, position.z), floats(position.yaw, position.pitch), Buffer.from([1]));
    state.position = position;
  }

  function checkReady() {
    if (!ready && state.login && state.position && chunks.size) {
      ready = true;
      emit({ event: 'ready', ...state });
    }
  }

  function receiveChat(observation: ChatObservation) {
    state.chats.push(observation.text);
    if (state.chats.length > 100) state.chats.shift();
    state.chat_messages.push({ ...observation, sequence: state.packets });
    if (state.chat_messages.length > 100) state.chat_messages.shift();
    emit({ event: 'chat', ...observation });
  }

  function receiveSign(observation: SignObservation) {
    const received = { ...observation, sequence: state.packets };
    state.sign_packets.push(received);
    if (state.sign_packets.length > 256) state.sign_packets.shift();
    emit({ event: 'sign_packet', ...received });
  }

  function packet(payload: Buffer) {
    const reader = new Reader(payload);
    const id = reader.varInt();
    state.packets++;
    if (values['raw-packets'] && phase === 'play' && [4, 18, 20, 32, 33, 60, 66, 110, 119, 124].includes(id)) {
      emit({ event: 'wire_packet', direction: 'clientbound', packet_id: id, sequence: state.packets, payload_base64: payload.toString('base64') });
    }
    if (phase === 'login') {
      if (id === 0) throw new Error(`Login disconnected: ${reader.string()}`);
      if (id === 1) throw new Error('Online-mode encryption requested; this probe requires an offline test server');
      if (id === 3) compression = reader.varInt();
      if (id === 4) send(2, varInt(reader.varInt()), Buffer.from([0]));
      if (id === 2) {
        send(3);
        phase = 'configuration';
        state.phase = phase;
        // ClientInformation: locale, distance, chat, colors, skin, hand,
        // text filtering, server listing, particle status.
        send(0, string('en_us'), Buffer.from([2]), varInt(0), Buffer.from([1, 127]), varInt(1), Buffer.from([0, 1]), varInt(0));
      }
      return;
    }
    if (phase === 'configuration') {
      if (id === 2) throw new Error(`Configuration disconnected: ${text(nbt(reader))}`);
      if (id === ids.config.keepAlive) { send(4, reader.take(8)); state.keepalives++; }
      if (id === ids.config.ping) send(5, reader.take(4));
      if (id === ids.config.registry) state.registry_packets++;
      if (id === ids.config.knownPacks) send(7, varInt(0));
      if (id === ids.config.conduct) send(9);
      if (id === ids.config.finish) { send(3); phase = 'play'; state.phase = phase; }
      return;
    }
    if (id === ids.play.disconnect) {
      const observation = decodeDisconnect(payload);
      state.disconnect = observation;
      const expected = expectedDisconnect !== null && observation.translation === expectedDisconnect;
      emit({ event: 'disconnect', expected, ...observation, sequence: state.packets });
      if (!expected) throw new Error(`Unexpected PLAY disconnect: ${JSON.stringify(observation)}`);
      quitting = true;
      socket.end();
      return;
    }
    if (id === 4) {
      state.block_ack = reader.varInt();
      if (state.block_ack < 0 || reader.offset !== payload.length) throw new Error('Invalid block acknowledgement');
    }
    if (id === ids.play.keepAlive) { send(28, reader.take(8)); state.keepalives++; }
    if (id === ids.play.ping) send(45, reader.take(4));
    if (id === ids.play.login) state.login = true;
    if (id === ids.play.commands) {
      state.commands_packets++;
      const body = reader.take(payload.length - reader.offset);
      try {
        const { nodes: _nodes, ...observation } = decodeCommandTree(body);
        state.command_tree = observation;
        state.command_tree_error = null;
        emit({ event: 'commands', commands_packets: state.commands_packets, command_tree: observation });
      } catch (error) {
        state.command_tree_error = { packet_id: id, payload_bytes: body.length,
          payload_sha256: createHash('sha256').update(body).digest('hex'),
          message: error instanceof Error ? error.message : String(error) };
        emit({ event: 'commands_error', ...state.command_tree_error });
        throw error;
      }
    }
    if (id === ids.play.chunk) {
      const key = `${reader.int()},${reader.int()}`;
      chunks.add(key); state.chunks = chunks.size;
      if (values['block-data']) {
        const { signs, decoded_bytes: _bytes, ...chunk } = decodeChunkBlockEntities(payload, 'rich');
        state.chunk_packets.push({ ...chunk, sequence: state.packets });
        if (state.chunk_packets.length > 128) state.chunk_packets.shift();
        signs.forEach(receiveSign);
      }
    }
    if (values['block-data'] && (id === 6 || id === 61)) {
      const observation = decodeSignPacket(payload, 'rich');
      if (observation) receiveSign(observation);
    }
    if (id === ids.play.chunkBatch) send(11, floats(64));
    if (id === ids.play.inventory) state.inventory_packets++;
    if (id === ids.play.slot) state.slot_packets++;
    if (values['block-data'] && (id === 60 || id === ids.play.inventory || id === ids.play.slot)) {
      const observation = { ...decodeContainerPacket(payload, values['container-metadata']), sequence: state.packets };
      state.player_inventory = observePlayerInventory(state.player_inventory, observation);
      state.menu_packets.push(observation);
      if (state.menu_packets.length > 64) state.menu_packets.shift();
      if (observation.kind === 'open') {
        state.menu = { container_id: observation.container_id, menu_type: observation.menu_type,
          opened_sequence: observation.sequence, state_id: 0, slots: [], carried: null };
      } else if (state.menu?.container_id === observation.container_id) {
        state.menu.state_id = observation.state_id;
        if (observation.kind === 'content') {
          state.menu.slots = observation.slots.slice();
          state.menu.carried = observation.carried;
        } else if (state.menu.slots.length) {
          if (observation.slot >= state.menu.slots.length) throw new Error('Update outside the open menu');
          state.menu.slots[observation.slot] = observation.stack;
        }
      }
      emit({ event: 'menu_packet', ...observation });
    }
    if (values['container-metadata'] && id === 119) {
      const observation = { ...decodeSoundPacket(payload), sequence: state.packets, payload_base64: payload.toString('base64') };
      state.sounds.push(observation);
      if (state.sounds.length > 64) state.sounds.shift();
      emit({ event: 'sound', ...observation });
    }
    if (id === ids.play.health) { state.health = reader.float(); state.food = reader.varInt(); }
    if (id === ids.play.position) {
      const teleportId = reader.varInt();
      const x = reader.double(), y = reader.double(), z = reader.double();
      reader.take(24); // Velocity delta is independent of position acknowledgement.
      const yaw = reader.float(), pitch = reader.float(), flags = reader.int();
      const previous = state.position ?? { x: 0, y: 0, z: 0, yaw: 0, pitch: 0 };
      const position = { x: x + (flags & 1 ? previous.x : 0), y: y + (flags & 2 ? previous.y : 0),
        z: z + (flags & 4 ? previous.z : 0), yaw: yaw + (flags & 8 ? previous.yaw : 0), pitch: pitch + (flags & 16 ? previous.pitch : 0) };
      state.server_position = position;
      send(0, varInt(teleportId), doubles(position.x, position.y, position.z), floats(position.yaw, position.pitch));
      state.teleport_confirmations++;
      move(position);
      if (firstPosition) { send(44); firstPosition = false; }
      emit({ event: 'position', position });
    }
    if (id === ids.play.block) {
      const packed = reader.take(8).readBigInt64BE();
      state.last_block = { x: Number(BigInt.asIntN(26, packed >> 38n)), y: Number(BigInt.asIntN(12, packed & 4095n)),
        z: Number(BigInt.asIntN(26, packed >> 12n)), state_id: reader.varInt() };
      state.block_updates++;
    }
    if (id === ids.play.chat) receiveChat(decodeSystemChat(payload));
    if (values['strict-offline-chat'] && (id === ids.play.playerChat || id === ids.play.disguisedChat)) {
      receiveChat(decodeOfflineChatPacket(payload));
    }
    if (!values['strict-offline-chat'] && id === ids.play.disguisedChat) {
      receiveChat({ text: text(nbt(reader)), packet_id: id,
        packet_type: 'disguised_chat',
        text_scope: 'component_text', signature: 'not_applicable', signature_verified: false });
    }
    if (!values['strict-offline-chat'] && id === ids.play.playerChat) {
      reader.varInt(); // Global message index.
      reader.take(16); // Sender UUID.
      reader.varInt(); // Sender-local index.
      const signed = Boolean(reader.byte());
      if (signed) reader.take(256); // Optional signature; this offline probe does not verify signatures.
      receiveChat({ text: reader.string(), packet_id: id, packet_type: 'player_chat',
        text_scope: 'message_body', signature: signed ? 'present' : 'absent', signature_verified: false });
    }
    checkReady();
  }

  socket.on('connect', () => {
    const short = Buffer.alloc(2); short.writeUInt16BE(port);
    send(0, varInt(PROTOCOL), string(values.host), short, varInt(2));
    const uuid = createHash('md5').update(`OfflinePlayer:${values.name}`).digest();
    uuid[6] = (uuid[6]! & 15) | 48; uuid[8] = (uuid[8]! & 63) | 128;
    send(0, string(values.name), uuid);
  });
  socket.on('data', (incoming: Buffer) => {
    try {
      buffer = Buffer.concat([buffer, incoming]);
      if (buffer.length > MAX_PACKET * 2) throw new Error('Receive buffer exceeded limit');
      while (buffer.length) {
        const length = readVarInt(buffer);
        if (!length) return;
        if (length.bytes > 3 || length.value < 1 || length.value > 2097151) throw new Error('Invalid frame length');
        if (buffer.length < length.bytes + length.value) return;
        const frame = buffer.subarray(length.bytes, length.bytes + length.value);
        buffer = buffer.subarray(length.bytes + length.value);
        packet(decodeCompressedPacket(frame, compression));
      }
    } catch (error) {
      emit({ event: 'error', message: error instanceof Error ? error.message : String(error) });
      process.exitCode = 1;
      socket.destroy();
    }
  });
  socket.on('error', error => { emit({ event: 'error', message: error.message }); process.exitCode = 1; });
  const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
  const tick = setInterval(() => { if (phase === 'play' && state.position) send(13); }, 50);
  const timeout = setTimeout(() => {
    if (!ready) { emit({ event: 'error', message: 'Timed out before login, position, and chunks', state }); process.exitCode = 1; socket.destroy(); }
  }, 60000);
  socket.on('close', () => {
    clearInterval(tick); clearTimeout(timeout); input.close();
    emit({ event: 'end', expected: quitting, state });
    if (!quitting) process.exitCode = 1;
    process.stdin.destroy();
  });
  input.on('line', line => {
    let id: unknown = null;
    try {
      if (line.length > 8192) throw new Error('IPC line exceeds limit');
      const value: unknown = JSON.parse(line);
      if (record(value)) id = value.id ?? null;
      const command = parseCommand(value);
      if (quitting && command.action !== 'state' && command.action !== 'quit') throw new Error('Connection is closing');
      if (command.action === 'state') emit({ id: command.id, ok: true, result: state });
      else if (command.action === 'quit') {
        quitting = true; emit({ id: command.id, ok: true, result: null }); socket.end();
      } else if (command.action === 'expect_disconnect') {
        if (expectedDisconnect !== null || state.disconnect) throw new Error('Disconnect expectation already armed/consumed');
        expectedDisconnect = command.translation;
        emit({ id: command.id, ok: true, result: { expected_translation: expectedDisconnect } });
      } else {
        if (!ready) throw new Error('Client is not ready');
        if (command.action === 'move') {
          const previous = state.position!;
          move({ x: command.x, y: command.y, z: command.z, yaw: command.yaw ?? previous.yaw, pitch: command.pitch ?? previous.pitch });
          emit({ id: command.id, ok: true, result: { sent_position: state.position } });
        } else if (command.action === 'swap_hands') {
          if (!values['container-metadata']) throw new Error('Swap-hands fixture requires --container-metadata');
          send(41, encodeSwapHands(++interactionSequence));
          emit({ id: command.id, ok: true, result: { sent_action: command.action, interaction_sequence: interactionSequence } });
        } else if (command.action === 'use_item_on' || command.action === 'close_menu' || command.action === 'swap' || command.action === 'sign_update') {
          if (!values['block-data']) throw new Error('Block interaction actions require --block-data');
          if (command.action === 'use_item_on') {
            send(66, encodeUseItemOn(command.x, command.y, command.z, ++interactionSequence, command.face));
          } else if (command.action === 'sign_update') {
            // Deliberately permit unsolicited sends: acceptance must exercise
            // server authorization, not a client-side editor precondition.
            send(62, encodeSignUpdate(command.x, command.y, command.z, command.messages, command.slot));
          } else {
            const menu = state.menu;
            if (!menu) throw new Error('No open menu');
            if (command.action === 'close_menu') {
              send(19, varInt(menu.container_id));
              state.menu = null;
            } else {
              if (menu.menu_type !== 2 || menu.slots.length !== 63) throw new Error('No synchronized single-container SWAP menu');
              if (menu.carried !== null) throw new Error('SWAP fixture requires an empty cursor');
              send(18, encodeContainerSwap(menu.container_id, menu.state_id, command.slot, command.hotbar, menu.slots));
            }
          }
          emit({ id: command.id, ok: true, result: { sent_action: command.action,
            ...(command.action === 'use_item_on' ? { interaction_sequence: interactionSequence } : {}) } });
        } else if (command.action === 'trigger') {
          const written = send(7, encodeTrigger(command.objective, command.value));
          emit({ id: command.id, ok: true, result: { sent_command: `trigger ${command.objective} set ${command.value}`, written } });
        } else {
          const message = command.text.replace(/^\//u, '');
          if (command.action === 'chat' && !command.text.startsWith('/')) {
            const written = send(9, encodeUnsignedChat(command.text));
            emit({ id: command.id, ok: true, result: { sent_chat: command.text, written } });
          } else {
            const written = send(7, string(message));
            emit({ id: command.id, ok: true, result: { sent_command: message, written } });
          }
        }
      }
    } catch (error) {
      emit({ id, ok: false, error: error instanceof Error ? error.message : String(error) });
    }
  });
  input.on('close', () => { if (!socket.destroyed) { quitting = true; socket.end(); } });
}

if (import.meta.main) {
  try { main(); }
  catch (error) { emit({ event: 'error', message: error instanceof Error ? error.message : String(error) }); process.exitCode = 1; }
}
