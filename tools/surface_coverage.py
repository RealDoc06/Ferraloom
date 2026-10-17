#!/usr/bin/env python3
"""Finite official-surface accounting. No engine or paired-coverage claims.

Only standard-library JSON/ZIP reads are used. The authored map pins groups;
the index stores their digests, and --expand exposes lossless source pointers.
"""

from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from dataclasses import dataclass, field
import hashlib
import json
from pathlib import Path
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FACETS = {
    'registry_entry': ('identity', 'codec_and_consumers'),
    'block_type': ('identity', 'state_and_data_relations'),
    'item_type': ('identity', 'defaults_and_stacking', 'gameplay_consumers'),
    'entity_type': ('factory', 'tracking', 'persistence_lifecycle', 'movement_combat_interaction', 'spawning_ai'),
    'block_entity_type': ('association_construction', 'persistence_live_edit', 'initial_live_privacy', 'menu_consumers'),
    'component_type': ('primary_schema', 'saved_decode_encode', 'wire_decode_encode_hash', 'default_patch_removal', 'gameplay_consumers'),
    'block_definition': ('identity_properties', 'placement_neighbor_tick_use', 'collision_shape', 'drops_data_relations'),
    'block_state': ('identity_tuple', 'state_behavior'),
    'command': ('grammar_redirect', 'parser_properties', 'permissions', 'advertisement_profile', 'execution_context_result_error'),
    'datapack_descriptor': ('descriptor_schema', 'references_tags_precedence', 'consumer_reload'),
    'item_definition': ('reported_defaults', 'effective_patch_removal'),
    'item_default': ('reported_value', 'saved_codec', 'wire_hash_codec', 'effective_patch_removal', 'gameplay_consumers'),
    'resource': ('decode_schema', 'references_tags_precedence', 'consumer_behavior', 'reload_profile_persistence'),
    'packet': ('field_codec', 'dispatch_state_legality', 'effect'),
    'rpc_method': ('schema', 'auth_enablement', 'execution'),
    'rpc_schema': ('schema', 'codec_consumers'),
}
REASONS = frozenset({
    'unassessed_behavior', 'missing_route', 'unsupported_form', 'safe_refusal',
    'missing_primary_schema', 'missing_reference_scenario', 'profile_not_observed',
    'nonserver_metadata', 'source_inapplicable',
})
SPECIAL_REGISTRIES = {
    'minecraft:block': 'block_type', 'minecraft:item': 'item_type',
    'minecraft:entity_type': 'entity_type',
    'minecraft:block_entity_type': 'block_entity_type',
    'minecraft:data_component_type': 'component_type',
}
TARGET_FIELDS = {'edition', 'version', 'protocol', 'java_major', 'metadata_sha1', 'server_sha1', 'server_size'}


class Invalid(ValueError):
    """An input or authored accounting contract is invalid."""


def require(condition: object, message: str) -> None:
    if not condition:
        raise Invalid(message)


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(',', ':'),
                      ensure_ascii=False, allow_nan=False).encode('utf-8')


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def fingerprint(value: object) -> str:
    return digest(canonical(value))


def read_json(raw: bytes) -> object:
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, f'duplicate JSON key: {key}')
            result[key] = value
        return result

    def nonfinite(value):
        raise Invalid(f'nonfinite JSON number: {value}')

    value = json.loads(raw, object_pairs_hook=pairs, parse_constant=nonfinite)
    canonical(value)  # Also rejects exponent overflow (for example 1e999).
    return value


def fields(value: dict, allowed: set[str], where: str) -> None:
    require(isinstance(value, dict), f'{where}: expected object')
    require(not value.keys() - allowed, f'{where}: unknown fields {sorted(value.keys() - allowed)}')


def pointer(*parts: object) -> str:
    return ''.join('/' + str(p).replace('~', '~0').replace('/', '~1') for p in parts)


def at_pointer(value: object, ref: str) -> object:
    require(ref == '' or ref.startswith('/'), f'invalid JSON pointer: {ref}')
    for part in ref.split('/')[1:]:
        part = part.replace('~1', '/').replace('~0', '~')
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def relative(root: Path, name: str) -> Path:
    path = root / name
    require(not Path(name).is_absolute() and '..' not in Path(name).parts,
            f'input must be root-relative: {name}')
    require(path.resolve().is_relative_to(root.resolve()), f'input escapes root: {name}')
    return path


def verified(root: Path, pin: dict) -> bytes:
    raw = relative(root, pin['path']).read_bytes()
    require(len(raw) == pin['bytes'] and digest(raw) == pin['sha256'],
            f'source bytes changed: {pin["path"]}')
    return raw


@dataclass
class Surface:
    key: list
    domain: str
    group: list
    source: dict
    value: object
    context: dict = field(default_factory=dict)

    def content_sha256(self) -> str:
        return fingerprint({'value': self.value, 'context': self.context})


@dataclass
class Inventory:
    surfaces: dict[str, Surface] = field(default_factory=dict)
    diagnostics: list[dict] = field(default_factory=list)
    metadata: dict = field(default_factory=dict)
    counts: dict = field(default_factory=dict)

    def add(self, surface: Surface) -> None:
        key = canonical(surface.key).decode()
        require(surface.domain in FACETS, f'unknown domain: {surface.domain}')
        require(key not in self.surfaces, f'duplicate surface: {key}')
        self.surfaces[key] = surface

    def unknown(self, value: dict, known: set[str], source: dict) -> None:
        require(isinstance(value, dict), f'expected source object: {source}')
        for name in sorted(value.keys() - known):
            self.diagnostics.append({'kind': 'unknown_field', 'source': source, 'field': name})

    def groups(self) -> dict[str, list[Surface]]:
        groups = defaultdict(list)
        for surface in self.surfaces.values():
            groups[canonical(surface.group).decode()].append(surface)
        return dict(sorted(groups.items()))


def member_pin(surfaces: list[Surface]) -> dict:
    keys = sorted((s.key for s in surfaces), key=canonical)
    return {'count': len(keys), 'sha256': fingerprint(keys)}


def content_pin(surfaces: list[Surface]) -> str:
    return fingerprint(sorted(([s.key, s.content_sha256()] for s in surfaces), key=lambda row: canonical(row[0])))


def extract(reports: dict, items: dict[str, bytes], resources: dict[str, bytes]) -> Inventory:
    """Read finite reports, retaining original values and source-derived relations.

    Redirects are edges, never traversal instructions. Resource kinds use the
    longest *reported descriptor* prefix so tags/worldgen retain nested kinds.
    """
    inv = Inventory()
    registries = reports['registries']
    blocks = reports['blocks']
    commands = reports['commands']
    datapack = reports['datapack']
    rpc = reports['json-rpc-api-schema']
    source = lambda report, *parts: {'report': report, 'pointer': pointer(*parts)}
    registry_metadata = {}
    for registry, record in sorted(registries.items()):
        inv.unknown(record, {'entries', 'protocol_id', 'default'}, source('registries', registry))
        registry_metadata[registry] = {k: v for k, v in record.items() if k != 'entries'}
        domain = SPECIAL_REGISTRIES.get(registry, 'registry_entry')
        for name, entry in sorted(record['entries'].items()):
            ref = source('registries', registry, 'entries', name)
            inv.unknown(entry, {'protocol_id'}, ref)
            inv.add(Surface(['registry', registry, name], domain, [domain, registry], ref,
                            entry, {'registry_metadata': registry_metadata[registry]}))

    state_ids = set()
    for block, record in sorted(blocks.items()):
        ref = source('blocks', block)
        inv.unknown(record, {'properties', 'states'}, ref)
        inv.add(Surface(['block', block], 'block_definition', ['block_definition'], ref, record))
        for index, state in enumerate(record['states']):
            require(type(state['id']) is int and state['id'] not in state_ids, 'duplicate/invalid state ID')
            state_ids.add(state['id'])
            ref = source('blocks', block, 'states', index)
            inv.unknown(state, {'id', 'properties', 'default'}, ref)
            inv.add(Surface(['block_state', block, state['id']], 'block_state', ['block_state'], ref,
                            state, {'block': ['block', block], 'property_domains': record.get('properties', {})}))

    command_counts = Counter()

    def command(node, path, kinds, chain, parts):
        ref = source('commands', *parts)
        inv.unknown(node, {'type', 'children', 'parser', 'properties', 'executable', 'redirect', 'permissions'}, ref)
        if 'properties' in node:
            inv.unknown(node['properties'], {'type', 'amount', 'registry', 'min', 'max'},
                        source('commands', *parts, 'properties'))
        kinds = kinds + [node['type']]
        if 'permissions' in node:
            chain = chain + [{'path': path, 'permission': node['permissions']}]
        context = {'path_kinds': kinds, 'permission_chain': chain,
                   'runtime_predicates': 'unknown; generated report is not session advertisement'}
        inv.add(Surface(['command', path], 'command', ['command', path[:1]], ref, node, context))
        command_counts.update({'nodes': 1, 'executable': int(node.get('executable', False)),
                               'argument': int(node['type'] == 'argument'),
                               'redirect': int('redirect' in node), 'permission': int('permissions' in node)})
        for name, child in sorted(node.get('children', {}).items()):
            command(child, path + [name], kinds, chain, parts + ['children', name])

    command(commands, [], [], [], [])
    for surface in inv.surfaces.values():
        if surface.domain == 'command' and 'redirect' in surface.value:
            require(canonical(['command', surface.value['redirect']]).decode() in inv.surfaces,
                    f'dangling command redirect: {surface.key}')

    inv.unknown(datapack, {'registries', 'others'}, source('datapack'))
    kinds = set(datapack['others']) | {name.split(':', 1)[1] for name in datapack['registries']}
    for category, records in sorted(datapack.items()):
        require(category in {'registries', 'others'}, f'unknown datapack section: {category}')
        for name, record in sorted(records.items()):
            ref = source('datapack', category, name)
            inv.unknown(record, {'elements', 'tags', 'stable', 'format'}, ref)
            inv.add(Surface(['datapack_descriptor', category, name], 'datapack_descriptor',
                            ['datapack_descriptor', category], ref, record,
                            {'static_registry': name if name in registries else None}))

    occurrences = Counter()
    for name, raw in sorted(items.items()):
        record = read_json(raw)
        ref = {'item_report': name, 'pointer': ''}
        inv.unknown(record, {'components'}, ref)
        inv.add(Surface(['item_definition', name], 'item_definition', ['item_definition'], ref,
                        record, {'raw_sha256': digest(raw)}))
        for component, value in sorted(record['components'].items()):
            occurrences[component] += 1
            inv.add(Surface(['item_default', name, component], 'item_default', ['item_default', component],
                            {'item_report': name, 'pointer': pointer('components', component)}, value))
    require(set(items) == set(registries['minecraft:item']['entries']), 'item report membership differs from item registry')
    component_types = set(registries['minecraft:data_component_type']['entries'])
    require(not occurrences.keys() - component_types, 'unregistered default component type')
    for name in sorted(component_types):
        key = canonical(['registry', 'minecraft:data_component_type', name]).decode()
        inv.surfaces[key].context['default_examples'] = occurrences[name]

    exclusions = []
    categories = Counter()
    packs = {}
    resource_kinds = Counter()
    recipe_types = Counter()
    observed_fields = defaultdict(Counter)
    namespaces = Counter()
    for path, raw in sorted(resources.items()):
        parts = path.split('/')
        require(parts[0] == 'data' and len(parts) >= 2, f'invalid resource path: {path}')
        if len(parts) == 2:
            exclusions.append({'path': path, 'raw_sha256': digest(raw), 'bytes': len(raw)})
            if path != 'data/.mcassetsroot':
                inv.diagnostics.append({'kind': 'unexplained_data_metadata', 'path': path})
            continue
        categories[parts[2]] += 1
        namespace, logical = parts[1], '/'.join(parts[2:])
        namespaces[namespace] += 1
        pack = ['vanilla']
        if parts[2] == 'datapacks':
            require(len(parts) >= 5, f'invalid optional pack path: {path}')
            pack = ['embedded', namespace, parts[3]]
            tail = parts[4:]
            if tail == ['pack.mcmeta']:
                kind, identifier = 'pack_metadata', 'pack.mcmeta'
                namespace = parts[1]
                packs[canonical(pack).decode()] = read_json(raw)
                logical = None
            else:
                require(len(tail) >= 4 and tail[0] == 'data', f'unexplained optional pack file: {path}')
                namespace, logical = tail[1], '/'.join(tail[2:])
        if logical is not None:
            tag = logical.startswith('tags/')
            candidate = logical[5:] if tag else logical
            matches = [k for k in kinds if candidate.startswith(k + '/')]
            require(matches, f'unclassified resource kind: {path}')
            kind = max(matches, key=lambda k: len(k.split('/')))
            identifier = candidate[len(kind) + 1:]
            identifier = identifier.rsplit('.', 1)[0]
            if tag:
                kind = 'tags/' + kind
        value = read_json(raw) if path.endswith(('.json', '.mcmeta')) else {'opaque_bytes_sha256': digest(raw)}
        discriminator = value.get('type') if isinstance(value, dict) else None
        require(discriminator is None or isinstance(discriminator, str), f'non-string resource discriminator: {path}')
        group = ['resource', pack, namespace, kind, discriminator]
        if isinstance(value, dict) and path.endswith(('.json', '.mcmeta')):
            observed_fields[canonical(group).decode()].update(value.keys())
        resource_kinds[canonical([pack, namespace, kind]).decode()] += 1
        if pack == ['vanilla'] and kind == 'recipe':
            recipe_types[discriminator] += 1
        inv.add(Surface(['resource', pack, namespace, kind, identifier], 'resource', group,
                        {'archive': 'minecraft.jar', 'member': path}, value,
                        {'raw_sha256': digest(raw), 'bytes': len(raw), 'discriminator': discriminator}))

    packet_counts = Counter()
    for state, directions in sorted(reports['packets'].items()):
        for direction, entries in sorted(directions.items()):
            require(direction in {'clientbound', 'serverbound'}, f'unknown packet direction: {direction}')
            ids = set()
            for name, record in sorted(entries.items()):
                ref = source('packets', state, direction, name)
                inv.unknown(record, {'protocol_id'}, ref)
                require(type(record['protocol_id']) is int and record['protocol_id'] not in ids, 'duplicate/invalid packet ID')
                ids.add(record['protocol_id'])
                packet_counts[state] += 1
                inv.add(Surface(['packet', state, direction, name], 'packet', ['packet', state, direction], ref, record))

    inv.unknown(rpc, {'components', 'info', 'methods', 'openrpc'}, source('json-rpc-api-schema'))
    inv.unknown(rpc['components'], {'schemas'}, source('json-rpc-api-schema', 'components'))
    refs = []

    def references(value, parts):
        if isinstance(value, dict):
            for name, child in sorted(value.items()):
                if name == '$ref':
                    require(isinstance(child, str) and child.startswith('#/'), f'unresolved external OpenRPC ref: {child}')
                    resolved = at_pointer(rpc, child[1:])
                    refs.append({'pointer': pointer(*parts, name), 'ref': child, 'resolved_sha256': fingerprint(resolved)})
                references(child, parts + [name])
        elif isinstance(value, list):
            for index, child in enumerate(value):
                references(child, parts + [index])

    references(rpc, [])
    inv.unknown(rpc['info'], {'title', 'version'}, source('json-rpc-api-schema', 'info'))

    def schema_fields(schema, parts):
        if not isinstance(schema, dict):
            return
        inv.unknown(schema, {'$ref', 'type', 'enum', 'properties', 'items', 'required', 'additionalProperties',
                             'description', 'default', 'oneOf', 'anyOf', 'allOf', 'not'}, source('json-rpc-api-schema', *parts))
        for name, child in schema.get('properties', {}).items():
            schema_fields(child, parts + ['properties', name])
        for name in ('items', 'additionalProperties', 'not'):
            if name in schema:
                schema_fields(schema[name], parts + [name])
        for name in ('oneOf', 'anyOf', 'allOf'):
            for index, child in enumerate(schema.get(name, [])):
                schema_fields(child, parts + [name, index])

    for index, method in enumerate(rpc['methods']):
        ref = source('json-rpc-api-schema', 'methods', index)
        inv.unknown(method, {'name', 'description', 'params', 'result'}, ref)
        inv.add(Surface(['rpc_method', method['name']], 'rpc_method', ['rpc_method'], ref, method))
        for param_index, param in enumerate(method['params']):
            parts = ['methods', index, 'params', param_index]
            inv.unknown(param, {'name', 'required', 'schema'}, source('json-rpc-api-schema', *parts))
            schema_fields(param.get('schema'), parts + ['schema'])
        if 'result' in method:
            parts = ['methods', index, 'result']
            inv.unknown(method['result'], {'name', 'schema'}, source('json-rpc-api-schema', *parts))
            schema_fields(method['result'].get('schema'), parts + ['schema'])
    for name, schema in sorted(rpc['components']['schemas'].items()):
        schema_fields(schema, ['components', 'schemas', name])
        inv.add(Surface(['rpc_schema', name], 'rpc_schema', ['rpc_schema'],
                        source('json-rpc-api-schema', 'components', 'schemas', name), schema))

    inv.counts = {
        'registries': len(registries), 'registry_entries': sum(len(r['entries']) for r in registries.values()),
        'blocks': len(blocks), 'block_states': len(state_ids), 'command_roots': len(commands.get('children', {})),
        'command_nodes': command_counts['nodes'], 'command_executable': command_counts['executable'],
        'command_arguments': command_counts['argument'], 'command_redirects': command_counts['redirect'],
        'command_permissions': command_counts['permission'], 'resource_files': sum(categories.values()),
        'resource_top_categories': len(categories), 'datapack_registries': len(datapack['registries']),
        'datapack_others': len(datapack['others']), 'datapack_also_static': len(set(datapack['registries']) & registries.keys()),
        'item_reports': len(items), 'component_occurrences': sum(occurrences.values()),
        'component_types': len(component_types), 'component_types_with_defaults': len(+occurrences),
        'component_types_without_defaults': len(component_types - occurrences.keys()),
        'packets': sum(packet_counts.values()), 'packet_states': dict(sorted(packet_counts.items())),
        'rpc_methods': len(rpc['methods']), 'rpc_schemas': len(rpc['components']['schemas']),
    }
    inv.metadata = {
        'registry_metadata': registry_metadata, 'data_metadata_exclusions': exclusions,
        'outer_data_namespaces': dict(sorted(namespaces.items())),
        'resource_top_categories': dict(sorted(categories.items())),
        'resource_kinds': dict(sorted(resource_kinds.items())), 'embedded_packs': packs,
        'recipe_discriminators': dict(sorted(recipe_types.items())),
        'observed_resource_fields_not_schemas': {k: dict(sorted(v.items())) for k, v in sorted(observed_fields.items())},
        'component_occurrences': dict(sorted(occurrences.items())),
        'components_without_defaults': sorted(component_types - occurrences.keys()),
        'openrpc': {'version': rpc['openrpc'], 'info': rpc['info'], 'internal_refs': refs},
    }
    return inv


def load_inputs(mapping: dict, source_root: Path, artifact_root: Path) -> tuple[Inventory, dict, set[str]]:
    """Re-read retained artifacts; neither acquisition nor existing harness imports."""
    inputs = mapping['inputs']
    fields(inputs, {'official_files', 'item_reports', 'resource_manifest_sha256', 'data_metadata_paths'}, 'inputs')
    pins = inputs['official_files']
    require(set(pins) == {'registries', 'blocks', 'commands', 'datapack', 'packets',
                          'json-rpc-api-schema', 'server', 'minecraft'}, 'incomplete official source pins')
    official = {}
    input_hashes = {}
    for name, pin in sorted(pins.items()):
        fields(pin, {'path', 'bytes', 'sha256'}, f'input {name}')
        raw = verified(artifact_root, pin)
        official[name] = raw
        input_hashes[name] = {**pin, 'canonical_sha256': fingerprint(read_json(raw))} if name not in {'server', 'minecraft'} else pin
    target = read_json((source_root / 'compatibility/target.json').read_bytes())
    require(set(mapping['target']) == TARGET_FIELDS, 'incomplete target pin')
    require(mapping['target'] == {k: target[k] for k in mapping['target']}, 'authored target differs from pinned target')
    require(hashlib.sha1(official['server']).hexdigest() == target['server_sha1']
            and len(official['server']) == target['server_size'], 'outer JAR target differs')
    # The report directory itself is scanned, rather than an ignored review list.
    item_source = inputs['item_reports']
    fields(item_source, {'reports_directory', 'count', 'sha256'}, 'item reports')
    parent = relative(artifact_root, item_source['reports_directory'])
    item_pins, items = [], {}
    for path in sorted(parent.glob('*/components/**/*')):
        if not path.is_file():
            continue
        relative_path = path.relative_to(parent)
        require(len(relative_path.parts) >= 4 and relative_path.parts[1:3] == ('components', 'item')
                and path.suffix == '.json', f'unexplained component report: {relative_path}')
        raw = relative(artifact_root, str(path.relative_to(artifact_root))).read_bytes()
        item_pins.append({'path': str(path.relative_to(artifact_root)), 'bytes': len(raw), 'sha256': digest(raw)})
        name = relative_path.parts[0] + ':' + '/'.join(relative_path.parts[3:]).removesuffix('.json')
        require(name not in items, 'duplicate item report identity')
        items[name] = raw
    require(len(item_pins) == item_source['count'] and fingerprint(item_pins) == item_source['sha256'],
            'item report bytes/membership changed')
    with zipfile.ZipFile(relative(artifact_root, pins['server']['path'])) as outer:
        require(outer.read(f'META-INF/versions/{target["version"]}/server-{target["version"]}.jar') == official['minecraft'],
                'inner JAR differs from pinned outer member')
    with zipfile.ZipFile(relative(artifact_root, pins['minecraft']['path'])) as jar:
        names = jar.namelist()
        require(len(names) == len(set(names)), 'duplicate archive member')
        resources = {n: jar.read(n) for n in sorted(names) if n.startswith('data/') and not n.endswith('/')}
        version = read_json(jar.read('version.json'))
    resource_pins = [{'path': n, 'bytes': len(raw), 'sha256': digest(raw)} for n, raw in resources.items() if n.startswith('data/minecraft/')]
    require(fingerprint(resource_pins) == inputs['resource_manifest_sha256'], 'resource manifest digest mismatch')
    require([n for n in resources if len(n.split('/')) == 2] == inputs['data_metadata_paths'], 'data metadata boundary changed')
    reports = {name: read_json(raw) for name, raw in official.items() if name not in {'server', 'minecraft'}}
    require(version['id'] == target['version'] and version['protocol_version'] == target['protocol'], 'JAR target differs')
    inv = extract(reports, items, resources)
    inv.metadata['jar_version'] = version
    require(inv.counts == mapping['expected_counts'], 'finite source counts changed')
    ledger = read_json((source_root / 'compatibility/features.json').read_bytes())
    families = {f['id'] for f in ledger['features'] if f['scope'] != 'deferred_plugin'}
    # Review provenance is descriptive metadata, never a file dependency.
    source_refs = []
    for identifier, definition in sorted(mapping['classes'].items()):
        for ref in definition.get('source_refs', []):
            fields(ref, {'path', 'bytes', 'sha256', 'anchor'}, f'{identifier} source reference')
            raw = verified(source_root, ref)
            require(isinstance(ref['anchor'], str) and ref['anchor'].strip()
                    and ref['anchor'].encode() in raw, f'{identifier}: missing source anchor')
            source_refs.append({'class': identifier, **ref})
    provenance = {'inputs': input_hashes, 'source_refs': source_refs,
                  'item_directory_sha256': fingerprint(item_pins), 'resource_manifest_sha256': fingerprint(resource_pins)}
    return inv, provenance, families


def catalog(inv: Inventory) -> list[dict]:
    return [{'group': json.loads(key), 'members': member_pin(rows), 'content_sha256': content_pin(rows)}
            for key, rows in inv.groups().items()]


def resolve(inv: Inventory, mapping: dict, families: set[str]) -> tuple[list[dict], dict]:
    """Exactly one disposition per surface/facet; nothing inherits ledger status.

    Version 1 deliberately supports only explicit open and source-justified N/A.
    Evidence/implementation attachments are rejected rather than under-validated.
    """
    classes = mapping['classes']
    groups = inv.groups()
    assigned = {}
    output = []
    for identifier, definition in sorted(classes.items()):
        fields(definition, {'domain', 'families', 'boundary', 'facets', 'source_refs'}, identifier)
        domain = definition['domain']
        require(domain in FACETS, f'{identifier}: unknown domain')
        require(definition['families'] and set(definition['families']) <= families, f'{identifier}: nonexistent/deferred family')
        require(isinstance(definition['boundary'], str) and definition['boundary'].strip(), f'{identifier}: empty source boundary')
        require(definition['facets'] and set(definition['facets']) <= set(FACETS[domain]), f'{identifier}: invalid facets')
        for facet, disposition in definition['facets'].items():
            fields(disposition, {'disposition', 'code', 'reason', 'basis'}, f'{identifier}/{facet}')
            require(disposition['disposition'] in {'open', 'not_applicable'}, 'implementation/evidence dispositions are not supported')
            require(disposition['code'] in REASONS and isinstance(disposition['reason'], str)
                    and disposition['reason'].strip(), f'{identifier}/{facet}: reason required')
            if disposition['code'] == 'missing_route':
                require(definition.get('source_refs'), f'{identifier}/{facet}: missing route needs a bounded source reference')
            if disposition['disposition'] == 'not_applicable':
                require(disposition['code'] in {'nonserver_metadata', 'source_inapplicable'},
                        f'{identifier}/{facet}: N/A needs a source-inapplicability reason')
                basis = disposition.get('basis', {})
                require(set(basis) == {'key', 'content_sha256', 'pointer', 'value_sha256'}, f'{identifier}/{facet}: N/A requires exact source basis')
                row = inv.surfaces.get(canonical(basis['key']).decode())
                require(row is not None and row.content_sha256() == basis['content_sha256'], 'N/A source basis changed')
                require(fingerprint(at_pointer(row.value, basis['pointer'])) == basis['value_sha256'], 'N/A source value changed')
    for assignment in mapping['assignments']:
        fields(assignment, {'selector', 'members', 'content_sha256', 'class'}, 'assignment')
        selector = assignment['selector']
        fields(selector, {'group', 'exact', 'except'}, 'selector')
        require(('group' in selector) != ('exact' in selector), 'selector needs group or exact')
        if 'group' in selector:
            group_key = canonical(selector['group']).decode()
            require(group_key in groups, f'unknown source group: {group_key}')
            rows = groups[group_key]
        else:
            keys = [canonical(k).decode() for k in selector['exact']]
            require(len(set(keys)) == len(keys) and set(keys) <= inv.surfaces.keys(), 'invalid exact members')
            rows = [inv.surfaces[k] for k in keys]
        excluded = [canonical(k).decode() for k in selector.get('except', [])]
        require(len(set(excluded)) == len(excluded) and set(excluded) <= {canonical(r.key).decode() for r in rows}, 'invalid selector exceptions')
        rows = [r for r in rows if canonical(r.key).decode() not in excluded]
        require(rows and assignment['members'] == member_pin(rows), 'assignment membership changed')
        require(assignment['content_sha256'] == content_pin(rows), 'assignment content changed')
        require(assignment['class'] in classes, 'unknown obligation class')
        definition = classes[assignment['class']]
        require(all(row.domain == definition['domain'] for row in rows), 'class domain differs from selected members')
        for disposition in definition['facets'].values():
            if disposition['disposition'] == 'not_applicable':
                require(len(rows) == 1 and rows[0].key == disposition['basis']['key'], 'N/A basis must bind the exact selected member')
        for row in rows:
            for facet in definition['facets']:
                key = (canonical(row.key).decode(), facet)
                require(key not in assigned, f'overlapping assignment: {key}')
                assigned[key] = (assignment['class'], definition['facets'][facet])
        output.append({**assignment, 'dispositions': {f: d['disposition'] for f, d in definition['facets'].items()}})
    summary = {}
    unmapped = defaultdict(list)
    for key, row in sorted(inv.surfaces.items()):
        for facet in FACETS[row.domain]:
            counts = summary.setdefault(row.domain, {}).setdefault(facet, dict.fromkeys(
                ('enumerated', 'mapped', 'explicit_open', 'unmapped', 'not_applicable', 'implementation_present', 'paired_covered'), 0))
            counts['enumerated'] += 1
            disposition = assigned.get((key, facet))
            if disposition is None:
                counts['unmapped'] += 1
                unmapped[(row.domain, facet)].append(row)
            else:
                counts['mapped'] += 1
                counts['explicit_open' if disposition[1]['disposition'] == 'open' else 'not_applicable'] += 1
    for (domain, facet), rows in sorted(unmapped.items()):
        output.append({'unmapped': {'domain': domain, 'facet': facet}, 'members': member_pin(rows),
                       'examples': [r.key for r in rows[:3]], 'content_sha256': content_pin(rows)})
    return sorted(output, key=canonical), summary


def complete_errors(index: dict) -> list[str]:
    errors = []
    if index['diagnostics']:
        errors.append('unreviewed source diagnostics remain')
    if any(c['unmapped'] for facets in index['summary'].values() for c in facets.values()):
        errors.append('unmapped surface/facets remain')
    if not index['non_enumerated_obligations'] or not index['limits'] or not index['profiles']:
        errors.append('limits, profiles and non-enumerated obligations are required')
    return errors


def render(inv: Inventory, mapping: dict, provenance: dict, families: set[str]) -> dict:
    fields(mapping, {'schema_version', 'scope', 'target', 'inputs', 'review_provenance', 'expected_counts', 'classes', 'assignments',
                     'profiles', 'non_enumerated_obligations', 'limits'}, 'behavior map')
    require(type(mapping['schema_version']) is int and mapping['schema_version'] == 1, 'unsupported map version')
    require(isinstance(mapping['limits'], list) and all(isinstance(s, str) and s.strip() for s in mapping['limits']), 'limits must be explicit text')
    profile_ids = set()
    for profile in mapping['profiles']:
        fields(profile, {'id', 'boundary'}, 'profile')
        require(profile['id'] and profile['id'] not in profile_ids and profile['boundary'].strip(), 'invalid/duplicate profile')
        profile_ids.add(profile['id'])
    obligation_ids = set()
    for obligation in mapping['non_enumerated_obligations']:
        fields(obligation, {'id', 'families', 'reason'}, 'non-enumerated obligation')
        require(obligation['families'] and set(obligation['families']) <= families and obligation['reason'].strip(), 'invalid non-enumerated obligation')
        require(obligation['id'] and obligation['id'] not in obligation_ids, 'invalid/duplicate non-enumerated obligation')
        obligation_ids.add(obligation['id'])
    groups, summary = resolve(inv, mapping, families)
    return {'schema_version': 1, 'scope': mapping['scope'], 'target': mapping['target'],
            'behavior_map_sha256': fingerprint(mapping), 'generator_sha256': digest(Path(__file__).read_bytes()),
            'review_provenance': mapping.get('review_provenance', {}),
            'provenance': provenance, 'counts': inv.counts, 'metadata': inv.metadata,
            'diagnostics': sorted(inv.diagnostics, key=canonical), 'groups': groups, 'summary': summary,
            'profiles': mapping['profiles'], 'non_enumerated_obligations': mapping['non_enumerated_obligations'],
            'limits': mapping['limits']}


def expand(inv: Inventory, mapping: dict):
    """Produce auditable rows without checking a giant duplicated source into Git."""
    resolve(inv, mapping, {f for c in mapping['classes'].values() for f in c['families']})
    groups = inv.groups()
    assigned = {}
    for assignment in mapping['assignments']:
        selector = assignment['selector']
        rows = groups[canonical(selector['group']).decode()] if 'group' in selector else [inv.surfaces[canonical(k).decode()] for k in selector['exact']]
        excluded = {canonical(k).decode() for k in selector.get('except', [])}
        definition = mapping['classes'][assignment['class']]
        for row in rows:
            if canonical(row.key).decode() not in excluded:
                for facet, disposition in definition['facets'].items():
                    assigned[(canonical(row.key).decode(), facet)] = {'class': assignment['class'], **disposition}
    for key, row in sorted(inv.surfaces.items(), key=lambda item: (item[1].domain, item[0])):
        for facet in sorted(FACETS[row.domain]):
            yield {'domain': row.domain, 'key': row.key, 'facet': facet, 'source': row.source,
                   'content_sha256': row.content_sha256(), 'context': row.context,
                   'accounting': assigned.get((key, facet), {'disposition': 'unmapped'})}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path, default=ROOT)
    parser.add_argument('--artifact-root', type=Path, default=ROOT)
    parser.add_argument('--map', type=Path, default=ROOT / 'compatibility/behavior-map.json')
    parser.add_argument('--output', type=Path, default=ROOT / 'compatibility/surface-coverage.json')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--write', action='store_true', help='write deterministic compact index')
    mode.add_argument('--catalog', action='store_true', help='read-only source group pins, not a coverage gate')
    mode.add_argument('--expand', action='store_true', help='read-only JSONL rows with source pointers')
    mode.add_argument('--check', action='store_true', help='require byte-identical regeneration (default)')
    parser.add_argument('--complete-inventory', action='store_true', help='also reject unmapped work/unknown source fields; open accounting is allowed')
    args = parser.parse_args()
    try:
        mapping = read_json(args.map.read_bytes())
        inv, provenance, families = load_inputs(mapping, args.source_root, args.artifact_root)
        if args.catalog:
            require(not args.complete_inventory, '--catalog is not a completion gate')
            print(canonical({'counts': inv.counts, 'groups': catalog(inv), 'diagnostics': inv.diagnostics}).decode())
            return 0
        index = render(inv, mapping, provenance, families)
        raw = canonical(index) + b'\n'
        if args.write:
            args.output.write_bytes(raw)
        elif args.expand:
            for row in expand(inv, mapping):
                print(canonical(row).decode())
        else:
            require(args.output.read_bytes() == raw, 'generated index differs; review source/map changes before --write')
        if args.complete_inventory:
            errors = complete_errors(index)
            if errors:
                print('; '.join(errors), file=sys.stderr)
                return 1
        if not args.expand:
            totals = Counter()
            for facets in index['summary'].values():
                for counts in facets.values():
                    totals.update(counts)
            print(json.dumps({'accounting_only': True, 'facet_totals': dict(totals),
                              'unknown_source_diagnostics': len(index['diagnostics'])}))
        return 0
    except (OSError, ValueError, KeyError, TypeError, IndexError, zipfile.BadZipFile) as error:
        print(f'surface inventory invalid: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
