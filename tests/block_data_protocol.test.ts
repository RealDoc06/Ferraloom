import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decodeContainerPacket, encodeContainerSwap, encodeUseItemOn, parseCommand, varInt,
  type PlainStack } from '../tools/parity/client.ts';

const plain = (item: number, count: number) => Buffer.concat([varInt(count), varInt(item), Buffer.from([0, 0])]);

test('26.3 container packets decode item identities, counts, cursor and signed-short slot', () => {
  const content = Buffer.concat([Buffer.from([18, 3, 9, 63]), plain(1012, 7), Buffer.alloc(63)]);
  const decoded = decodeContainerPacket(content);
  assert.equal(decoded.kind, 'content');
  if (decoded.kind !== 'content') return;
  assert.equal(decoded.container_id, 3);
  assert.equal(decoded.state_id, 9);
  assert.equal(decoded.slots.length, 63);
  assert.deepEqual(decoded.slots[0], { item_id: 1012, count: 7 });
  assert.equal(decoded.slots[62], null);
  assert.equal(decoded.carried, null);
  assert.deepEqual(decodeContainerPacket(Buffer.concat([Buffer.from([20, 3, 10, 0, 54]), plain(1022, 5)])),
    { kind: 'slot', packet_id: 20, container_id: 3, state_id: 10, slot: 54, stack: { item_id: 1022, count: 5 } });
  // A network text component is unnamed NBT, here TAG_String containing "Box".
  assert.deepEqual(decodeContainerPacket(Buffer.from([60, 3, 2, 8, 0, 3, 66, 111, 120])),
    { kind: 'open', packet_id: 60, container_id: 3, menu_type: 2, title: 'Box' });
});

test('focused decoder fails closed on components, invalid counts, truncation and trailing bytes', () => {
  for (const payload of [
    Buffer.from([20, 3, 1, 0, 0, 7, 1, 1, 0]), // Added component, not a plain stack.
    Buffer.from([20, 3, 1, 0, 0, 7, 1, 0, 1]), // Removed component.
    Buffer.concat([Buffer.from([18, 3, 1]), varInt(-1)]),
    Buffer.from([20, 3, 1, 0, 0, 7]),
    Buffer.from([20, 3, 1, 0, 0, 0, 0]),
  ]) assert.throws(() => decodeContainerPacket(payload));
});

test('UseItemOn encodes packed signed coordinates and modern two-boolean hit result', () => {
  const packet = encodeUseItemOn(-1, 200, -2, 17);
  assert.equal(packet[0], 0); // Main hand.
  const packed = packet.readBigUInt64BE(1);
  assert.equal(Number(BigInt.asIntN(26, packed >> 38n)), -1);
  assert.equal(Number(BigInt.asIntN(26, packed >> 12n)), -2);
  assert.equal(Number(BigInt.asIntN(12, packed)), 200);
  assert.equal(packet[9], 1); // UP.
  assert.equal(packet.readFloatBE(10), 0.5);
  assert.equal(packet.readFloatBE(14), 1);
  assert.equal(packet.readFloatBE(18), 0.5);
  assert.deepEqual([...packet.subarray(22)], [0, 0, 17]);
  assert.throws(() => encodeUseItemOn(0.5, 200, 0, 1));
});

test('SWAP uses hashed optional stacks, container revision and menu hotbar index 54', () => {
  const slots: PlainStack[] = Array.from({ length: 63 }, () => null);
  slots[54] = { item_id: 1022, count: 5 };
  const packet = encodeContainerSwap(3, 12, 2, 0, slots);
  assert.deepEqual(packet, Buffer.concat([
    Buffer.from([3, 12, 0, 2, 0, 2, 2, 0, 2, 1]), varInt(1022),
    Buffer.from([5, 0, 0, 0, 54, 0, 0]),
  ]));
  assert.equal(slots[2], null); // Prediction must not mutate received observations.
  assert.deepEqual(slots[54], { item_id: 1022, count: 5 });
  assert.throws(() => encodeContainerSwap(0, 12, 2, 0, slots));
  assert.throws(() => encodeContainerSwap(3, 12, 27, 0, slots));
  assert.throws(() => parseCommand({ id: 1, action: 'swap', slot: 0, hotbar: 9 }));
  assert.deepEqual(parseCommand({ id: 1, action: 'use_item_on', x: 4097, y: 200, z: 1 }),
    { id: 1, action: 'use_item_on', x: 4097, y: 200, z: 1 });
});
