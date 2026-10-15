"""Golden NBT and region fixtures exercise independent storage edge cases."""
from __future__ import annotations

import gzip
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch
import zlib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
from parity import worlds


def text(value: str) -> bytes:
    raw = value.encode('utf-8')
    return struct.pack('>H', len(raw)) + raw


def named(kind: int, name: str, payload: bytes) -> bytes:
    return bytes([kind]) + text(name) + payload


def compound(*tags: bytes) -> bytes:
    return b''.join(tags) + b'\x00'


def state(name: str, properties: tuple[tuple[str, str], ...] = ()) -> bytes:
    tags = [named(8, 'Name', text(name))]
    if properties:
        tags.append(named(10, 'Properties', compound(*(named(8, key, text(value)) for key, value in properties))))
    return compound(*tags)


def section(y: int, palette: tuple[bytes, ...], words: tuple[int, ...] | None = None) -> bytes:
    tags = [named(9, 'palette', b'\x0a' + struct.pack('>i', len(palette)) + b''.join(palette))]
    if words is not None:
        tags.append(named(12, 'data', struct.pack('>i', len(words)) + b''.join(struct.pack('>q', word) for word in words)))
    return compound(named(1, 'Y', struct.pack('>b', y)), named(10, 'block_states', compound(*tags)))


def chunk(sections: tuple[bytes, ...], *, x: int = 0, z: int = 0, status: str = 'minecraft:full') -> bytes:
    return named(10, 'Chunk', compound(
        named(3, 'DataVersion', struct.pack('>i', 5023)),
        named(3, 'xPos', struct.pack('>i', x)), named(3, 'zPos', struct.pack('>i', z)),
        named(8, 'Status', text(status)),
        named(9, 'sections', b'\x0a' + struct.pack('>i', len(sections)) + b''.join(sections)),
    ))


def write_region(world: Path, payload: bytes, *, compression: int = 2, x: int = 0, z: int = 0) -> Path:
    region = world / 'region' / f'r.{x // 32}.{z // 32}.mca'
    region.parent.mkdir(parents=True, exist_ok=True)
    if compression == 2:
        payload = zlib.compress(payload)
    elif compression == 1:
        payload = gzip.compress(payload)
    record = struct.pack('>I', len(payload) + 1) + bytes([compression]) + payload
    sectors = (len(record) + 4095) // 4096
    header = bytearray(8192)
    index = ((x % 32) + (z % 32) * 32) * 4
    header[index:index + 4] = ((2 << 8) | sectors).to_bytes(4, 'big')
    region.write_bytes(header + record + bytes(sectors * 4096 - len(record)))
    return region


class NBTTests(unittest.TestCase):
    def test_named_root_scalars_lists_compounds_and_arrays(self):
        fixture = named(10, 'Root', compound(
            named(1, 'byte', b'\xff'), named(2, 'short', b'\x80\x00'),
            named(3, 'int', b'\xff\xff\xff\xfe'),
            named(4, 'long', b'\x80\x00\x00\x00\x00\x00\x00\x00'),
            named(5, 'float', bytes.fromhex('3fc00000')),
            named(6, 'double', bytes.fromhex('c004000000000000')),
            named(7, 'bytes', b'\x00\x00\x00\x02\x00\xff'),
            named(8, 'string', b'\x00\x03a\xc0\x80'),
            named(9, 'list', b'\x03\x00\x00\x00\x02\x00\x00\x00\x01\xff\xff\xff\xfe'),
            named(10, 'nested', compound(named(8, 'key', text('value')))),
            named(11, 'ints', b'\x00\x00\x00\x01\xff\xff\xff\xff'),
            named(12, 'longs', b'\x00\x00\x00\x01\xff\xff\xff\xff\xff\xff\xff\xff'),
        ))
        actual = worlds.decode_nbt(fixture)
        self.assertEqual(actual, {'byte': -1, 'short': -32768, 'int': -2, 'long': -(1 << 63),
                                  'float': 1.5, 'double': -2.5, 'bytes': b'\x00\xff', 'string': 'a\x00',
                                  'list': [1, -2], 'nested': {'key': 'value'}, 'ints': (-1,), 'longs': (-1,)})

    def test_malformed_lengths_trailing_data_and_depth_fail(self):
        for fixture in (
            b'\x0a',
            named(10, '', compound(named(9, 'bad', b'\x03\xff\xff\xff\xff'))),
            named(10, '', compound(named(9, 'bad', b'\x00\x00\x00\x00\x01'))),
            named(10, '', compound(named(8, 'bad', b'\x00\x10x'))),
            named(10, '', compound()) + b'extra',
            named(10, '', compound(named(1, 'same', b'\x01'), named(1, 'same', b'\x02'))),
            b'\x0a\x00\x00' + b'\x0a\x00\x01x' * 66 + b'\x00' * 67,
        ):
            with self.subTest(fixture=fixture[:20]), self.assertRaises(ValueError):
                worlds.decode_nbt(fixture)

    def test_decompression_formats_and_expansion_limit(self):
        payload = named(10, '', compound())
        for compression, encoded in ((1, gzip.compress(payload)), (2, zlib.compress(payload)), (3, payload)):
            self.assertEqual(worlds.decompress_nbt(encoded, compression), payload)
        for compression in (0, 4, 130):
            with self.assertRaises(ValueError):
                worlds.decompress_nbt(payload, compression)
        with patch.object(worlds, 'MAX_NBT_BYTES', 64), self.assertRaises(ValueError):
            worlds.decompress_nbt(zlib.compress(b'x' * 65), 2)
        for payload in (zlib.compress(b'x')[:-1], zlib.compress(b'x') + b'trailing'):
            with self.assertRaises(ValueError):
                worlds.decompress_nbt(payload, 2)

    def test_five_bit_non_straddling_palette_words_and_signed_longs(self):
        palette = tuple(state(f'test:block_{index}') for index in range(17))
        # Golden word contains palette indices 0..11, then four unused high bits
        # all set. The next state starts a fresh word rather than straddling it.
        words = (-745020035755309024, 16, *([0] * 340))
        decoded = worlds.decode_nbt(chunk((section(-4, palette, words),)))
        states = worlds.decode_section(decoded['sections'][0])
        self.assertEqual([name for name, _ in states[:14]], [*(f'test:block_{index}' for index in range(12)), 'test:block_16', 'test:block_0'])
        self.assertEqual(len(states), 4096)
        decoded['sections'][0]['block_states']['data'] = (31, *([0] * 341))
        with self.assertRaisesRegex(ValueError, 'index is out of bounds'):
            worlds.decode_section(decoded['sections'][0])
        decoded['sections'][0]['block_states']['data'] = words[:-1]
        with self.assertRaisesRegex(ValueError, '342 words'):
            worlds.decode_section(decoded['sections'][0])

    def test_target_codec_string_wrappers_and_lowercase_state_fields(self):
        # NBT cannot directly mix string and compound elements, so 26.3 wraps
        # shorthand strings under an empty key in a heterogeneous codec list.
        shorthand = compound(named(8, '', text('minecraft:stone')))
        full = compound(named(8, 'id', text('minecraft:oak_log')),
                        named(10, 'properties', compound(named(8, 'axis', text('y')))))
        decoded = worlds.decode_nbt(chunk((section(-4, (shorthand, full), (1, *([0] * 255))),)))
        states = worlds.decode_section(decoded['sections'][0])
        self.assertEqual(states[0], ('minecraft:oak_log', (('axis', 'y'),)))
        self.assertEqual(states[1], ('minecraft:stone', ()))
        string_palette = compound(named(1, 'Y', b'\xfc'), named(10, 'block_states', compound(
            named(9, 'palette', b'\x08\x00\x00\x00\x01' + text('minecraft:air')))))
        decoded = worlds.decode_nbt(chunk((string_palette,)))
        self.assertEqual(set(worlds.decode_section(decoded['sections'][0])), {worlds.AIR})


class RegionComparisonTests(unittest.TestCase):
    def test_modern_dimension_directory_and_ambiguous_layout(self):
        with tempfile.TemporaryDirectory() as directory:
            world = Path(directory)
            dimension = world / 'dimensions/minecraft/overworld'
            write_region(dimension, chunk(()))
            self.assertEqual(worlds.read_chunk(world)['DataVersion'], 5023)
            write_region(world, chunk(()))
            with self.assertRaisesRegex(ValueError, 'exactly one'):
                worlds.read_chunk(world)

    def test_hashes_ignore_property_order_and_surface_fixture_edits(self):
        with tempfile.TemporaryDirectory() as directory:
            vanilla, rust = Path(directory) / 'vanilla', Path(directory) / 'rust'
            first = state('minecraft:oak_log', (('axis', 'y'), ('test', 'value')))
            second = state('minecraft:oak_log', (('test', 'value'), ('axis', 'y')))
            write_region(vanilla, chunk(tuple(section(y, (first,)) for y in range(-4, 0)) + (section(6, (state('minecraft:stone'),)),)))
            write_region(rust, chunk(tuple(section(y, (second,)) for y in range(-4, 0)) + (section(6, (state('minecraft:glass'),)),)), compression=1)
            result = worlds.compare_terrain(vanilla, rust)
            self.assertEqual(result['result'], 'passed')
            self.assertEqual(result['blocks_compared'], 16384)
            self.assertEqual(result['mismatched_blocks'], 0)
            self.assertEqual(result['vanilla_sha256'], result['rust_sha256'])
            self.assertEqual((result['y_min'], result['y_max']), (-64, -1))

    def test_one_mismatching_block_reports_exact_negative_world_coordinate(self):
        with tempfile.TemporaryDirectory() as directory:
            vanilla, rust = Path(directory) / 'vanilla', Path(directory) / 'rust'
            stone, granite = state('minecraft:stone'), state('minecraft:granite')
            write_region(vanilla, chunk((section(-4, (stone,)),), x=-1, z=-1), x=-1, z=-1)
            write_region(rust, chunk((section(-4, (stone, granite), (1, *([0] * 255))),), x=-1, z=-1), x=-1, z=-1)
            result = worlds.compare_terrain(vanilla, rust, chunk_x=-1, chunk_z=-1)
            self.assertEqual(result['result'], 'failed')
            self.assertEqual(result['mismatched_blocks'], 1)
            self.assertNotEqual(result['vanilla_sha256'], result['rust_sha256'])
            sample = result['mismatch_sample'][0]
            self.assertEqual((sample['x'], sample['y'], sample['z']), (-16, -64, -16))
            self.assertEqual(sample['rust']['name'], 'minecraft:granite')

    def test_valid_absent_sections_are_air_but_present_invalid_sections_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            vanilla, rust = Path(directory) / 'vanilla', Path(directory) / 'rust'
            write_region(vanilla, chunk(()))
            write_region(rust, chunk(tuple(section(y, (state('minecraft:air'),)) for y in range(-4, 0))))
            self.assertEqual(worlds.compare_terrain(vanilla, rust)['result'], 'passed')
            malformed = compound(named(1, 'Y', b'\xfc'))
            write_region(rust, chunk((malformed,)))
            with self.assertRaisesRegex(ValueError, 'missing block_states'):
                worlds.compare_terrain(vanilla, rust)
            with self.assertRaises(ValueError):
                worlds.compare_terrain(vanilla, rust, section_ys=(6,))

    def test_region_absence_allocation_and_record_bounds_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            world = Path(directory)
            path = write_region(world, chunk(()))
            with self.assertRaisesRegex(ValueError, 'absent'):
                worlds.read_chunk(world, 1, 0)
            valid = path.read_bytes()
            for damaged in (
                valid[:100],
                b'\x00\x00\x01\x01' + valid[4:],
                valid[:8192] + b'\x00\x00\x10\x00' + valid[8196:],
                valid[:9000],
            ):
                path.write_bytes(damaged)
                with self.assertRaises(ValueError):
                    worlds.read_chunk(world)
            write_region(world, chunk((), x=1))
            with self.assertRaisesRegex(ValueError, 'coordinates'):
                worlds.read_chunk(world)
            write_region(world, chunk((), status='minecraft:noise'))
            with self.assertRaisesRegex(ValueError, 'fully generated'):
                worlds.read_chunk(world)
            write_region(world, b'not-inline', compression=130)
            with self.assertRaisesRegex(ValueError, 'External'):
                worlds.read_chunk(world)


if __name__ == '__main__':
    unittest.main()
