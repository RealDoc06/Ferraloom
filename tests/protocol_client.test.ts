import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { deflateSync } from 'node:zlib';
import { COMMAND_ARGUMENT_TYPES, PROTOCOL, VERSION, decodeCommandTree, decodeCompressedPacket, parseCommand, readVarInt, varInt } from '../tools/parity/client.ts';

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
