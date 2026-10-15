"""Read saved Anvil terrain without loading a server or mutating a world.

The comparison covers block states in one underground chunk. Callers must create
both fixtures with the same seed; this does not establish complete worldgen parity.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
import re
import struct
import zlib

MAX_NBT_BYTES = 16 * 1024 * 1024
MAX_NODES = 1_000_000
MAX_DEPTH = 64
SECTOR_BYTES = 4096
State = tuple[str, tuple[tuple[str, str], ...]]
AIR: State = ('minecraft:air', ())
IDENTIFIER = re.compile(r'[a-z0-9_.-]+:[a-z0-9_./-]+')
ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class BlockDefinition:
    defaults: dict[str, str]
    values: dict[str, frozenset[str]]
    states: frozenset[tuple[tuple[str, str], ...]]


def default_blocks_report() -> Path:
    target = json.loads((ROOT / 'compatibility/target.json').read_text())
    return ROOT / '.cache/vanilla' / target['version'] / 'generated/reports/blocks.json'


def load_block_catalog(path: Path) -> tuple[dict[str, BlockDefinition], str]:
    """Index exact official defaults and legal states rather than guessing omissions."""
    try:
        raw = path.read_bytes()
        report = json.loads(raw)
    except (OSError, ValueError) as error:
        raise ValueError(f'Cannot read official block report {path}: {error}') from error
    if not isinstance(report, dict) or not report:
        raise ValueError('Official block report must be a nonempty object')
    catalog = {}
    for name, entry in report.items():
        if not IDENTIFIER.fullmatch(name) or not isinstance(entry, dict):
            raise ValueError('Invalid official block definition')
        values = entry.get('properties', {})
        if not isinstance(values, dict) or any(
            not isinstance(key, str) or not isinstance(options, list) or not options or
            any(not isinstance(option, str) for option in options) for key, options in values.items()
        ):
            raise ValueError(f'Invalid property domains for {name}')
        states = entry.get('states')
        if not isinstance(states, list) or not states:
            raise ValueError(f'Missing official states for {name}')
        canonical = set()
        defaults = []
        for state in states:
            if not isinstance(state, dict):
                raise ValueError(f'Invalid official state for {name}')
            properties = state.get('properties', {})
            if not isinstance(properties, dict) or properties.keys() != values.keys() or any(
                not isinstance(value, str) or value not in values[key] for key, value in properties.items()
            ):
                raise ValueError(f'Invalid official state properties for {name}')
            canonical.add(tuple(sorted(properties.items())))
            if state.get('default') is True:
                defaults.append(properties)
        if len(defaults) != 1:
            raise ValueError(f'Expected exactly one official default state for {name}')
        catalog[name] = BlockDefinition(dict(defaults[0]), {key: frozenset(options) for key, options in values.items()}, frozenset(canonical))
    return catalog, hashlib.sha256(raw).hexdigest()


def normalize_states(states: list[State], catalog: dict[str, BlockDefinition]) -> list[State]:
    """Resolve omitted properties; reject unknown blocks and unsupported explicit states."""
    normalized = {}
    for state in set(states):
        name, explicit = state
        definition = catalog.get(name)
        if definition is None:
            raise ValueError(f'Unknown block in terrain sample: {name}')
        properties = definition.defaults.copy()
        for key, value in explicit:
            if key not in definition.values or value not in definition.values[key]:
                raise ValueError(f'Unknown block property or value: {name}[{key}={value}]')
            properties[key] = value
        canonical = tuple(sorted(properties.items()))
        if canonical not in definition.states:
            raise ValueError(f'Unsupported block state combination: {name}{properties}')
        normalized[state] = name, canonical
    return [normalized[state] for state in states]


class NBTReader:
    """Bounded named-root NBT reader; long arrays preserve signed 64-bit values."""

    def __init__(self, data: bytes):
        if len(data) > MAX_NBT_BYTES:
            raise ValueError('NBT exceeds byte limit')
        self.data = memoryview(data)
        self.offset = 0
        self.nodes = 0

    def take(self, count: int) -> memoryview:
        if count < 0 or count > len(self.data) - self.offset:
            raise ValueError('Truncated NBT payload')
        result = self.data[self.offset:self.offset + count]
        self.offset += count
        return result

    def number(self, code: str) -> int | float:
        return struct.unpack('>' + code, self.take(struct.calcsize(code)))[0]

    def string(self) -> str:
        size = self.number('H')
        raw = bytes(self.take(size))
        try:
            # Java's modified UTF-8 uses a two-byte NUL and UTF-16 surrogates.
            decoded = raw.replace(b'\xc0\x80', b'\x00').decode('utf-8', 'surrogatepass')
            return decoded.encode('utf-16-le', 'surrogatepass').decode('utf-16-le', 'surrogatepass')
        except UnicodeError as error:
            raise ValueError('Invalid NBT string encoding') from error

    def count(self) -> int:
        count = self.number('i')
        if count < 0 or count > MAX_NODES:
            raise ValueError('NBT collection length exceeds bounds')
        return count

    def value(self, tag: int, depth: int = 0) -> object:
        self.nodes += 1
        if depth > MAX_DEPTH or self.nodes > MAX_NODES:
            raise ValueError('NBT nesting or node limit exceeded')
        if tag in (1, 2, 3, 4, 5, 6):
            return self.number({1: 'b', 2: 'h', 3: 'i', 4: 'q', 5: 'f', 6: 'd'}[tag])
        if tag == 7:
            return bytes(self.take(self.count()))
        if tag == 8:
            return self.string()
        if tag == 9:
            element = self.number('B')
            count = self.count()
            if element not in range(13) or (element == 0 and count):
                raise ValueError('Invalid NBT list element type')
            if count > MAX_NODES - self.nodes:
                raise ValueError('NBT list exceeds node budget')
            return [self.value(element, depth + 1) for _ in range(count)]
        if tag == 10:
            result = {}
            while True:
                element = self.number('B')
                if element == 0:
                    return result
                name = self.string()
                if name in result:
                    raise ValueError('Duplicate NBT compound key')
                result[name] = self.value(element, depth + 1)
        if tag in (11, 12):
            count = self.count()
            if count > MAX_NODES - self.nodes:
                raise ValueError('NBT array exceeds node budget')
            self.nodes += count
            code = 'i' if tag == 11 else 'q'
            return tuple(value[0] for value in struct.iter_unpack('>' + code, self.take(count * struct.calcsize(code))))
        raise ValueError(f'Unsupported NBT tag type {tag}')


def decode_nbt(data: bytes) -> dict:
    """Decode exactly one named compound root; reject trailing or truncated data."""
    reader = NBTReader(data)
    if reader.number('B') != 10:
        raise ValueError('NBT root must be a named compound')
    reader.string()
    result = reader.value(10)
    if reader.offset != len(reader.data):
        raise ValueError('Trailing bytes after NBT root')
    return result


def decompress_nbt(payload: bytes, compression: int) -> bytes:
    if compression & 0x80:
        raise ValueError('External Anvil chunk files are unsupported')
    if compression == 3:
        if len(payload) > MAX_NBT_BYTES:
            raise ValueError('Raw chunk exceeds NBT byte limit')
        return payload
    if compression not in (1, 2):
        raise ValueError(f'Unsupported Anvil compression type {compression}')
    decoder = zlib.decompressobj(16 + zlib.MAX_WBITS if compression == 1 else zlib.MAX_WBITS)
    try:
        data = decoder.decompress(payload, MAX_NBT_BYTES + 1)
        if len(data) > MAX_NBT_BYTES or decoder.unconsumed_tail:
            raise ValueError('Decompressed chunk exceeds NBT byte limit')
        if not decoder.eof or decoder.unused_data:
            raise ValueError('Truncated or trailing compressed chunk data')
        return data
    except zlib.error as error:
        raise ValueError('Invalid compressed chunk data') from error


def read_chunk(world: Path, chunk_x: int = 0, chunk_z: int = 0) -> dict:
    """Read one occupied region entry, honoring its sector allocation and length."""
    directories = [directory for directory in (
        Path(world) / 'region',
        Path(world) / 'dimensions/minecraft/overworld/region',
    ) if directory.is_dir()]
    if len(directories) != 1:
        raise ValueError(f'World must have exactly one recognizable overworld region directory: {world}')
    region = directories[0] / f'r.{chunk_x // 32}.{chunk_z // 32}.mca'
    try:
        with region.open('rb') as handle:
            header = handle.read(2 * SECTOR_BYTES)
            if len(header) != 2 * SECTOR_BYTES:
                raise ValueError('Truncated Anvil region header')
            index = ((chunk_x % 32) + (chunk_z % 32) * 32) * 4
            location = int.from_bytes(header[index:index + 4], 'big')
            sector, count = location >> 8, location & 255
            if sector == 0 and count == 0:
                raise ValueError(f'Chunk ({chunk_x}, {chunk_z}) is absent')
            if sector < 2 or count == 0:
                raise ValueError('Invalid Anvil chunk sector allocation')
            if (sector + count) * SECTOR_BYTES > region.stat().st_size:
                raise ValueError('Chunk allocation extends beyond region file')
            handle.seek(sector * SECTOR_BYTES)
            raw_length = handle.read(4)
            if len(raw_length) != 4:
                raise ValueError('Truncated Anvil chunk length')
            length = int.from_bytes(raw_length, 'big')
            if not 2 <= length <= count * SECTOR_BYTES - 4:
                raise ValueError('Chunk length exceeds its allocated sectors')
            record = handle.read(length)
            if len(record) != length:
                raise ValueError('Truncated Anvil chunk record')
    except OSError as error:
        raise ValueError(f'Cannot read region {region}: {error}') from error
    root = decode_nbt(decompress_nbt(record[1:], record[0]))
    if root.get('xPos') != chunk_x or root.get('zPos') != chunk_z:
        raise ValueError('Stored chunk coordinates do not match region entry')
    if root.get('Status') not in ('full', 'minecraft:full'):
        raise ValueError('Terrain comparison requires a fully generated chunk')
    if type(root.get('DataVersion')) is not int or root['DataVersion'] < 0:
        raise ValueError('Chunk must declare a valid DataVersion')
    if not isinstance(root.get('sections'), list):
        raise ValueError('Modern chunk sections list is missing')
    return root


def _palette_state(entry: object) -> State:
    # 26.3 codecs encode default states as strings, including defaults with
    # properties. Mixed NBT lists wrap
    # those strings in {"": value}; property states use lowercase id/properties.
    # Older Anvil writers use Name/Properties, which has the same state meaning.
    if isinstance(entry, str):
        name, properties = entry, {}
    elif isinstance(entry, dict) and set(entry) == {''}:
        name, properties = entry[''], {}
    elif isinstance(entry, dict) and 'id' in entry and set(entry) <= {'id', 'properties'}:
        name, properties = entry['id'], entry.get('properties', {})
    elif isinstance(entry, dict) and 'Name' in entry and set(entry) <= {'Name', 'Properties'}:
        name, properties = entry['Name'], entry.get('Properties', {})
    else:
        raise ValueError('Unrecognized block palette state encoding')
    if not isinstance(name, str) or not IDENTIFIER.fullmatch(name):
        raise ValueError('Block palette entry needs a namespaced Name')
    if not isinstance(properties, dict) or any(not isinstance(key, str) or not isinstance(value, str) for key, value in properties.items()):
        raise ValueError('Block palette properties must be strings')
    return name, tuple(sorted(properties.items()))


def decode_section(section: dict) -> list[State]:
    """Decode modern non-straddling packed states, ordered y then z then x."""
    states = section.get('block_states')
    if not isinstance(states, dict):
        raise ValueError('Present terrain section is missing block_states')
    palette = states.get('palette')
    if not isinstance(palette, list) or not 1 <= len(palette) <= 4096:
        raise ValueError('Block palette must contain 1 to 4096 entries')
    palette = [_palette_state(entry) for entry in palette]
    if len(set(palette)) != len(palette):
        raise ValueError('Duplicate block palette entry')
    words = states.get('data')
    if len(palette) == 1:
        if words not in (None, ()):
            raise ValueError('Single-entry palette must omit packed data or use an empty long array')
        return [palette[0]] * 4096
    bits = max(4, (len(palette) - 1).bit_length())
    per_word = 64 // bits
    expected = (4096 + per_word - 1) // per_word
    if not isinstance(words, tuple) or len(words) != expected or any(type(word) is not int for word in words):
        raise ValueError(f'Packed state long array needs {expected} words')
    mask = (1 << bits) - 1
    result = []
    for index in range(4096):
        word = words[index // per_word] & 0xFFFFFFFFFFFFFFFF
        palette_index = (word >> ((index % per_word) * bits)) & mask
        if palette_index >= len(palette):
            raise ValueError('Packed block palette index is out of bounds')
        result.append(palette[palette_index])
    return result


def sample_states(chunk: dict, section_ys: tuple[int, ...]) -> list[State]:
    sections = {}
    for section in chunk['sections']:
        if not isinstance(section, dict) or type(section.get('Y')) is not int or not -128 <= section['Y'] <= 127:
            raise ValueError('Section must have a signed byte Y')
        if section['Y'] in sections:
            raise ValueError('Duplicate section Y')
        sections[section['Y']] = section
    result = []
    for section_y in section_ys:
        # A missing section in a valid full chunk is all air. A present but
        # malformed section is an error rather than an invitation to invent air.
        result.extend(decode_section(sections[section_y]) if section_y in sections else [AIR] * 4096)
    return result


def _canonical_hash(states: list[State]) -> str:
    digest = hashlib.sha256()
    for name, properties in states:
        digest.update(json.dumps([name, properties], separators=(',', ':'), ensure_ascii=True).encode('ascii'))
        digest.update(b'\n')
    return digest.hexdigest()


def compare_terrain(vanilla_world: Path, rust_world: Path, *, chunk_x: int = 0, chunk_z: int = 0,
                    section_ys: tuple[int, ...] = (-4, -3, -2, -1), sample_limit: int = 12,
                    blocks_report: Path | None = None) -> dict:
    """Compare a fixed underground sample; malformed or absent data raises ValueError."""
    if not section_ys or any(type(y) is not int or not -4 <= y <= -1 for y in section_ys) or len(set(section_ys)) != len(section_ys):
        raise ValueError('Select unique underground sections Y=-4..-1 only')
    if type(sample_limit) is not int or not 0 <= sample_limit <= 100:
        raise ValueError('Mismatch sample limit must be between 0 and 100')
    section_ys = tuple(sorted(section_ys))
    catalog, report_sha256 = load_block_catalog(blocks_report or default_blocks_report())
    chunks = {name: read_chunk(world, chunk_x, chunk_z) for name, world in (('vanilla', vanilla_world), ('rust', rust_world))}
    states = {name: normalize_states(sample_states(chunk, section_ys), catalog) for name, chunk in chunks.items()}
    mismatches = 0
    examples = []
    for index, (vanilla, rust) in enumerate(zip(states['vanilla'], states['rust'], strict=True)):
        if vanilla == rust:
            continue
        mismatches += 1
        if len(examples) < sample_limit:
            local = index % 4096
            examples.append({'x': chunk_x * 16 + (local & 15),
                             'y': section_ys[index // 4096] * 16 + (local >> 8),
                             'z': chunk_z * 16 + ((local >> 4) & 15),
                             'vanilla': {'name': vanilla[0], 'properties': dict(vanilla[1])},
                             'rust': {'name': rust[0], 'properties': dict(rust[1])}})
    return {
        'result': 'passed' if mismatches == 0 else 'failed',
        'scope': 'Block states only in one same-seed underground chunk sample; seed is a fixture precondition. No claim of complete terrain, biome, structure or world-generation parity.',
        'chunk': {'x': chunk_x, 'z': chunk_z}, 'section_ys': list(section_ys),
        'y_min': min(section_ys) * 16, 'y_max': max(section_ys) * 16 + 15,
        'blocks_compared': len(states['vanilla']), 'matching_blocks': len(states['vanilla']) - mismatches,
        'mismatched_blocks': mismatches, 'mismatch_sample': examples,
        'vanilla_sha256': _canonical_hash(states['vanilla']), 'rust_sha256': _canonical_hash(states['rust']),
        'blocks_report_sha256': report_sha256,
        'data_versions': {name: chunk['DataVersion'] for name, chunk in chunks.items()},
    }
