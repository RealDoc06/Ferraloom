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
  play: { chunk: 46, login: 50, position: 73, health: 106, chat: 124, disguisedChat: 33, playerChat: 66, keepAlive: 45, ping: 62,
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
);

export function parseCommand(value: unknown): Command {
  if (!record(value) || (typeof value.id !== 'string' && !Number.isSafeInteger(value.id))) throw new Error('id must be a string or safe integer');
  const id = typeof value.id === 'string' ? value.id : Number(value.id);
  if (value.action === 'state' || value.action === 'quit') return { id, action: value.action };
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
  const { values } = parseArgs({ options: { host: { type: 'string', default: '127.0.0.1' }, port: { type: 'string', default: '25565' }, name: { type: 'string', default: 'FerraloomTest' } } });
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
  const state = {
    version: VERSION, protocol: PROTOCOL, phase: 'login', login: false, chunks: 0, registry_packets: 0,
    inventory_packets: 0, slot_packets: 0, health: null as number | null, food: null as number | null,
    position: null as Position | null, server_position: null as Position | null,
    teleport_confirmations: 0, keepalives: 0, packets: 0, block_updates: 0,
    last_block: null as { x: number; y: number; z: number; state_id: number } | null,
    chats: [] as string[],
    chat_messages: [] as ChatObservation[],
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
    if (id === ids.play.chunk) {
      const key = `${reader.int()},${reader.int()}`;
      chunks.add(key); state.chunks = chunks.size;
    }
    if (id === ids.play.chunkBatch) send(11, floats(64));
    if (id === ids.play.inventory) state.inventory_packets++;
    if (id === ids.play.slot) state.slot_packets++;
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
