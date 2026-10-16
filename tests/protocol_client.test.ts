import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { deflateSync } from 'node:zlib';
import { COMMAND_ARGUMENT_TYPES, PROTOCOL, VERSION, decodeCommandTree, decodeCompressedPacket, decodeContainerPacket, decodeContainerTitle, decodeStatisticsPacket, parseCommand, readVarInt, varInt } from '../tools/parity/client.ts';
import { HORN_NAMES, crc32c, decodeHornContainerPacket, decodeHornCooldownPacket, decodeHornEntitySoundPacket, decodeHornLoginHeader, decodeHornRegistryPacket,
  encodeHornSelect, encodeHornStackHash, encodeHornSwap, encodeHornUse, hornRegistryEntries,
  instrumentHashPreimage, referenceName } from '../tools/parity/horns.ts';
import { SONG_PROFILES, JUKEBOX_ITEM, decodeSongRegistryPacket, decodeJukeboxContainerPacket,
  songRegistryEntries, songName, songReference, songHashPreimage, encodeJukeboxStackHash,
  encodeJukeboxSwap, encodeJukeboxUse, decodeJukeboxBlockEntityPacket, decodeJukeboxLevelEvent } from '../tools/parity/jukeboxes.ts';

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

test('26.3 award_stats retains received triples, including unmapped numeric IDs', () => {
  // Handwritten packet ID, map count, category/statistic/value triples. These bytes
  // are independent of the decoder and never imply actual server progress.
  const payload = Buffer.from('03 05 08 3f 03 08 00 01 08 ce 01 07 09 01 02 00 01 ff ff ff ff 07'.replaceAll(' ', ''), 'hex');
  assert.deepEqual(decodeStatisticsPacket(payload), { packet_id: 3, entries: [
    { category_id: 8, statistic_id: 63, value: 3 },
    { category_id: 8, statistic_id: 0, value: 1 },
    { category_id: 8, statistic_id: 206, value: 7 },
    { category_id: 9, statistic_id: 1, value: 2 },
    { category_id: 0, statistic_id: 1, value: 2147483647 },
  ], payload_bytes: payload.length, decoded_bytes: payload.length });
  assert.deepEqual(decodeStatisticsPacket(Buffer.from([3, 0])).entries, []);
  assert.deepEqual(parseCommand({ id: 'stats', action: 'request_stats' }), { id: 'stats', action: 'request_stats' });
});

test('award_stats fails closed on truncated, unbounded, negative, duplicate or trailing data', () => {
  for (const [label, hex] of [
    ['wrong packet', '04 00'], ['missing count', '03'], ['partial count', '03 80'],
    ['negative count', '03 ff ff ff ff 0f'], ['over budget count', '03 81 80 04'],
    ['count mismatch', '03 02 08 3f 03'], ['partial value', '03 01 08 3f 80'],
    ['negative category', '03 01 ff ff ff ff 0f 3f 03'],
    ['negative ID', '03 01 08 ff ff ff ff 0f 03'],
    ['negative value', '03 01 08 3f ff ff ff ff 0f'],
    ['overflow', '03 01 08 3f 80 80 80 80 10'],
    ['duplicate key', '03 02 08 3f 03 08 3f 05'], ['trailing bytes', '03 01 08 3f 03 00'],
  ] as const) assert.throws(() => decodeStatisticsPacket(Buffer.from(hex.replaceAll(' ', ''), 'hex')), label);
});

test('plain barrel title retains its translation key without a rendering claim', () => {
  assert.deepEqual(decodeContainerTitle({ translate: 'container.barrel' }), { translate: 'container.barrel' });
  assert.deepEqual(decodeContainerTitle({ translate: 'container.barrel', type: 'translatable' }),
    { translate: 'container.barrel', type: 'translatable' });
  assert.throws(() => decodeContainerTitle({ translate: 'container.barrel', with: ['invented'] }));
});

test('horn CRC32C uses independent known inputs and the exact typed UTF16 preimage', () => {
  assert.equal(crc32c(Buffer.alloc(0)), 0);
  assert.equal(crc32c(Buffer.from('123456789', 'ascii')), 0xe3069283);
  // Handwritten HashOps STRING encoding, not a captured Java hash/packet golden.
  assert.equal(instrumentHashPreimage('minecraft:sing_goat_horn').toString('hex'),
    '0c180000006d0069006e006500630072006100660074003a00730069006e0067005f0067006f00610074005f0068006f0072006e00');
});

function hornProfiles() {
  const indices = { admire: 4, call: 5, dream: 7, feel: 3, ponder: 0, seek: 2, sing: 1, yearn: 6 };
  return HORN_NAMES.map(name => ({ identifier: `minecraft:${name}_goat_horn`, data: {
    sound_event: `minecraft:item.goat_horn.sound.${indices[name]}`, use_duration: 7, range: 256,
    description: { translate: `instrument.minecraft.${name}_goat_horn` },
  } }));
}

test('horn reference IDs follow the captured registry order on each connection', () => {
  const entries = hornRegistryEntries(hornProfiles().reverse());
  const registry = { registry: 'minecraft:instrument' as const, entries, payload_bytes: 0, decoded_bytes: 0 };
  assert.equal(referenceName(registry, 1), 'minecraft:yearn_goat_horn');
  assert.equal(referenceName(registry, 2), 'minecraft:sing_goat_horn');
  assert.equal(referenceName(registry, 8), 'minecraft:admire_goat_horn');
  for (const marker of [0, -1, 9, 1.5, NaN]) assert.throws(() => referenceName(registry, marker));
});

test('horn registry refuses duplicate, missing, unknown and overridden builtin profiles', () => {
  const profiles = hornProfiles();
  assert.throws(() => hornRegistryEntries(profiles.slice(1)));
  assert.throws(() => hornRegistryEntries([...profiles.slice(1), profiles[1]!]));
  assert.throws(() => hornRegistryEntries([{ ...profiles[0]!, identifier: 'test:custom' }, ...profiles.slice(1)]));
  for (const change of [{ range: 255 }, { use_duration: 1 }, { durability_damage: 1 },
    { sound_event: 'minecraft:item.goat_horn.sound.0' }, { extra: 0 }, { description: { text: 'custom' } }]) {
    assert.throws(() => hornRegistryEntries([{ ...profiles[0]!, data: { ...profiles[0]!.data, ...change } }, ...profiles.slice(1)]));
  }
});

// Codec-derived inputs below exercise decoder boundaries, not runtime goldens.
function hornNbt(value: string | number | Record<string, unknown>): Buffer {
  if (typeof value === 'string') {
    const bytes = Buffer.from(value), length = Buffer.alloc(2);
    length.writeUInt16BE(bytes.length);
    return Buffer.concat([Buffer.from([8]), length, bytes]);
  }
  if (typeof value === 'number') return Buffer.concat([Buffer.from([5]), numericBytes('float', value)]);
  const fields = Object.entries(value).map(([key, child]) => {
    assert.ok(typeof child === 'string' || typeof child === 'number' || (typeof child === 'object' && child !== null && !Array.isArray(child)));
    const encoded = hornNbt(child as string | number | Record<string, unknown>);
    const name = Buffer.from(key), length = Buffer.alloc(2);
    length.writeUInt16BE(name.length);
    return Buffer.concat([encoded.subarray(0, 1), length, name, encoded.subarray(1)]);
  });
  return Buffer.concat([Buffer.from([10]), ...fields, Buffer.from([0])]);
}

function hornRegistryPacket() {
  return Buffer.concat([Buffer.from([7]), wireString('minecraft:instrument'), Buffer.from([8]),
    ...hornProfiles().reverse().map(entry => Buffer.concat([wireString(entry.identifier), Buffer.from([1]), hornNbt(entry.data)]))]);
}

test('horn configuration requires present complete profiles and consumes exactly the packet', () => {
  const packet = hornRegistryPacket(), registry = decodeHornRegistryPacket(packet);
  assert.equal(registry.decoded_bytes, packet.length);
  assert.equal(referenceName(registry, 2), 'minecraft:sing_goat_horn');
  for (const end of [0, 1, 7, packet.length - 1]) assert.throws(() => decodeHornRegistryPacket(packet.subarray(0, end)));
  assert.throws(() => decodeHornRegistryPacket(Buffer.concat([packet, Buffer.from([0])])));
  const start = 1 + wireString('minecraft:instrument').length;
  for (const count of [0, 7, 9, 255]) {
    const broken = Buffer.from(packet);
    broken[start] = count;
    assert.throws(() => decodeHornRegistryPacket(broken));
  }
  for (const present of [0, 2]) {
    const broken = Buffer.from(packet);
    broken[start + 1 + wireString('minecraft:yearn_goat_horn').length] = present;
    assert.throws(() => decodeHornRegistryPacket(broken));
  }
  const duplicateNbt = Buffer.concat([Buffer.from([7]), wireString('minecraft:instrument'), Buffer.from([8]),
    wireString('minecraft:yearn_goat_horn'), Buffer.from([1]),
    Buffer.from('0a0300016100000000030001610000000000', 'hex')]);
  assert.throws(() => decodeHornRegistryPacket(duplicateNbt), /Duplicate horn NBT key/u);
});

test('horn registry retains generated Double widths and rejects nonfinite or truncated Double fields', () => {
  // Actual retained source bytes, not a captured runtime packet. registry.rs:20
  // Git blob 942994bcde0409a2558c4002e9a6c02a07ab77ae at commit 66b593ec; CRegistryData
  // writes these entry.data bytes unchanged. Pin provenance before extracting.
  const source = readFileSync(new URL('../vendor/pumpkin/crates/pumpkin-data/src/generated/registry.rs', import.meta.url));
  assert.equal(createHash('sha256').update(source).digest('hex'), '32ef62fa967de9674f58105c1490c47157f305ab5740e8aede9dd6d8f705144d');
  const matches = [...source.toString('utf8').matchAll(/name : "admire_goat_horn" , data : b"([^"\r\n]{1,1024})"/gu)];
  assert.equal(matches.length, 1);
  const literal = matches[0]![1]!;
  // Only the escapes present in this retained Rust byte literal; fail on gaps.
  const tokens = [...literal.matchAll(/\\x([0-9A-F]{2})|\\([n0t])|([^\\])/gu)];
  assert.equal(tokens.map(([token]) => token).join(''), literal);
  const data = Buffer.from(tokens.map(([, hex, escape, ascii]) => {
    if (hex !== undefined) return Number.parseInt(hex, 16);
    if (escape === 'n') return 10;
    if (escape === '0') return 0;
    if (escape === 't') return 9;
    assert.ok(ascii !== undefined);
    return ascii.charCodeAt(0);
  }));
  const profiles = hornProfiles();
  const packet = Buffer.concat([Buffer.from([7]), wireString('minecraft:instrument'), Buffer.from([8]),
    ...profiles.map((entry, index) => Buffer.concat([wireString(entry.identifier), Buffer.from([1]), index === 0 ? data : hornNbt(entry.data)]))]);
  const registry = decodeHornRegistryPacket(packet);
  assert.equal(registry.decoded_bytes, packet.length);
  assert.deepEqual(registry.entries[0]!.data, profiles[0]!.data);
  for (const [marker, expected] of [['\x06\0\x0Cuse_duration', 7], ['\x06\0\x05range', 256]] as const) {
    const field = Buffer.from(marker, 'latin1'), start = packet.indexOf(field);
    assert.notEqual(start, -1);
    const offset = start + field.length;
    assert.equal(packet.readDoubleBE(offset), expected);
    for (let available = 0; available < 8; available++) {
      assert.throws(() => decodeHornRegistryPacket(packet.subarray(0, offset + available)), /Truncated horn packet/u);
    }
    for (const value of [NaN, Infinity, -Infinity]) {
      const broken = Buffer.from(packet);
      broken.writeDoubleBE(value, offset);
      assert.throws(() => decodeHornRegistryPacket(broken), /Nonfinite horn double/u);
    }
  }
});

test('horn slot codec binds holders and preserves removal while generic decoding stays off', () => {
  const registry = decodeHornRegistryPacket(hornRegistryPacket());
  // slot packet20, player0,state1,slot36, count1,item1504, add2/remove0,
  // instrument63->received reference2 (Sing), followed by damage3=7 sentinel.
  const packet = Buffer.from('140001002401e00b02003f020307', 'hex');
  const result = decodeHornContainerPacket(packet, registry);
  assert.ok(result.kind === 'slot');
  assert.deepEqual(result.stack, { item_id: 1504, count: 1,
    components: { 'minecraft:instrument': 'minecraft:sing_goat_horn', 'minecraft:damage': 7 } });
  assert.throws(() => decodeContainerPacket(packet));
  const removal = decodeHornContainerPacket(Buffer.from('140001002401e00b00013f', 'hex'), registry);
  assert.ok(removal.kind === 'slot');
  assert.deepEqual(removal.stack, { item_id: 1504, count: 1, removed_components: ['minecraft:instrument'] });
  for (let length = 0; length < packet.length; length++) assert.throws(() => decodeHornContainerPacket(packet.subarray(0, length), registry));
  for (const hex of [
    '140001002401e00b01003f00', // direct holder unsupported
    '140001002401e00b01003f09', // unknown holder
    '140001002401e00b02003f023f02', // duplicate added component
    '140001002401e00b01013f023f', // conflicting removal
    '140001002401e00b01007f00', // unknown component
    '140001002401e00b00023f3f', // duplicate/excess removed count
    '140001002401e00b0400', // excess added count
    '1200012e00', // truncated 46-slot content
  ]) assert.throws(() => decodeHornContainerPacket(Buffer.from(hex, 'hex'), registry));
  assert.throws(() => decodeHornContainerPacket(Buffer.concat([packet, Buffer.from([0])]), registry));
});

test('horn cooldown and use actions retain exact fields and reject malformed boundaries', () => {
  const packet = Buffer.concat([Buffer.from([22]), wireString('minecraft:goat_horn'), Buffer.from([0x8c, 1])]);
  assert.deepEqual(decodeHornCooldownPacket(packet), { packet_id: 22, group: 'minecraft:goat_horn', ticks: 140,
    payload_bytes: packet.length, decoded_bytes: packet.length });
  for (let length = 0; length < packet.length; length++) assert.throws(() => decodeHornCooldownPacket(packet.subarray(0, length)));
  assert.throws(() => decodeHornCooldownPacket(Buffer.concat([packet, Buffer.from([0])])));
  assert.throws(() => decodeHornCooldownPacket(Buffer.concat([Buffer.from([22]), wireString('test:unknown'), Buffer.from([1])])));
  assert.equal(encodeHornUse(0, 1, 90, 0).toString('hex'), '000142b4000000000000');
  assert.equal(encodeHornUse(1, 128, 0, -90).toString('hex'), '01800100000000c2b40000');
  assert.equal(encodeHornSelect(8).toString('hex'), '0008');
  assert.throws(() => encodeHornUse(2, 1, 0, 0));
  assert.throws(() => encodeHornUse(0, -1, 0, 0));
  assert.throws(() => encodeHornUse(0, 1, Infinity, 0));
  assert.throws(() => encodeHornUse(0, 1, 0, 91));
  for (const hotbar of [-1, 9, 1.5]) assert.throws(() => encodeHornSelect(hotbar));
  assert.deepEqual(parseCommand({ id: 1, action: 'horn_use', hand: 0, other_hotbar: 4 }),
    { id: 1, action: 'horn_use', hand: 0, other_hotbar: 4 });
  assert.throws(() => parseCommand({ id: 1, action: 'horn_use', hand: 1, other_hotbar: 4 }));
  assert.throws(() => parseCommand({ id: 1, action: 'horn_select', hotbar: 9 }));
});

test('horn named control components remain opaque or typed and reject altered profiles', () => {
  const registry = decodeHornRegistryPacket(hornRegistryPacket());
  const custom = Buffer.concat([Buffer.from('140001002401e00b010000', 'hex'), hornNbt({ instrument: 'minecraft:sing_goat_horn' })]);
  const decoded = decodeHornContainerPacket(custom, registry);
  assert.ok(decoded.kind === 'slot');
  assert.deepEqual(decoded.stack, { item_id: 1504, count: 1,
    components: { 'minecraft:custom_data': { instrument: 'minecraft:sing_goat_horn' } } });
  const cooldown = Buffer.concat([Buffer.from('140001002401e00b02001a3f80000001', 'hex'), wireString('test:horn_group'), Buffer.from([63, 2])]);
  const control = decodeHornContainerPacket(cooldown, registry);
  assert.ok(control.kind === 'slot');
  assert.deepEqual(control.stack, { item_id: 1504, count: 1, components: {
    'minecraft:use_cooldown': { seconds: 1, cooldown_group: 'test:horn_group' }, 'minecraft:instrument': 'minecraft:sing_goat_horn',
  } });
  for (let length = 0; length < cooldown.length; length++) assert.throws(() => decodeHornContainerPacket(cooldown.subarray(0, length), registry));
  for (const data of [{ instrument: 'test:custom' }, { instrument: 'minecraft:sing_goat_horn', extra: 1 }]) {
    assert.throws(() => decodeHornContainerPacket(Buffer.concat([custom.subarray(0, 11), hornNbt(data)]), registry));
  }
  const bad = Buffer.from(cooldown);
  bad[15] = 2; // Optional group boolean must be exactly1 in this named control.
  assert.throws(() => decodeHornContainerPacket(bad, registry));
});

test('horn entity sound118 consumes holder category entity floats seed and rejects direct or truncated data', () => {
  // Codec-derived layout; sound1 is intentionally not a runtime horn mapping.
  const packet = Buffer.from('76020207418000003f8000000102030405060708', 'hex');
  assert.deepEqual(decodeHornEntitySoundPacket(packet), { packet_id: 118, registry_id: 1, source: 2, entity_id: 7,
    volume: 16, pitch: 1, seed: '0102030405060708', payload_bytes: 20, decoded_bytes: 20 });
  for (let length = 0; length < packet.length; length++) assert.throws(() => decodeHornEntitySoundPacket(packet.subarray(0, length)));
  assert.throws(() => decodeHornEntitySoundPacket(Buffer.concat([packet, Buffer.from([0])])));
  for (const [index, value] of [[0, 119], [1, 0], [2, 10]] as const) {
    const broken = Buffer.from(packet);
    broken[index] = value;
    assert.throws(() => decodeHornEntitySoundPacket(broken));
  }
  const nonfinite = Buffer.from(packet);
  nonfinite.writeFloatBE(Infinity, 4);
  assert.throws(() => decodeHornEntitySoundPacket(nonfinite));
});

test('horn predictions use one fixed-width instrument checksum and never hash opaque controls', () => {
  const stack = { item_id: 1504, count: 1, components: { 'minecraft:instrument': 'minecraft:sing_goat_horn' } };
  const hash = encodeHornStackHash(stack);
  assert.equal(hash.length, 11); // present,item1504,count,added1,id63,int32,removed0
  assert.equal(hash.subarray(0, 6).toString('hex'), '01e00b01013f');
  assert.equal(hash[10], 0);
  assert.equal(encodeHornStackHash({ item_id: 1504, count: 1 }).toString('hex'), '01e00b010000');
  assert.equal(encodeHornStackHash({ item_id: 1504, count: 1, removed_components: ['minecraft:instrument'] }).toString('hex'), '01e00b0100013f');
  for (const components of [{ 'minecraft:instrument': 'test:unknown' }, { 'minecraft:custom_data': { instrument: 'minecraft:sing_goat_horn' } },
    { 'minecraft:use_cooldown': { seconds: 1, cooldown_group: 'test:horn_group' } }]) {
    assert.throws(() => encodeHornStackHash({ item_id: 1504, count: 1, components }));
  }
  const slots = Array.from({ length: 63 }, (_, slot) => slot === 6 ? stack : null), before = JSON.stringify(slots);
  const swap = encodeHornSwap(1, 2, 6, 0, slots);
  assert.equal(swap.subarray(0, 7).toString('hex'), '01020006000202');
  assert.equal(swap.subarray(7, 12).toString('hex'), '0006000036');
  assert.equal(JSON.stringify(slots), before);
  assert.throws(() => encodeHornSwap(1, 2, 6, 0, slots.slice(1)));
});

test('horn Login header retains signed fixed-width entity ID and an explicitly undecoded remainder', () => {
  // Synthetic header and arbitrary remainder, deliberately not a full Login fixture.
  const packet = Buffer.from('3201020304aabbcc', 'hex');
  assert.deepEqual(decodeHornLoginHeader(packet), { packet_id: 50, entity_id: 0x01020304, header_only: true,
    header_bytes: 5, payload_bytes: 8, undecoded_bytes: 3, undecoded_base64: Buffer.from('aabbcc', 'hex').toString('base64'),
    payload_base64: packet.toString('base64') });
  const header = decodeHornLoginHeader(packet.subarray(0, 5));
  assert.equal(header.header_only, true);
  assert.equal(header.undecoded_bytes, 0); // Never promotes a header to full Login validation.
  for (let length = 0; length < 5; length++) assert.throws(() => decodeHornLoginHeader(packet.subarray(0, length)));
  for (const hex of ['3101020304', '32ffffffff', '3280000000']) {
    assert.throws(() => decodeHornLoginHeader(Buffer.from(hex, 'hex')));
  }
  assert.throws(() => decodeHornLoginHeader(Buffer.alloc(65537)));
});

function wireString(value: string): Buffer {
  const bytes = Buffer.from(value, 'utf8');
  return Buffer.concat([varInt(bytes.length), bytes]);
}

type WireCommandNode = {
  flags: number; children?: number[]; redirect?: number; name?: string;
  parser?: number; properties?: Buffer; suggestion?: string;
};

function commandPacket(nodes: WireCommandNode[], root = 0): Buffer {
  return Buffer.concat([varInt(nodes.length), ...nodes.flatMap(node => [
    Buffer.from([node.flags]), varInt(node.children?.length ?? 0), ...(node.children ?? []).map(varInt),
    ...(node.redirect === undefined ? [] : [varInt(node.redirect)]),
    ...(node.name === undefined ? [] : [wireString(node.name)]),
    ...(node.parser === undefined ? [] : [varInt(node.parser)]),
    node.properties ?? Buffer.alloc(0),
    ...(node.suggestion === undefined ? [] : [wireString(node.suggestion)]),
  ]), varInt(root)]);
}

function numericBytes(type: 'float' | 'double' | 'int' | 'long', ...values: (number | bigint)[]): Buffer {
  const size = type === 'float' || type === 'int' ? 4 : 8;
  const bytes = Buffer.alloc(values.length * size);
  for (const [index, value] of values.entries()) {
    const offset = index * size;
    if (type === 'long') bytes.writeBigInt64BE(BigInt(value), offset);
    else if (type === 'float') bytes.writeFloatBE(Number(value), offset);
    else if (type === 'double') bytes.writeDoubleBE(Number(value), offset);
    else bytes.writeInt32BE(Number(value), offset);
  }
  return bytes;
}

test('26.3 command decoder preserves restricted flags, suggestions, long bounds and legal root redirects', () => {
  // Source-backed 26.3 wire layouts, including restricted bit0x20 and exact signed-i64 limits.
  const packet = commandPacket([
    { flags: 0, children: [1, 4, 6] },
    { flags: 1 | 32, name: 'execute', children: [2] },
    { flags: 2 | 4 | 16, name: 'targets', parser: 6, properties: Buffer.from([3]),
      suggestion: 'minecraft:ask_server', children: [3] },
    { flags: 1 | 8, name: 'run', redirect: 0 },
    { flags: 1, name: 'scoreboard', children: [5] },
    { flags: 2 | 4, name: 'value', parser: 4,
      properties: Buffer.concat([Buffer.from([3]), numericBytes('long', -(1n << 63n), (1n << 63n) - 1n)]) },
    { flags: 1 | 8, name: 'alias', redirect: 4 },
  ]);
  const tree = decodeCommandTree(packet);
  assert.deepEqual(tree.roots, ['alias', 'execute', 'scoreboard']);
  assert.equal(tree.node_count, 7);
  assert.equal(tree.child_edge_count, 6);
  assert.equal(tree.root_nodes, 1);
  assert.equal(tree.literal_nodes, 4);
  assert.equal(tree.argument_nodes, 2);
  assert.equal(tree.executable_nodes, 2);
  assert.equal(tree.restricted_nodes, 1);
  assert.equal(tree.redirect_nodes, 2);
  assert.equal(tree.suggestion_nodes, 1);
  assert.equal(tree.nodes[1]?.restricted, true);
  assert.equal(tree.nodes[2]?.suggestion_provider, 'minecraft:ask_server');
  assert.deepEqual(tree.nodes[2]?.properties, { kind: 'entity', single: true, players_only: true });
  assert.deepEqual(tree.nodes[5]?.properties, { kind: 'number', flags: 3,
    min: '-9223372036854775808', max: '9223372036854775807' });
  assert.deepEqual(tree.literal_paths, ['alias', 'execute', 'execute <targets> run', 'scoreboard']);
  assert.deepEqual(tree.argument_parsers, ['brigadier:long', 'minecraft:entity']);
  assert.deepEqual(tree.parser_counts, { 'minecraft:entity': 1, 'brigadier:long': 1 });
  assert.equal(tree.decoded_bytes, packet.length);
  assert.doesNotThrow(() => JSON.stringify(tree));
});

test('all 62 official 26.3 parser IDs consume their exact property layouts', () => {
  const properties = new Map<number, Buffer>([
    [1, Buffer.concat([Buffer.from([3]), numericBytes('float', -1.5, 2.25)])],
    [2, Buffer.concat([Buffer.from([2]), numericBytes('double', 0.125)])],
    [3, Buffer.concat([Buffer.from([1]), numericBytes('int', -1234567)])],
    [4, Buffer.from([0])], [5, varInt(2)], [6, Buffer.from([2])], [31, Buffer.from([1])],
    [43, numericBytes('int', -40)],
    ...[44, 45, 46, 47, 48].map(id => [id, wireString('minecraft:worldgen/biome')] as const),
  ]);
  const nodes: WireCommandNode[] = [{ flags: 0, children: COMMAND_ARGUMENT_TYPES.map((_, id) => id + 1) },
    ...COMMAND_ARGUMENT_TYPES.map((_, id) => ({ flags: 2, name: `p${id}`, parser: id,
      properties: properties.get(id) ?? Buffer.alloc(0) }))];
  const packet = commandPacket(nodes);
  const tree = decodeCommandTree(packet);
  assert.equal(tree.argument_nodes, 62);
  assert.equal(tree.argument_parsers.length, 62);
  assert.equal(tree.nodes[1]?.parser, 'brigadier:bool');
  assert.equal(tree.nodes[49]?.parser, 'minecraft:resource_selector');
  assert.equal(tree.nodes[62]?.parser, 'minecraft:uuid');
  assert.deepEqual(tree.nodes[2]?.properties, { kind: 'number', flags: 3, min: -1.5, max: 2.25 });
  assert.deepEqual(tree.nodes[3]?.properties, { kind: 'number', flags: 2, min: null, max: 0.125 });
  assert.deepEqual(tree.nodes[4]?.properties, { kind: 'number', flags: 1, min: -1234567, max: null });
  assert.deepEqual(tree.nodes[5]?.properties, { kind: 'number', flags: 0, min: null, max: null });
  assert.deepEqual(tree.nodes[6]?.properties, { kind: 'string', mode: 'greedy' });
  assert.deepEqual(tree.nodes[7]?.properties, { kind: 'entity', single: false, players_only: true });
  assert.deepEqual(tree.nodes[32]?.properties, { kind: 'score_holder', multiple: true });
  assert.deepEqual(tree.nodes[44]?.properties, { kind: 'time', minimum: -40 });
  for (const id of [44, 45, 46, 47, 48]) {
    assert.deepEqual(tree.nodes[id + 1]?.properties, { kind: 'registry', registry: 'minecraft:worldgen/biome' });
  }
  assert.equal(tree.decoded_bytes, packet.length);
});

test('command tree rejects invalid references, graph cycles, trailing bytes and truncation', () => {
  const valid: WireCommandNode[] = [{ flags: 0, children: [1] }, { flags: 1 | 4, name: 'test' }];
  const malformed = [
    commandPacket([{ flags: 0, children: [-1] }]), commandPacket([{ flags: 0, children: [1] }]),
    commandPacket([{ flags: 0, children: [1] }, { flags: 1, name: 'cycle', children: [1] }]),
    commandPacket([{ flags: 0, children: [1] }, { flags: 1 | 8, name: 'cycle', redirect: 1 }]),
    commandPacket([{ flags: 0, children: [1] }, { flags: 1 | 8, name: 'missing', redirect: 2 }]),
    commandPacket([{ flags: 0, children: [1] }, { flags: 1, name: 'cycle', children: [0] }]),
    commandPacket(valid, 1), commandPacket(valid, 2), commandPacket(valid, -1),
    Buffer.concat([commandPacket(valid), Buffer.from([0])]),
    varInt(0), varInt(16385),
  ];
  for (const packet of malformed) assert.throws(() => decodeCommandTree(packet));
  const packet = commandPacket(valid);
  for (let length = 0; length < packet.length; length++) {
    assert.throws(() => decodeCommandTree(packet.subarray(0, length)));
  }
});

test('command parser properties reject unsupported IDs, truncated fields and invalid enum/identifier values', () => {
  const argument = (parser: number, properties: Buffer) => commandPacket([
    { flags: 0, children: [1] }, { flags: 2, name: 'value', parser, properties },
  ]);
  for (const [id, properties] of [
    [-1, Buffer.alloc(0)], [62, Buffer.alloc(0)],
    [4, Buffer.from([3, 1])], [5, varInt(3)],
    [43, Buffer.from([1])], [44, wireString('BAD:registry')],
  ] as const) assert.throws(() => decodeCommandTree(argument(id, properties)));
});

test('command path expansion is explicitly bounded rather than truncating a tree', () => {
  const nodes: WireCommandNode[] = [{ flags: 0, children: [1] }];
  for (let index = 1; index <= 129; index++) {
    nodes.push({ flags: 1, name: `level${index}`, children: index < 129 ? [index + 1] : [] });
  }
  assert.throws(() => decodeCommandTree(commandPacket(nodes)), /probe support budget/u);
});

test('Java-accepted unused nodes, duplicate children and root children are observations rather than failures', () => {
  const unreachable = decodeCommandTree(commandPacket([
    { flags: 0, children: [1] }, { flags: 1, name: 'visible' }, { flags: 1, name: 'unused' },
  ]));
  assert.equal(unreachable.unreachable_nodes, 1);
  assert.deepEqual(unreachable.roots, ['visible']);

  // More child references than nodes is valid: NodeResolver adds the same leaf repeatedly.
  const duplicateIndices = decodeCommandTree(commandPacket([
    { flags: 0, children: [1, 1, 1] }, { flags: 1, name: 'same' },
  ]));
  assert.equal(duplicateIndices.duplicate_child_indices, 2);
  assert.equal(duplicateIndices.duplicate_child_names, 2);
  assert.equal(duplicateIndices.child_edge_count, 3);
  assert.deepEqual(duplicateIndices.roots, ['same']);
  assert.deepEqual(duplicateIndices.literal_paths, ['same']);
  const duplicateNames = decodeCommandTree(commandPacket([
    { flags: 0, children: [1, 2] }, { flags: 1, name: 'same' }, { flags: 1, name: 'same' },
  ]));
  assert.equal(duplicateNames.duplicate_child_indices, 0);
  assert.equal(duplicateNames.duplicate_child_names, 1);
  assert.deepEqual(duplicateNames.roots, ['same']);

  // NodeResolver resolves a child root, then explicitly omits it from the parent's children.
  const rootChild = decodeCommandTree(commandPacket([
    { flags: 0, children: [1, 2] }, { flags: 0, children: [3] },
    { flags: 1, name: 'visible' }, { flags: 1, name: 'hidden_under_child_root' },
  ]));
  assert.equal(rootChild.root_child_edges, 1);
  assert.equal(rootChild.root_nodes, 2);
  assert.deepEqual(rootChild.roots, ['visible']);
  assert.deepEqual(rootChild.literal_paths, ['visible']);
  // validateEntries still checks unreachable nodes for cycles.
  assert.throws(() => decodeCommandTree(commandPacket([
    { flags: 0 }, { flags: 1, name: 'unused_cycle', children: [1] },
  ])), /Cyclic command children graph/u);
});

test('Java masking and UTF-8 replacement preserve unusual accepted values without lossy JSON bounds', () => {
  const tree = decodeCommandTree(commandPacket([
    { flags: 64, children: [1, 2, 3, 4, 5, 6, 7] },
    { flags: 1 | 16 | 128, name: '' }, // Non-argument suggestion bit carries no field.
    { flags: 2, name: 'float', parser: 1,
      properties: Buffer.concat([Buffer.from([128 | 3]), numericBytes('float', NaN, Infinity)]) },
    { flags: 2, name: 'integer', parser: 3,
      properties: Buffer.concat([Buffer.from([3]), numericBytes('int', 10, -10)]) },
    { flags: 2, name: 'entities', parser: 6, properties: Buffer.from([252]) },
    { flags: 2, name: 'scores', parser: 31, properties: Buffer.from([255]) },
    { flags: 2, name: 'no_bounds', parser: 2, properties: Buffer.from([128]) },
    { flags: 3 }, // Java read() returns a null stub for type3; NodeResolver builds a root.
  ]));
  assert.equal(tree.unused_flag_nodes, 2);
  assert.equal(tree.reserved_type_nodes, 1);
  assert.equal(tree.root_child_edges, 1);
  assert.deepEqual(tree.nodes[2]?.properties, { kind: 'number', flags: 131, min: 'NaN', max: 'Infinity' });
  assert.deepEqual(tree.nodes[3]?.properties, { kind: 'number', flags: 3, min: 10, max: -10 });
  assert.deepEqual(tree.nodes[4]?.properties, { kind: 'entity', single: false, players_only: false });
  assert.deepEqual(tree.nodes[5]?.properties, { kind: 'score_holder', multiple: true });
  assert.deepEqual(tree.nodes[6]?.properties, { kind: 'number', flags: 128, min: null, max: null });
  assert.ok(tree.roots.includes(''));
  const json: unknown = JSON.parse(JSON.stringify(tree));
  assert.ok(json);
  const replacement = decodeCommandTree(Buffer.from([2, 0, 1, 1, 1, 0, 1, 255, 0]));
  assert.deepEqual(replacement.roots, ['\uFFFD']);
  // JDK25 UTF_8.Decoder replaces the complete surrogate sequence, or its truncated
  // two-byte prefix, once; WHATWG/Node decoding alone would produce 3/2 replacements.
  for (const name of [Buffer.from([0xed, 0xa0, 0x80]), Buffer.from([0xed, 0xa0])]) {
    const packet = Buffer.concat([Buffer.from([2, 0, 1, 1, 1, 0]), varInt(name.length), name, varInt(0)]);
    assert.deepEqual(decodeCommandTree(packet).roots, ['\uFFFD']);
  }
});

// Synthetic codec inputs only. No numeric Java hash or actual packet golden.
function songProfiles() {
  return Object.entries(SONG_PROFILES).map(([name, [seconds, comparator]]) => ({
    identifier: `minecraft:${name}`, data: { sound_event: `minecraft:music_disc.${name}`,
      length_in_seconds: seconds, comparator_output: comparator,
      description: { translate: `jukebox_song.minecraft.${name}` } },
  }));
}

function songRegistryPacket() {
  return Buffer.concat([varInt(7), wireString('minecraft:jukebox_song'), varInt(22),
    ...songProfiles().reverse().map(entry => Buffer.concat([wireString(entry.identifier), Buffer.from([1]), hornNbt(entry.data)]))]);
}

test('jukebox registry binds all 22 identities to the received order and refuses direct holders', () => {
  const payload = songRegistryPacket(), registry = decodeSongRegistryPacket(payload);
  assert.equal(registry.decoded_bytes, payload.length);
  for (const [index, profile] of songProfiles().reverse().entries()) {
    assert.equal(songReference(registry, index + 1), profile.identifier);
    assert.equal(registry.entries[index]?.registry_id, index);
  }
  for (const reference of [0, -1, 23, 1.1, Infinity, NaN]) assert.throws(() => songReference(registry, reference));
  for (let end = 0; end < payload.length; end++) assert.throws(() => decodeSongRegistryPacket(payload.subarray(0, end)));
  assert.throws(() => decodeSongRegistryPacket(Buffer.concat([payload, Buffer.from([0])])));
  assert.throws(() => decodeSongRegistryPacket(Buffer.alloc(65537)));
  const header = 1 + wireString('minecraft:jukebox_song').length;
  for (const count of [0, 21, 23, 255]) {
    const broken = Buffer.from(payload); broken[header] = count;
    assert.throws(() => decodeSongRegistryPacket(broken));
  }
  const present = header + 1 + wireString(songProfiles().reverse()[0]!.identifier).length;
  for (const byte of [0, 2]) {
    const broken = Buffer.from(payload); broken[present] = byte;
    assert.throws(() => decodeSongRegistryPacket(broken));
  }
});

test('jukebox profiles reject custom fields, namespaces, duplicate names and nonfinite lengths', () => {
  const profiles = songProfiles(), first = profiles[0]!;
  assert.throws(() => songRegistryEntries(profiles.slice(1)));
  assert.throws(() => songRegistryEntries([...profiles.slice(1), profiles[1]!]));
  for (const identifier of ['test:11', 'minecraft:missing', '11', ':11']) {
    assert.throws(() => songRegistryEntries([{ ...first, identifier }, ...profiles.slice(1)]));
  }
  for (const change of [{ length_in_seconds: NaN }, { length_in_seconds: Infinity }, { length_in_seconds: -1 },
    { length_in_seconds: 0 }, { comparator_output: 16 }, { comparator_output: 1.1 }, { extra: 0 },
    { sound_event: 'minecraft:music_disc.cat' }, { description: { text: 'replacement' } }]) {
    assert.throws(() => songRegistryEntries([{ ...first, data: { ...first.data, ...change } }, ...profiles.slice(1)]));
  }
  const duplicate = Buffer.concat([varInt(7), wireString('minecraft:jukebox_song'), varInt(22),
    wireString(first.identifier), Buffer.from([1]), Buffer.from('0a0300016100000000030001610000000000', 'hex')]);
  assert.throws(() => decodeSongRegistryPacket(duplicate), /Duplicate/u);
  const payload = songRegistryPacket(), marker = Buffer.from('\x05\0\x11length_in_seconds', 'latin1');
  const offset = payload.indexOf(marker) + marker.length;
  assert.ok(offset >= marker.length);
  for (const value of [Infinity, -Infinity, NaN]) {
    const broken = Buffer.from(payload); broken.writeFloatBE(value, offset);
    assert.throws(() => decodeSongRegistryPacket(broken), /Nonfinite/u);
  }
});

test('jukebox Identifier aliases hash as canonical strings, opaque controls cannot be predicted', () => {
  const preimage = songHashPreimage('11');
  assert.equal(preimage.toString('hex'), '0c0c0000006d0069006e006500630072006100660074003a0031003100');
  assert.deepEqual(songHashPreimage(':11'), preimage);
  assert.equal(songName('cat'), 'minecraft:cat');
  for (const name of [null, { song: 'minecraft:11' }, 'test:11', 'minecraft:no', 'Minecraft:11']) {
    assert.throws(() => songHashPreimage(name));
  }
  const disc = { item_id: JUKEBOX_ITEM, count: 1 };
  assert.deepEqual(encodeJukeboxStackHash(null), Buffer.from([0]));
  assert.deepEqual(encodeJukeboxStackHash(disc), Buffer.concat([Buffer.from([1]), varInt(JUKEBOX_ITEM), Buffer.from([1, 0, 0])]));
  assert.throws(() => encodeJukeboxStackHash({ ...disc, components: { 'minecraft:custom_data': { jukebox_playable: 'minecraft:cat' } } }));
  assert.throws(() => encodeJukeboxStackHash({ ...disc, components: { 'minecraft:jukebox_playable': 'minecraft:cat' }, removed_components: ['minecraft:jukebox_playable'] }));
  for (const count of [0, 2, NaN]) assert.throws(() => encodeJukeboxStackHash({ ...disc, count }));
});

test('jukebox inventory reads reference patches, default elision and removal without a generic decoder', () => {
  const registry = decodeSongRegistryPacket(songRegistryPacket());
  const packet = (patch: Buffer) => Buffer.concat([Buffer.from([20, 0, 1, 0, 36, 1]), varInt(JUKEBOX_ITEM), patch]);
  const explicit = packet(Buffer.from([1, 0, 66, 1]));
  const decoded = decodeJukeboxContainerPacket(explicit, registry);
  assert.equal(decoded.kind, 'slot');
  if (decoded.kind !== 'slot') throw new Error('Expected slot');
  assert.deepEqual(decoded.stack, { item_id: JUKEBOX_ITEM, count: 1, components: { 'minecraft:jukebox_playable': songReference(registry, 1) } });
  for (const end of [0, 1, 6, explicit.length - 1]) assert.throws(() => decodeJukeboxContainerPacket(explicit.subarray(0, end), registry));
  for (const patch of [Buffer.from([1, 0, 66, 0]), Buffer.from([1, 0, 66, 23]), Buffer.from([1, 0, 99]),
    Buffer.from([2, 0, 66, 1, 66, 2]), Buffer.from([1, 1, 66, 1, 66]), Buffer.from([0, 1, 63])]) {
    assert.throws(() => decodeJukeboxContainerPacket(packet(patch), registry));
  }
  assert.throws(() => decodeContainerPacket(explicit)); // Ordinary decoder remains strict.
  const removed = decodeJukeboxContainerPacket(packet(Buffer.from([0, 1, 66])), registry);
  assert.ok(removed.kind === 'slot' && removed.stack?.removed_components?.[0] === 'minecraft:jukebox_playable');
  const slots = Array.from({ length: 63 }, (_, index) => index === 5 ? decoded.stack : null);
  const before = structuredClone(slots);
  encodeJukeboxSwap(1, 1, 5, 0, slots);
  assert.deepEqual(slots, before);
  for (const slot of [-1, 27, 1.5]) assert.throws(() => encodeJukeboxSwap(1, 1, slot, 0, slots));
  assert.deepEqual(parseCommand({ id: 1, action: 'jukebox_select', hotbar: 8 }), { id: 1, action: 'jukebox_select', hotbar: 8 });
  assert.throws(() => parseCommand({ id: 1, action: 'jukebox_select', hotbar: 9 }));
});

test('jukebox level_event uses fixed Int32 and raw song IDs, never coordinate/entity sound', () => {
  const registry = decodeSongRegistryPacket(songRegistryPacket());
  // Synthetic 1010, packed (-1,200,1), raw ID0, local broadcast.
  const bytes = Buffer.from('2f000003f2ffffffc0000010c80000000000', 'hex');
  const observation = decodeJukeboxLevelEvent(bytes, registry);
  assert.deepEqual(observation, { packet_id: 47, event: 1010, x: -1, y: 200, z: 1,
    data: 0, global_event: false, song: songReference(registry, 1), payload_bytes: 18, decoded_bytes: 18 });
  for (let end = 0; end < bytes.length; end++) assert.throws(() => decodeJukeboxLevelEvent(bytes.subarray(0, end), registry));
  for (const id of [118, 119]) {
    const sound = Buffer.from(bytes); sound[0] = id;
    assert.throws(() => decodeJukeboxLevelEvent(sound, registry));
  }
  for (const data of [-1, 22, 2147483647]) {
    const broken = Buffer.from(bytes); broken.writeInt32BE(data, 13);
    assert.throws(() => decodeJukeboxLevelEvent(broken, registry));
  }
  const boolean = Buffer.from(bytes); boolean[17] = 2;
  assert.throws(() => decodeJukeboxLevelEvent(boolean, registry));
  const stop = Buffer.from(bytes); stop.writeInt32BE(1011, 1);
  assert.equal(decodeJukeboxLevelEvent(stop, registry).song, undefined);
  assert.throws(() => decodeJukeboxLevelEvent(Buffer.concat([bytes, Buffer.from([0])]), registry));
});

test('jukebox UseItemOn encodes actual hand and bounded anonymous hit codec fields', () => {
  // Synthetic source-derived bytes, not a captured Java packet.
  const expected = Buffer.from('00ffffffc0000010c8013f0000003f8000003f00000000008001', 'hex');
  assert.deepEqual(encodeJukeboxUse(0, -1, 200, 1, 128), expected);
  const offhand = Buffer.from(expected); offhand[0] = 1;
  assert.deepEqual(encodeJukeboxUse(1, -1, 200, 1, 128), offhand);
  const command = { id: 1, action: 'jukebox_use', hand: 1, x: -1, y: 200, z: 1 };
  assert.deepEqual(parseCommand(command), command);
  for (const change of [{ hand: 2 }, { hand: '1' }, { x: 33554432 }, { x: -33554433 },
    { y: 2048 }, { y: -2049 }, { z: NaN }, { z: Infinity }, { z: 0.5 }]) {
    assert.throws(() => parseCommand({ ...command, ...change }));
  }
  for (const sequence of [-1, 1.5, Infinity, 2147483648]) {
    assert.throws(() => encodeJukeboxUse(0, 0, 0, 0, sequence));
  }
});

test('jukebox live BE requires compound and preserves leaked signed elapsed before privacy assertions', () => {
  // Synthetic BEData at (-1,200,1), raw BE type4, empty unnamed compound.
  const header = Buffer.from('06ffffffc0000010c804', 'hex');
  const empty = Buffer.concat([header, Buffer.from([10, 0])]);
  assert.deepEqual(decodeJukeboxBlockEntityPacket(empty), { packet_id: 6, x: -1, y: 200, z: 1,
    block_entity_type: 4, data: {}, payload_bytes: 12, decoded_bytes: 12 });
  for (let end = 0; end < empty.length; end++) {
    assert.throws(() => decodeJukeboxBlockEntityPacket(empty.subarray(0, end)));
  }
  for (const tag of [Buffer.from([0]), Buffer.from([3, 0, 0, 0, 0]), Buffer.from([10, 0, 0])]) {
    assert.throws(() => decodeJukeboxBlockEntityPacket(Buffer.concat([header, tag])));
  }
  const elapsed = Buffer.concat([Buffer.from([4, 0, 24]), Buffer.from('ticks_since_song_started'), Buffer.from('ffffffffffffffff', 'hex')]);
  const leaked = Buffer.concat([header, Buffer.from([10]), elapsed, Buffer.from([0])]);
  assert.deepEqual(decodeJukeboxBlockEntityPacket(leaked)?.data,
    { ticks_since_song_started: { nbt_type: 'Long', value: '-1' } });
  assert.throws(() => decodeJukeboxBlockEntityPacket(Buffer.concat([header, Buffer.from([10]), elapsed, elapsed, Buffer.from([0])])));
  assert.throws(() => decodeJukeboxBlockEntityPacket(Buffer.alloc(65537)));
  let deep = hornNbt({ hidden: 'private' });
  for (let level = 0; level < 5; level++) deep = Buffer.concat([Buffer.from([10, 10, 0, 1, 120]), deep.subarray(1), Buffer.from([0])]);
  assert.throws(() => decodeJukeboxBlockEntityPacket(Buffer.concat([header, deep])));
  const other = Buffer.from(empty); other[9] = 1;
  assert.equal(decodeJukeboxBlockEntityPacket(other), null);
});
