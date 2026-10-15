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

export type PlainStack = { item_id: number; count: number } | null;
export type ContainerPacket =
  | { kind: 'open'; packet_id: 60; container_id: number; menu_type: number; title: string }
  | { kind: 'content'; packet_id: 18; container_id: number; state_id: number; slots: PlainStack[]; carried: PlainStack }
  | { kind: 'slot'; packet_id: 20; container_id: number; state_id: number; slot: number; stack: PlainStack };

function plainStack(reader: Reader): PlainStack {
  const count = reader.varInt();
  if (count === 0) return null;
  if (count < 0) throw new Error('Negative item count');
  const item_id = reader.varInt(), added = reader.varInt(), removed = reader.varInt();
  if (item_id <= 0) throw new Error('Invalid nonempty item ID');
  // The acceptance fixtures deliberately use plain stacks. Component payloads
  // are not length-prefixed in this trusted codec; guessing their size is unsafe.
  if (added !== 0 || removed !== 0) throw new Error(`Unsupported item component patch: ${added}/${removed}`);
  return { item_id, count };
}

/** Exact 26.3 menu codecs; unsupported item components fail the focused probe. */
export function decodeContainerPacket(payload: Buffer): ContainerPacket {
  const reader = new Reader(payload);
  const packet_id = reader.varInt(), container_id = reader.varInt();
  if (container_id < 0 || container_id > 255) throw new Error('Invalid container ID');
  let result: ContainerPacket;
  if (packet_id === 60) {
    const menu_type = reader.varInt();
    if (menu_type < 0) throw new Error('Invalid menu type');
    result = { kind: 'open', packet_id, container_id, menu_type, title: text(nbt(reader)) };
  } else if (packet_id === 18) {
    const state_id = reader.varInt(), count = reader.varInt();
    if (state_id < 0 || count < 0 || count > 4096) throw new Error('Invalid content state or slot count');
    const slots = Array.from({ length: count }, () => plainStack(reader));
    result = { kind: 'content', packet_id, container_id, state_id, slots, carried: plainStack(reader) };
  } else if (packet_id === 20) {
    const state_id = reader.varInt(), slot = reader.take(2).readInt16BE();
    if (state_id < 0 || slot < 0 || slot >= 4096) throw new Error('Invalid slot state or index');
    result = { kind: 'slot', packet_id, container_id, state_id, slot, stack: plainStack(reader) };
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

export function encodeUseItemOn(x: number, y: number, z: number, sequence: number): Buffer {
  if (!Number.isSafeInteger(sequence) || sequence < 0 || sequence > 2147483647) throw new Error('Invalid interaction sequence');
  return Buffer.concat([varInt(0), packedBlockPos(x, y, z), varInt(1), floats(0.5, 1, 0.5), Buffer.from([0, 0]), varInt(sequence)]);
}

function plainStackHash(stack: PlainStack): Buffer {
  if (stack === null) return Buffer.from([0]);
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
};
type Command = { id: string | number } & (
  | { action: 'state' }
  | { action: 'quit' }
  | { action: 'chat' | 'command'; text: string }
  | { action: 'move'; x: number; y: number; z: number; yaw?: number; pitch?: number }
  | { action: 'use_item_on'; x: number; y: number; z: number }
  | { action: 'close_menu' }
  | { action: 'swap'; slot: number; hotbar: number }
);

export function parseCommand(value: unknown): Command {
  if (!record(value) || (typeof value.id !== 'string' && !Number.isSafeInteger(value.id))) throw new Error('id must be a string or safe integer');
  const id = typeof value.id === 'string' ? value.id : Number(value.id);
  if (value.action === 'state' || value.action === 'quit') return { id, action: value.action };
  if (value.action === 'close_menu') return { id, action: value.action };
  if (value.action === 'use_item_on') {
    const { x, y, z } = value;
    if (typeof x !== 'number' || typeof y !== 'number' || typeof z !== 'number') throw new Error('Interaction requires block coordinates');
    packedBlockPos(x, y, z);
    return { id, action: value.action, x, y, z };
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
  const { values } = parseArgs({ options: { host: { type: 'string', default: '127.0.0.1' }, port: { type: 'string', default: '25565' }, name: { type: 'string', default: 'FerraloomTest' }, 'block-data': { type: 'boolean', default: false } } });
  const port = Number(values.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535 || !/^[A-Za-z0-9_]{1,16}$/u.test(values.name)) throw new Error('Invalid port or username');
  const socket = createConnection({ host: values.host, port });
  let buffer = Buffer.alloc(0);
  let phase: 'login' | 'configuration' | 'play' = 'login';
  let compression = -1;
  let ready = false;
  let quitting = false;
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
    chat_messages: [] as ChatObservation[],
    menu: null as { container_id: number; menu_type: number; opened_sequence: number; state_id: number;
      slots: PlainStack[]; carried: PlainStack } | null,
    menu_packets: [] as (ContainerPacket & { sequence: number })[],
  };

  function send(id: number, ...parts: Buffer[]) {
    let packet = Buffer.concat([varInt(id), ...parts]);
    if (compression >= 0) packet = packet.length >= compression
      ? Buffer.concat([varInt(packet.length), deflateSync(packet)])
      : Buffer.concat([varInt(0), packet]);
    socket.write(Buffer.concat([varInt(packet.length), packet]));
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
    state.chat_messages.push(observation);
    if (state.chat_messages.length > 100) state.chat_messages.shift();
    emit({ event: 'chat', ...observation });
  }

  function packet(payload: Buffer) {
    const reader = new Reader(payload);
    const id = reader.varInt();
    state.packets++;
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
    if (id === ids.play.disconnect) throw new Error(`Play disconnected: ${text(nbt(reader))}`);
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
    }
    if (id === ids.play.chunkBatch) send(11, floats(64));
    if (id === ids.play.inventory) state.inventory_packets++;
    if (id === ids.play.slot) state.slot_packets++;
    if (values['block-data'] && (id === 60 || id === ids.play.inventory || id === ids.play.slot)) {
      const observation = { ...decodeContainerPacket(payload), sequence: state.packets };
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
    if (id === ids.play.chat || id === ids.play.disguisedChat) {
      receiveChat({ text: text(nbt(reader)), packet_id: id,
        packet_type: id === ids.play.chat ? 'system_chat' : 'disguised_chat',
        text_scope: 'component_text', signature: 'not_applicable', signature_verified: false });
    }
    if (id === ids.play.playerChat) {
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
      if (command.action === 'state') emit({ id: command.id, ok: true, result: state });
      else if (command.action === 'quit') {
        quitting = true; emit({ id: command.id, ok: true, result: null }); socket.end();
      } else {
        if (!ready) throw new Error('Client is not ready');
        if (command.action === 'move') {
          const previous = state.position!;
          move({ x: command.x, y: command.y, z: command.z, yaw: command.yaw ?? previous.yaw, pitch: command.pitch ?? previous.pitch });
          emit({ id: command.id, ok: true, result: { sent_position: state.position } });
        } else if (command.action === 'use_item_on' || command.action === 'close_menu' || command.action === 'swap') {
          if (!values['block-data']) throw new Error('Menu actions require --block-data');
          if (command.action === 'use_item_on') {
            send(66, encodeUseItemOn(command.x, command.y, command.z, ++interactionSequence));
          } else {
            const menu = state.menu;
            if (!menu || menu.menu_type !== 2 || menu.slots.length !== 63) throw new Error('No synchronized single-container menu');
            if (command.action === 'close_menu') {
              send(19, varInt(menu.container_id));
              state.menu = null;
            } else {
              if (menu.carried !== null) throw new Error('SWAP fixture requires an empty cursor');
              send(18, encodeContainerSwap(menu.container_id, menu.state_id, command.slot, command.hotbar, menu.slots));
            }
          }
          emit({ id: command.id, ok: true, result: { sent_action: command.action } });
        } else {
          const message = command.text.replace(/^\//u, '');
          if (command.action === 'chat' && !command.text.startsWith('/')) {
            const timestamp = Buffer.alloc(8); timestamp.writeBigInt64BE(BigInt(Date.now()));
            // Exact 26.3 unsigned Chat codec: timestamp, salt, optional signature,
            // and LastSeenMessages.Update(offset, fixed 20-bit set, checksum).
            send(9, string(command.text), timestamp, Buffer.alloc(8), Buffer.from([0]), varInt(0), Buffer.alloc(4));
            emit({ id: command.id, ok: true, result: { sent_chat: command.text } });
          } else {
            send(7, string(message));
            emit({ id: command.id, ok: true, result: { sent_command: message } });
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
