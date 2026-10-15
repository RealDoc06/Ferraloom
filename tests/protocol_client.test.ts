import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { deflateSync } from 'node:zlib';
import { PROTOCOL, VERSION, decodeCompressedPacket, parseCommand, readVarInt, varInt } from '../tools/parity/client.ts';

test('probe version matches the repository compatibility target', () => {
  const target: unknown = JSON.parse(readFileSync(new URL('../compatibility/target.json', import.meta.url), 'utf8'));
  assert.ok(target && typeof target === 'object' && 'version' in target && 'protocol' in target);
  assert.equal(VERSION, target.version);
  assert.equal(PROTOCOL, target.protocol);
});

test('IPC rejects malformed commands and unsafe movement without executing input', () => {
  for (const value of [null, {}, { id: 1, action: 'eval', text: 'process.exit()' },
    { id: 1, action: 'move', x: Infinity, y: 0, z: 0 },
    { id: 1, action: 'move', x: 0, y: 0, z: 0, pitch: 100 },
    { id: 1, action: 'command', text: 'say hello\nstop' }]) {
    assert.throws(() => parseCommand(value));
  }
  assert.deepEqual(parseCommand({ id: 'a', action: 'move', x: 1, y: 64, z: -2 }),
    { id: 'a', action: 'move', x: 1, y: 64, z: -2 });
});

test('VarInt decoding distinguishes partial frames, signed values, and overflow', () => {
  assert.equal(readVarInt(Buffer.from([128])), undefined);
  assert.deepEqual(readVarInt(varInt(-1)), { value: -1, bytes: 5 });
  assert.deepEqual(readVarInt(varInt(777)), { value: 777, bytes: 2 });
  assert.throws(() => readVarInt(Buffer.from([128, 128, 128, 128, 16])));
  assert.throws(() => readVarInt(Buffer.from([128, 128, 128, 128, 128, 0])));
});

test('compression validates declared lengths rather than accepting corrupt payloads', () => {
  const packet = Buffer.from('test packet');
  const compressed = deflateSync(packet);
  assert.deepEqual(decodeCompressedPacket(Buffer.concat([varInt(packet.length), compressed]), 1), packet);
  assert.throws(() => decodeCompressedPacket(Buffer.concat([varInt(packet.length + 1), compressed]), 1));
  assert.throws(() => decodeCompressedPacket(Buffer.concat([varInt(packet.length), compressed]), 100));
});
