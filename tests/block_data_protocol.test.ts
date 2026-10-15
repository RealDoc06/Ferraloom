import assert from 'node:assert/strict';
import { test } from 'node:test';
import { decodeChunkBlockEntities, decodeContainerPacket, decodeSignData, decodeSignPacket, decodeRichSignData, decodeSystemChat,
  encodeContainerSwap, encodeSignUpdate, encodeUseItemOn, encodeTrigger, parseCommand, varInt,
  type PlainStack } from '../tools/parity/client.ts';
import { decodeContainerTitle, decodeDisconnect, decodeOfflineChatPacket, decodeSoundPacket, encodeSwapHands, encodeUnsignedChat } from '../tools/parity/client.ts';

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

const i16 = (value: number) => { const bytes = Buffer.alloc(2); bytes.writeInt16BE(value); return bytes; };
const i32 = (value: number) => { const bytes = Buffer.alloc(4); bytes.writeInt32BE(value); return bytes; };
const utf = (value: string) => { const bytes = Buffer.from(value); return Buffer.concat([i16(bytes.length), bytes]); };
const wireString = (value: string) => { const bytes = Buffer.from(value); return Buffer.concat([varInt(bytes.length), bytes]); };
const field = (type: number, name: string, bytes: Buffer) => Buffer.concat([Buffer.from([type]), utf(name), bytes]);
const messages = ['front direct', '"quoted literal"', '', 'four'];
const faceNBT = (lines: string[], color: string, glow: number) => Buffer.concat([
  field(9, 'messages', Buffer.concat([Buffer.from([8]), i32(4), ...lines.map(utf)])),
  field(8, 'color', utf(color)), field(1, 'has_glowing_text', Buffer.from([glow])), Buffer.from([0]),
]);
const signNBT = () => Buffer.concat([Buffer.from([10]),
  field(10, 'front_text', faceNBT(messages, 'red', 1)),
  field(10, 'back_text', faceNBT(['back', 'north', 'other', 'face'], 'blue', 0)),
  field(1, 'is_waxed', Buffer.from([1])), Buffer.from([0]),
]);
const expectedSign = {
  front_text: { messages, color: 'red', has_glowing_text: true },
  back_text: { messages: ['back', 'north', 'other', 'face'], color: 'blue', has_glowing_text: false },
  is_waxed: true,
};
const blockPos = encodeUseItemOn(-22, -64, 53, 1).subarray(1, 9);

test('metadata menu mode retains selected styled title, native extras and untouched NBT', () => {
  const raw = Buffer.concat([Buffer.from([10]), field(8, 'text', utf('Box')), field(1, 'bold', Buffer.from([1])),
    field(1, 'italic', Buffer.from([0])), field(8, 'color', utf('gold')),
    field(9, 'extra', Buffer.concat([Buffer.from([10]), i32(2), field(8, 'text', utf(' red')), field(8, 'color', utf('red')),
      Buffer.from([0]), field(8, '', utf(' literal')), Buffer.from([0])])), Buffer.from([0])]);
  const packet = Buffer.concat([Buffer.from([60, 4, 2]), raw]);
  const observed = decodeContainerPacket(packet, true);
  assert.equal(observed.kind, 'open');
  if (observed.kind !== 'open') return;
  assert.deepEqual(observed.title_component, { text: 'Box', bold: true, italic: false, color: 'gold',
    extra: [{ text: ' red', color: 'red' }, { text: ' literal' }] });
  assert.deepEqual(observed.title_nbt, { text: 'Box', bold: 1, italic: 0, color: 'gold',
    extra: [{ text: ' red', color: 'red' }, { '': ' literal' }] });
  assert.deepEqual(decodeContainerTitle('{"text":"literal"}'), { text: '{"text":"literal"}' });
  for (const value of [{ '': 'root envelope' }, { text: 'bad', italic: 2 }, { translate: 'container.chest' },
    { text: 'bad', extra: [{ '': { '': 'double envelope' } }] }]) assert.throws(() => decodeContainerTitle(value));
  assert.throws(() => decodeContainerPacket(Buffer.concat([packet, Buffer.from([0])]), true));
});

test('metadata item patch decodes only pinned custom_name=6/item_name=9 NBT and refuses guessed codecs', () => {
  for (const [id, name] of [[6, 'minecraft:custom_name'], [9, 'minecraft:item_name']] as const) {
    const packet = Buffer.concat([Buffer.from([20, 3, 1, 0, 54, 1]), varInt(123), Buffer.from([1, 0]), varInt(id), Buffer.from([8]), utf('key')]);
    const decoded = decodeContainerPacket(packet, true);
    assert.deepEqual(decoded, { kind: 'slot', packet_id: 20, container_id: 3, state_id: 1, slot: 54,
      stack: { item_id: 123, count: 1, components: { [name]: 'key' } } });
    assert.throws(() => decodeContainerPacket(packet)); // Original plain fixture stays strict.
    assert.throws(() => decodeContainerPacket(packet.subarray(0, -1), true));
  }
  for (const id of [0, 7, 1000]) assert.throws(() => decodeContainerPacket(Buffer.concat([
    Buffer.from([20, 3, 1, 0, 54, 1, 123, 1, 0]), varInt(id), Buffer.from([8]), utf('unmodeled'),
  ]), true), /Unsupported\/duplicate item component/);
  const slots: PlainStack[] = Array.from({ length: 63 }, () => null);
  slots[54] = { item_id: 123, count: 1, components: { 'minecraft:custom_name': 'key' } };
  assert.throws(() => encodeContainerSwap(3, 1, 2, 0, slots), /hashed components/);
});

test('26.3 locked sound observes ID+1 holder, fixed positions, source, volume/pitch and exact boundary', () => {
  const float = (value: number) => { const bytes = Buffer.alloc(4); bytes.writeFloatBE(value); return bytes; };
  const tail = Buffer.concat([varInt(4), i32(4097 * 8 + 4), i32(200 * 8 + 4), i32(12), float(1), float(1), Buffer.alloc(8)]);
  const packet = Buffer.concat([varInt(119), varInt(349), tail]);
  assert.deepEqual(decodeSoundPacket(packet), { packet_id: 119, registry_id: 348, sound_id: null, fixed_range: null,
    source: 4, x: 4097.5, y: 200.5, z: 1.5, volume: 1, pitch: 1, seed: '0000000000000000' });
  const direct = decodeSoundPacket(Buffer.concat([varInt(119), varInt(0), wireString('minecraft:block.chest.locked'), Buffer.from([0]), tail]));
  assert.equal(direct.sound_id, 'minecraft:block.chest.locked');
  for (const bad of [packet.subarray(0, -1), Buffer.concat([packet, Buffer.from([0])]),
    Buffer.concat([varInt(119), varInt(0), wireString('sound'), Buffer.from([2]), tail])]) assert.throws(() => decodeSoundPacket(bad));
});

test('first unsigned offline chat uses Instant, absent signature and exact 20-bit update plus checksum', () => {
  // Official ServerboundChatPacket.STREAM_CODEC + LastSeenMessages.Update.
  const instant = Buffer.alloc(8); instant.writeBigInt64BE(123456789n);
  assert.deepEqual(encodeUnsignedChat('hello', 123456789), Buffer.concat([
    wireString('hello'), instant, Buffer.alloc(8), Buffer.from([0, 0, 0, 0, 0, 0]),
  ]));
  for (const message of ['', 'x'.repeat(257), 'bad\nchat']) assert.throws(() => encodeUnsignedChat(message, 0));
});

test('strict offline delivery consumes the whole unsigned body, pass-through filter and registered chat decoration', () => {
  const bound = Buffer.concat([varInt(1), Buffer.from([8]), utf('Sender'), Buffer.from([0])]);
  const packet = Buffer.concat([varInt(66), varInt(0), Buffer.alloc(16), varInt(0), Buffer.from([0]), wireString('delivered'),
    Buffer.alloc(16), varInt(0), Buffer.from([0]), varInt(0), bound]);
  assert.deepEqual(decodeOfflineChatPacket(packet), { text: 'delivered', packet_id: 66, packet_type: 'player_chat',
    text_scope: 'message_body', signature: 'absent', signature_verified: false });
  const disguised = Buffer.concat([varInt(33), Buffer.from([8]), utf('system body'), bound]);
  assert.equal(decodeOfflineChatPacket(disguised).text, 'system body');
  for (let end = 0; end < packet.length; end++) assert.throws(() => decodeOfflineChatPacket(packet.subarray(0, end)));
  assert.throws(() => decodeOfflineChatPacket(Buffer.concat([packet, Buffer.from([0])])));
  const signed = Buffer.from(packet); signed[19] = 1;
  assert.throws(() => decodeOfflineChatPacket(signed), /Signed chat outside/);
});

test('only an explicit typed spam negative case can arm expected-disconnect IPC', () => {
  const payload = Buffer.concat([varInt(32), Buffer.from([10]), field(8, 'translate', utf('disconnect.spam')), Buffer.from([0])]);
  const observation = decodeDisconnect(payload);
  assert.equal(observation.translation, 'disconnect.spam');
  assert.equal(observation.payload_base64, payload.toString('base64'));
  assert.deepEqual(parseCommand({ id: 1, action: 'expect_disconnect', translation: 'disconnect.spam' }),
    { id: 1, action: 'expect_disconnect', translation: 'disconnect.spam' });
  for (const translation of ['disconnect.timeout', '', true]) assert.throws(() => parseCommand({ id: 1, action: 'expect_disconnect', translation }));
  assert.throws(() => decodeDisconnect(payload.subarray(0, -1)));
  assert.throws(() => decodeDisconnect(Buffer.concat([payload, Buffer.from([0])])));
});

test('26.3 actual hand swap sends PlayerAction ordinal 7 with packed origin, byte face and sequence', () => {
  assert.deepEqual(encodeSwapHands(19), Buffer.from([7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19]));
  assert.deepEqual(parseCommand({ id: 2, action: 'swap_hands' }), { id: 2, action: 'swap_hands' });
  assert.throws(() => encodeSwapHands(-1));
});

test('26.3 sign update is position, four bounded UTF strings, then BACK0/FRONT1 VarInt', () => {
  for (const slot of [0, 1] as const) {
    assert.deepEqual(encodeSignUpdate(-22, -64, 53, messages, slot),
      Buffer.concat([blockPos, ...messages.map(wireString), varInt(slot)]));
  }
  const maximum = ['界'.repeat(384), '', '', ''];
  assert.doesNotThrow(() => encodeSignUpdate(0, 0, 0, maximum, 1));
  for (const lines of [[], ['', '', ''], ['', '', '', '', ''], ['x'.repeat(385), '', '', ''], ['a\n', '', '', '']]) {
    assert.throws(() => encodeSignUpdate(0, 0, 0, lines, 1));
  }
  const command = { id: 1, action: 'sign_update', x: -22, y: -64, z: 53, messages, slot: 0 };
  assert.deepEqual(parseCommand(command), command);
  for (const slot of [-1, 2, true, undefined]) assert.throws(() => parseCommand({ ...command, slot }));
  assert.throws(() => parseCommand({ ...command, messages: ['not four'] }));
});

test('editor uses a trailing slot VarInt and BEData reads direct component strings and wax', () => {
  for (const slot of [0, 1]) assert.deepEqual(decodeSignPacket(Buffer.concat([varInt(61), blockPos, varInt(slot)])),
    { kind: 'editor', packet_id: 61, x: -22, y: -64, z: 53, slot });
  for (const block_entity_type of [7, 8]) {
    assert.deepEqual(decodeSignPacket(Buffer.concat([varInt(6), blockPos, varInt(block_entity_type), signNBT()])),
      { kind: 'block_entity', packet_id: 6, x: -22, y: -64, z: 53, block_entity_type, data: expectedSign });
  }
  assert.equal(decodeSignPacket(Buffer.concat([varInt(6), blockPos, varInt(1), Buffer.from([10, 0])])), null);
  // Embedded quotes are literal content, never old JSON component syntax.
  assert.equal(expectedSign.front_text.messages[1], '"quoted literal"');
});

test('sign codecs fail closed on truncated, trailing, malformed NBT and unsupported rich/filter state', () => {
  const valid = Buffer.concat([varInt(6), blockPos, varInt(7), signNBT()]);
  for (const bytes of [valid.subarray(0, -1), Buffer.concat([valid, Buffer.from([0])]),
    Buffer.concat([varInt(61), blockPos, varInt(2)]), Buffer.concat([varInt(61), blockPos]),
    Buffer.concat([varInt(6), blockPos, varInt(7), Buffer.from([0])]),
    Buffer.concat([varInt(6), blockPos, varInt(7), Buffer.from([10, 99, 0, 0, 0])]),
    Buffer.concat([varInt(6), blockPos, varInt(7), Buffer.from([10, 9, 0, 1, 120, 8]), i32(-1)])]) {
    assert.throws(() => decodeSignPacket(bytes));
  }
  const face = { messages, color: 'red', has_glowing_text: 1 };
  const base = { front_text: face, back_text: face, is_waxed: 0 };
  assert.equal(decodeSignData(base).is_waxed, false);
  for (const front_text of [{ ...face, messages: [{ text: 'rich' }, '', '', ''] },
    { ...face, filtered_messages: ['different', '', '', ''] }, { ...face, color: 'invalid' },
    { ...face, has_glowing_text: 2 }, { ...face, messages: ['', '', ''] }]) {
    assert.throws(() => decodeSignData({ ...base, front_text }));
  }
  assert.throws(() => decodeSignData({ ...base, is_waxed: 2 }));
  const largeList = (name: string) => field(9, name, Buffer.concat([Buffer.from([1]), i32(65536), Buffer.alloc(65536)]));
  assert.throws(() => decodeSignPacket(Buffer.concat([varInt(6), blockPos, varInt(7), Buffer.from([10]),
    largeList('a'), largeList('b'), largeList('c'), Buffer.from([0])])), /NBT node budget/);
});

function chunkWith(records: Buffer[], tail: Buffer = Buffer.alloc(6)): Buffer {
  // Opaque sections need not contain a terrain palette: this decoder only skips
  // the length. Heightmap map and lighting are still consumed to their boundary.
  return Buffer.concat([varInt(46), i32(-2), i32(3), varInt(1), varInt(1), varInt(2), Buffer.alloc(16),
    varInt(3), Buffer.from([99, 98, 97]), varInt(records.length), ...records, tail]);
}
const chunkRecord = (type: number, data: Buffer) => Buffer.concat([Buffer.from([0xa5]), i16(-64), varInt(type), data]);

test('chunk BE metadata decodes packed local XZ, signed Y and optional compound without terrain decoding', () => {
  const packet = chunkWith([chunkRecord(1, Buffer.from([0])), chunkRecord(7, signNBT()), chunkRecord(8, signNBT())]);
  const decoded = decodeChunkBlockEntities(packet);
  assert.equal(decoded.x, -2);
  assert.equal(decoded.z, 3);
  assert.equal(decoded.section_bytes, 3);
  assert.equal(decoded.block_entity_count, 3);
  assert.equal(decoded.decoded_bytes, packet.length);
  assert.deepEqual(decoded.signs, [7, 8].map(block_entity_type => ({ kind: 'chunk', packet_id: 46,
    x: -22, y: -64, z: 53, block_entity_type, data: expectedSign })));
  // Nonempty masks exercise the official byte-array BIT_SET codec. Empty masks
  // alone also fit the older long-array schema and cannot distinguish the two.
  const light = Buffer.concat([Buffer.from([2, 0, 1, 0, 1, 4, 2, 0, 2]),
    varInt(1), varInt(2048), Buffer.alloc(2048), Buffer.from([0])]);
  assert.equal(decodeChunkBlockEntities(chunkWith([], light)).signs.length, 0);
});

test('chunk probe rejects unsupported metadata, missing sign compounds, truncation and light overflows', () => {
  const valid = chunkWith([chunkRecord(7, signNBT())]);
  const prefix = Buffer.concat([varInt(46), i32(0), i32(0)]);
  for (const packet of [valid.subarray(0, -1), Buffer.concat([valid, Buffer.from([0])]),
    chunkWith([chunkRecord(7, Buffer.from([0]))]), chunkWith([chunkRecord(8, Buffer.from([1, 0]))]),
    Buffer.concat([prefix, varInt(7)]), Buffer.concat([prefix, varInt(-1)]),
    Buffer.concat([prefix, varInt(0), varInt(2097153)]),
    Buffer.concat([prefix, varInt(0), varInt(0), varInt(4097)]),
    chunkWith([], Buffer.concat([Buffer.alloc(4), varInt(1), varInt(2049)])),
    chunkWith([], varInt(513)),
    chunkWith([], Buffer.from([2, 1]))]) {
    assert.throws(() => decodeChunkBlockEntities(packet));
  }
});

test('sign interactions use physical SOUTH/NORTH hit faces while retaining the container default', () => {
  for (const face of [2, 3]) {
    const packet = encodeUseItemOn(1, 200, 1, 7, face);
    assert.equal(packet[9], face);
    assert.equal(packet.readFloatBE(14), 0.5);
    assert.equal(packet.readFloatBE(18), face === 2 ? 0 : 1);
    assert.deepEqual(parseCommand({ id: 1, action: 'use_item_on', x: 1, y: 200, z: 1, face }),
      { id: 1, action: 'use_item_on', x: 1, y: 200, z: 1, face });
  }
  assert.throws(() => encodeUseItemOn(1, 200, 1, 7, 6));
  assert.throws(() => parseCommand({ id: 1, action: 'use_item_on', x: 1, y: 200, z: 1, face: 0 }));
});

const runCommand = 'scoreboard players add @s signcmd 1';
const literalJSON = JSON.stringify({ text: 'RUN', click_event: { action: 'run_command', command: runCommand } });
const commandNBT = Buffer.concat([field(8, 'text', utf('RUN')), field(1, 'bold', Buffer.from([1])),
  field(8, 'color', utf('red')), field(10, 'click_event', Buffer.concat([
    field(8, 'action', utf('run_command')), field(8, 'command', utf(runCommand)), Buffer.from([0]),
  ])), Buffer.from([0])]);
const mixedNBT = Buffer.concat([Buffer.from([10]), i32(4), commandNBT,
  // Official ListTag.write emits TAG_Compound list entries and EXACT one-key
  // empty-name compounds for scalars. There is no per-entry tag byte here.
  field(8, '', utf('')), Buffer.from([0]),
  field(8, 'text', utf('third')), field(1, 'bold', Buffer.from([0])), Buffer.from([0]),
  field(8, '', utf(literalJSON)), Buffer.from([0]),
]);
const richNBT = () => Buffer.concat([Buffer.from([10]), field(10, 'front_text', Buffer.concat([
  field(9, 'messages', mixedNBT), field(9, 'filtered_messages', Buffer.concat([
    Buffer.from([8]), i32(4), ...['filtered zero', 'filtered one', 'filtered two', 'filtered three'].map(utf),
  ])), field(8, 'color', utf('red')), field(1, 'has_glowing_text', Buffer.from([1])), Buffer.from([0]),
])), field(10, 'back_text', faceNBT(['back', 'north', 'other', 'face'], 'blue', 0)),
field(1, 'is_waxed', Buffer.from([1])), field(1, 'allow_op_features', Buffer.from([1])), Buffer.from([0])]);

test('rich observation decodes actual heterogeneous ListTag envelopes without parsing literal JSON', () => {
  const payload = Buffer.concat([varInt(6), blockPos, varInt(7), richNBT()]);
  assert.throws(() => decodeSignPacket(payload)); // The strict plain mode stays strict.
  const decoded = decodeSignPacket(payload, 'rich');
  assert.ok(decoded && decoded.kind === 'block_entity');
  assert.equal(decoded.data, null);
  assert.deepEqual(decoded.rich_data?.front_text.messages, [
    { text: 'RUN', bold: true, color: 'red', click_event: { action: 'run_command', command: runCommand } },
    '', { text: 'third', bold: false }, literalJSON,
  ]);
  assert.deepEqual(decoded.rich_data?.front_text.filtered_messages, ['filtered zero', 'filtered one', 'filtered two', 'filtered three']);
  assert.equal(decoded.rich_data?.allow_op_features, true);
  assert.deepEqual(decoded.component_nbt, {
    front_text: { messages: [
      { text: 'RUN', bold: 1, color: 'red', click_event: { action: 'run_command', command: runCommand } },
      { '': '' }, { text: 'third', bold: 0 }, { '': literalJSON },
    ], filtered_messages: ['filtered zero', 'filtered one', 'filtered two', 'filtered three'], color: 'red', has_glowing_text: 1 },
    back_text: { messages: ['back', 'north', 'other', 'face'], color: 'blue', has_glowing_text: 0 }, is_waxed: 1, allow_op_features: 1,
  });
  const chunk = decodeChunkBlockEntities(chunkWith([chunkRecord(7, richNBT())]), 'rich');
  assert.deepEqual(chunk.signs[0]?.kind === 'chunk' && chunk.signs[0].rich_data, decoded.rich_data);
});

test('rich mode retains the original strict plain projection and byte boolean defaults', () => {
  const decoded = decodeSignPacket(Buffer.concat([varInt(6), blockPos, varInt(8), signNBT()]), 'rich');
  assert.ok(decoded && decoded.kind === 'block_entity');
  assert.deepEqual(decoded.data, expectedSign);
  assert.equal(decoded.rich_data?.allow_op_features, false);
  assert.deepEqual(decoded.rich_data?.front_text.filtered_messages, messages);
  assert.deepEqual(encodeSignUpdate(-22, -64, 53, [literalJSON, '', '', ''], 1),
    Buffer.concat([blockPos, ...[literalJSON, '', '', ''].map(wireString), varInt(1)]));
});

test('rich list unwrapping is one level at the entry boundary and never inside components', () => {
  const face = { messages: [{ '': { text: 'zero', type: 'text', bold: 1 } }, '', '', ''] };
  assert.deepEqual(decodeRichSignData({ front_text: face, back_text: face }).front_text.messages[0],
    { text: 'zero', type: 'text', bold: true });
  for (const line of [{ '': { '': 'recursive' } }, { '': '', text: 'sibling' },
    { text: { '': 'wrapped text' } }, { text: 'x', click_event: { '': { action: 'run_command', command: runCommand } } },
    { translate: 'unknown' }, { text: 'x', extra: ['unknown'] }, { text: 'x', clickEvent: { action: 'run_command', value: runCommand } },
    { text: 'x', click_event: { action: 'run_command', value: runCommand } }, { text: 'x', click_event: { action: 'open_url', command: runCommand } },
    { text: 'x', bold: 2 }, { text: 'x', color: 'invalid' }, { text: 'x', type: 'selector' }]) {
    assert.throws(() => decodeRichSignData({ front_text: { messages: [line, '', '', ''] }, back_text: face }), /front_text.messages\[0\]/);
  }
  assert.throws(() => decodeRichSignData({ front_text: { '': face }, back_text: face }));
});

test('rich sign mode rejects truncated and trailing wire data, malformed lists and flags', () => {
  const valid = Buffer.concat([varInt(6), blockPos, varInt(7), richNBT()]);
  for (const length of [valid.length - 1, valid.length - 8, 26, 40]) assert.throws(() => decodeSignPacket(valid.subarray(0, length), 'rich'));
  assert.throws(() => decodeSignPacket(Buffer.concat([valid, Buffer.from([0])]), 'rich'));
  const face = { messages: ['', '', '', ''] };
  for (const invalid of [{ ...face, filtered_messages: ['three', '', ''] }, { ...face, messages: [1, '', '', ''] },
    { ...face, has_glowing_text: true }]) assert.throws(() => decodeRichSignData({ front_text: invalid, back_text: face }));
  assert.throws(() => decodeRichSignData({ front_text: face, back_text: face, allow_op_features: 2 }));
  const threeLines = Buffer.concat([Buffer.from([10]), field(10, 'front_text', Buffer.concat([
    field(9, 'messages', Buffer.concat([Buffer.from([8]), i32(3), ...['one', 'two', 'three'].map(utf)])), Buffer.from([0]),
  ])), field(10, 'back_text', faceNBT(messages, 'blue', 0)), Buffer.from([0])]);
  assert.throws(() => decodeSignPacket(Buffer.concat([varInt(6), blockPos, varInt(7), threeLines]), 'rich'),
    /Sign \(-22,-64,53\): front_text.messages/);
});

test('nonop trigger IPC encodes the existing command packet payload and rejects injection/overflow', () => {
  const command = { id: 'fence', action: 'trigger', objective: 'signfence', value: 17 };
  assert.deepEqual(parseCommand(command), command);
  assert.deepEqual(encodeTrigger('signfence', 17), wireString('trigger signfence set 17'));
  for (const objective of ['', 'x\ny', 'x set 1', 'x'.repeat(17), '@s']) assert.throws(() => parseCommand({ ...command, objective }));
  for (const value of [2147483648, -2147483649, 1.2, '17', null]) assert.throws(() => parseCommand({ ...command, value }));
  assert.deepEqual(encodeTrigger('signfence', -2147483648), wireString('trigger signfence set -2147483648'));
});

test('system chat preserves fresh disabled-action overlay and permission-denial component identity', () => {
  const packet = (key: string, overlay: number) => Buffer.concat([varInt(124), Buffer.from([10]),
    field(8, 'translate', utf(key)), Buffer.from([0, overlay])]);
  const disabled = decodeSystemChat(packet('sign.click_actions_disabled', 1));
  assert.equal(disabled.overlay, true);
  assert.equal(disabled.text, 'sign.click_actions_disabled');
  assert.deepEqual(disabled.component_nbt, { translate: 'sign.click_actions_disabled' });
  const denied = packet('command.unknown.command', 0);
  assert.equal(decodeSystemChat(denied).overlay, false);
  for (const invalid of [denied.subarray(0, -1), Buffer.concat([denied, Buffer.from([0])]), packet('x', 2)]) {
    assert.throws(() => decodeSystemChat(invalid));
  }
});
