"""Finite accounting contracts. Synthetic inputs never start an engine or JVM."""

from copy import deepcopy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

from tools import surface_coverage as coverage


def fixture():
    registries = {
        'minecraft:item': {'protocol_id': 0, 'entries': {'minecraft:box': {'protocol_id': 0}}},
        'minecraft:data_component_type': {'protocol_id': 1, 'entries': {
            'minecraft:custom_data': {'protocol_id': 0},
            'minecraft:instrument': {'protocol_id': 1},
            'minecraft:creative_slot_lock': {'protocol_id': 2}}},
        'minecraft:entity_type': {'protocol_id': 2, 'entries': {'minecraft:mob': {'protocol_id': 0}}},
        'minecraft:block_entity_type': {'protocol_id': 3, 'entries': {'minecraft:box': {'protocol_id': 0}}},
        'minecraft:menu': {'protocol_id': 4, 'entries': {'minecraft:box': {'protocol_id': 0}}},
    }
    commands = {'type': 'root', 'children': {
        'box': {'type': 'literal', 'executable': True, 'permissions': {'role': 'gamemasters'},
                'children': {'count': {'type': 'argument', 'parser': 'brigadier:integer',
                                       'properties': {'min': 1, 'max': 8}, 'executable': True,
                                       'permissions': {'role': 'owners'}}}},
        'alias_a': {'type': 'literal', 'redirect': ['alias_b']},
        'alias_b': {'type': 'literal', 'redirect': ['alias_a']}}}
    reports = {
        'registries': registries,
        'blocks': {'minecraft:box': {'properties': {'open': ['false', 'true']}, 'states': [
            {'id': 3, 'properties': {'open': 'false'}, 'default': True},
            {'id': 97, 'properties': {'open': 'true'}}]}},
        'commands': commands,
        'datapack': {'registries': {
            'minecraft:worldgen/biome': {'elements': True, 'tags': True, 'stable': True},
            'minecraft:empty': {'elements': True, 'tags': True, 'stable': False}},
            'others': {'function': {'format': 'mcfunction'}, 'structure': {'format': 'nbt'}}},
        'packets': {'play': {'serverbound': {'minecraft:use': {'protocol_id': 9}}}},
        'json-rpc-api-schema': {'openrpc': '1.2.6', 'info': {'title': 'fixture', 'version': '1'},
            'methods': [{'name': 'minecraft:box', 'params': [{'name': 'value', 'required': True,
                         'schema': {'$ref': '#/components/schemas/Box'}}],
                         'result': {'name': 'result', 'schema': {'type': 'boolean'}}}],
            'components': {'schemas': {'Box': {'type': 'object', 'properties': {'value': {'type': 'integer'}}}}}},
    }
    items = {'minecraft:box': coverage.canonical({'components': {
        'minecraft:custom_data': {'components': {'arbitrary': 4}},
        'minecraft:instrument': 'minecraft:ponder_goat_horn'}})}
    resources = {
        'data/.mcassetsroot': b'',
        'data/minecraft/worldgen/biome/plains.json': b'{"temperature":0.8}',
        'data/minecraft/tags/worldgen/biome/plains.json': b'{"values":[]}',
        'data/minecraft/datapacks/test/pack.mcmeta': b'{"pack":{"description":"fixture"}}',
        'data/minecraft/datapacks/test/data/minecraft/worldgen/biome/plains.json': b'{"temperature":0.1}',
    }
    return reports, items, resources


def accounting(inv):
    """Deliberately open fixtures, pinned before each adversarial mutation."""
    classes, assignments = {}, []
    for number, row in enumerate(coverage.catalog(inv)):
        identifier = f'open.{number}'
        domain = row['group'][0]
        classes[identifier] = {
            'domain': domain, 'families': ['fixture.family'], 'boundary': 'Exact synthetic source group.',
            'facets': {facet: {'disposition': 'open', 'code': 'unassessed_behavior',
                               'reason': 'Implementation and reference observations are unassessed.'}
                       for facet in coverage.FACETS[domain]}}
        assignments.append({'selector': {'group': row['group']}, 'members': row['members'],
                            'content_sha256': row['content_sha256'], 'class': identifier})
    return {'schema_version': 1, 'scope': 'Synthetic accounting only.', 'target': {},
            'classes': classes, 'assignments': assignments,
            'profiles': [{'id': 'fixture', 'boundary': 'Unobserved runtime profile.'}],
            'non_enumerated_obligations': [{'id': 'lifecycle', 'families': ['fixture.family'],
                                           'reason': 'Unbounded lifecycle remains open.'}],
            'limits': ['Finite reports cannot enumerate accepted custom inputs.']}


class SurfaceCoverageTests(unittest.TestCase):
    def test_pinned_inputs_regenerate_without_private_review_cache(self):
        reports, items, resources = fixture()
        inv = coverage.extract(reports, items, resources)
        mapping = accounting(inv)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'compatibility').mkdir()
            target = {'edition': 'java', 'version': 'fixture', 'protocol': 1, 'java_major': 25,
                      'metadata_sha1': 'test', 'server_sha1': '', 'server_size': 0}
            inner_bytes = io.BytesIO()
            with zipfile.ZipFile(inner_bytes, 'w') as jar:
                jar.writestr('version.json', json.dumps({'id': 'fixture', 'protocol_version': 1}))
                for name, raw in resources.items():
                    jar.writestr(name, raw)
            outer_bytes = io.BytesIO()
            with zipfile.ZipFile(outer_bytes, 'w') as jar:
                jar.writestr('META-INF/versions/fixture/server-fixture.jar', inner_bytes.getvalue())
            raw_outer = outer_bytes.getvalue()
            target.update(server_sha1=hashlib.sha1(raw_outer).hexdigest(), server_size=len(raw_outer))
            mapping['target'] = target
            (root / 'compatibility/target.json').write_bytes(coverage.canonical(target))
            ledger = root / 'compatibility/features.json'
            ledger.write_text('{"features":[{"id":"fixture.family","scope":"vanilla","rust_status":"partial"}]}')
            pins = {}
            def retain(name, path, raw):
                destination = root / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(raw)
                pins[name] = {'path': path, 'bytes': len(raw), 'sha256': coverage.digest(raw)}
            for name, report in reports.items():
                retain(name, f'.cache/vanilla/fixture/generated/reports/{name}.json', coverage.canonical(report))
            retain('server', '.cache/vanilla/fixture/server.jar', raw_outer)
            retain('minecraft', '.cache/vanilla/fixture/minecraft.jar', inner_bytes.getvalue())
            item_path = '.cache/vanilla/fixture/generated/reports/minecraft/components/item/box.json'
            item_file = root / item_path
            item_file.parent.mkdir(parents=True)
            item_file.write_bytes(items['minecraft:box'])
            item_manifest = [{'path': item_path, 'bytes': item_file.stat().st_size,
                              'sha256': coverage.digest(item_file.read_bytes())}]
            resource_manifest = [{'path': name, 'bytes': len(raw), 'sha256': coverage.digest(raw)}
                                 for name, raw in sorted(resources.items()) if name.startswith('data/minecraft/')]
            mapping['inputs'] = {'official_files': pins, 'item_reports': {
                'reports_directory': '.cache/vanilla/fixture/generated/reports', 'count': 1,
                'sha256': coverage.fingerprint(item_manifest)},
                'resource_manifest_sha256': coverage.fingerprint(resource_manifest),
                'data_metadata_paths': ['data/.mcassetsroot']}
            mapping['expected_counts'] = inv.counts
            mapping['review_provenance'] = {'design': {'path': '.cache/reviews/absent/design.md', 'sha256': 'historical'}}
            source_file = root / 'fixture_source.rs'
            source_file.write_bytes(b'// bounded source anchor\n')
            next(iter(mapping['classes'].values()))['source_refs'] = [{
                'path': source_file.name, 'bytes': source_file.stat().st_size,
                'sha256': coverage.digest(source_file.read_bytes()), 'anchor': 'bounded source anchor'}]
            loaded, provenance, families = coverage.load_inputs(mapping, root, root)
            first = coverage.render(loaded, mapping, provenance, families)
            self.assertFalse((root / '.cache/reviews').exists())
            # Published prose and family status bytes do not define finite membership.
            (root / 'README.md').write_text('An unrelated publication changed.')
            ledger.write_text(ledger.read_text().replace('partial', 'verified'))
            again, again_provenance, again_families = coverage.load_inputs(mapping, root, root)
            self.assertEqual(coverage.canonical(first), coverage.canonical(
                coverage.render(again, mapping, again_provenance, again_families)))
            self.assertEqual([], coverage.complete_errors(first))
            for mutation in ('default', 'missing_report', 'source_bytes', 'extra_item'):
                with self.subTest(mutation=mutation):
                    if mutation == 'default':
                        item_file.write_bytes(items['minecraft:box'].replace(b'4', b'5'))
                    elif mutation == 'missing_report':
                        (root / pins['commands']['path']).unlink()
                    elif mutation == 'source_bytes':
                        source_file.write_bytes(b'// changed source anchor\n')
                    else:
                        item_file.with_name('extra.json').write_bytes(items['minecraft:box'])
                    with self.assertRaises((coverage.Invalid, FileNotFoundError)):
                        coverage.load_inputs(mapping, root, root)
                    item_file.write_bytes(items['minecraft:box'])
                    source_file.write_bytes(b'// bounded source anchor\n')
                    (root / pins['commands']['path']).write_bytes(coverage.canonical(reports['commands']))
            for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'{"a":1e999}'):
                with self.assertRaises(ValueError):
                    coverage.read_json(raw)

    def test_command_graph_preserves_executable_parents_permissions_and_cycles(self):
        reports, items, resources = fixture()
        inv = coverage.extract(reports, items, resources)
        parent = inv.surfaces['["command",["box"]]']
        argument = inv.surfaces['["command",["box","count"]]']
        self.assertEqual((5, 2, 2, 2), tuple(inv.counts[k] for k in (
            'command_nodes', 'command_executable', 'command_redirects', 'command_permissions')))
        self.assertTrue(parent.value['executable'])
        self.assertEqual(['root', 'literal', 'argument'], argument.context['path_kinds'])
        self.assertEqual(['gamemasters', 'owners'], [p['permission']['role'] for p in argument.context['permission_chain']])
        self.assertEqual({'min': 1, 'max': 8}, argument.value['properties'])
        self.assertEqual(argument.value, coverage.at_pointer(reports['commands'], argument.source['pointer']))
        for field in ('executable', 'permissions', 'redirect'):
            changed = deepcopy(reports)
            node = changed['commands']['children']['alias_a' if field == 'redirect' else 'box']
            node[field] = ['box'] if field == 'redirect' else False if field == 'executable' else {'role': 'admins'}
            with self.subTest(field=field), self.assertRaises(coverage.Invalid):
                coverage.resolve(coverage.extract(changed, items, resources), accounting(inv), {'fixture.family'})
        reports['commands']['children']['box']['unknown_predicate'] = True
        unknown = coverage.extract(reports, items, resources)
        self.assertIn('unreviewed source diagnostics remain', coverage.complete_errors(
            coverage.render(unknown, accounting(unknown), {}, {'fixture.family'})))

    def test_exact_partitions_reject_overlap_growth_and_unexplained_holes(self):
        reports, items, resources = fixture()
        inv = coverage.extract(reports, items, resources)
        mapping = accounting(inv)
        _, summary = coverage.resolve(inv, mapping, {'fixture.family'})
        self.assertTrue(all(c['explicit_open'] == c['enumerated'] for fs in summary.values() for c in fs.values()))
        for mutation in ('overlap', 'family', 'new_member', 'not_applicable'):
            changed = deepcopy(mapping)
            other = deepcopy(reports)
            if mutation == 'overlap':
                changed['assignments'].append(deepcopy(changed['assignments'][0]))
            elif mutation == 'family':
                next(iter(changed['classes'].values()))['families'] = ['nonexistent.family']
            elif mutation == 'new_member':
                other['registries']['minecraft:menu']['entries']['minecraft:new'] = {'protocol_id': 1}
            else:
                facet = next(iter(next(iter(changed['classes'].values()))['facets'].values()))
                facet['disposition'] = 'not_applicable'
            with self.subTest(mutation=mutation), self.assertRaises(coverage.Invalid):
                coverage.resolve(coverage.extract(other, items, resources), changed, {'fixture.family'})
        mapping['assignments'].pop()
        index = coverage.render(inv, mapping, {}, {'fixture.family'})
        self.assertIn('unmapped surface/facets remain', coverage.complete_errors(index))
        self.assertTrue(any('unmapped' in group for group in index['groups']))
        # N/A must bind one exact source member and the actual predicate value.
        mapping = accounting(inv)
        parent = inv.surfaces['["command",["box"]]']
        assignment = next(a for a in mapping['assignments'] if a['selector']['group'] == parent.group)
        assignment.update(selector={'exact': [parent.key]}, members=coverage.member_pin([parent]),
                          content_sha256=coverage.content_pin([parent]))
        definition = mapping['classes'][assignment['class']]
        definition['facets'] = {'parser_properties': {
            'disposition': 'not_applicable', 'code': 'source_inapplicable',
            'reason': 'This literal node has no argument parser; its child argument stays separate.',
            'basis': {'key': parent.key, 'content_sha256': parent.content_sha256(),
                      'pointer': '/type', 'value_sha256': coverage.fingerprint('literal')}}}
        _, summary = coverage.resolve(inv, mapping, {'fixture.family'})
        self.assertEqual(1, summary['command']['parser_properties']['not_applicable'])
        definition['facets']['parser_properties']['basis']['value_sha256'] = coverage.fingerprint('argument')
        with self.assertRaises(coverage.Invalid):
            coverage.resolve(inv, mapping, {'fixture.family'})

    def test_default_examples_never_supply_codec_schema_or_holder_acceptance(self):
        reports, items, resources = fixture()
        inv = coverage.extract(reports, items, resources)
        self.assertEqual(['minecraft:creative_slot_lock'], inv.metadata['components_without_defaults'])
        opaque = inv.surfaces['["item_default","minecraft:box","minecraft:custom_data"]']
        self.assertEqual({'components': {'arbitrary': 4}}, opaque.value)
        changed_items = dict(items)
        changed_items['minecraft:box'] = items['minecraft:box'].replace(b'4', b'5')
        changed = coverage.extract(reports, changed_items, resources)
        self.assertNotEqual(opaque.content_sha256(), changed.surfaces[coverage.canonical(opaque.key).decode()].content_sha256())
        mapping = accounting(inv)
        assignment = next(a for a in mapping['assignments'] if a['selector']['group'] == ['item_default', 'minecraft:instrument'])
        definition = mapping['classes'][assignment['class']]
        definition['facets'] = {'reported_value': definition['facets']['reported_value']}
        rows = list(coverage.expand(inv, mapping))
        instrument = [r for r in rows if r['key'] == ['item_default', 'minecraft:box', 'minecraft:instrument']]
        self.assertEqual('open', next(r for r in instrument if r['facet'] == 'reported_value')['accounting']['disposition'])
        self.assertTrue(all(r['accounting']['disposition'] == 'unmapped' for r in instrument if r['facet'] != 'reported_value'))
        # No direct-holder/patch/removal or network-only persistence rule is invented.
        self.assertEqual('minecraft:ponder_goat_horn', inv.surfaces[
            '["item_default","minecraft:box","minecraft:instrument"]'].value)
        self.assertTrue(all(r['accounting']['disposition'] == 'open' for r in rows
                            if r['key'] == ['registry', 'minecraft:data_component_type', 'minecraft:creative_slot_lock']))

    def test_positive_and_evidence_claims_fail_closed_in_accounting_version(self):
        inv = coverage.extract(*fixture())
        base = accounting(inv)
        for attachment in (
            {'implementation': 'present'}, {'implementation_refs': [{'path': 'factory.rs'}]},
            {'scenario_refs': [{'test_id': 'paired', 'member': 'wrong', 'profile': 'wrong'}]},
            {'evidence': [{'rust': 'old-binary', 'vanilla': 'passed', 'suite': 'wrong'}]},
            {'evidence': [{'result': 'failed'}]}, {'evidence': [{'result': 'skipped'}]},
            {'evidence': [{'result': 'passed', 'kind': 'native-only'}]},
        ):
            mapping = deepcopy(base)
            next(iter(mapping['classes'].values())).update(attachment)
            with self.subTest(attachment=attachment), self.assertRaises(coverage.Invalid):
                coverage.resolve(inv, mapping, {'fixture.family'})
        mapping = deepcopy(base)
        next(iter(next(iter(mapping['classes'].values()))['facets'].values()))['disposition'] = 'present'
        with self.assertRaises(coverage.Invalid):
            coverage.resolve(inv, mapping, {'fixture.family'})
        index = coverage.render(inv, base, {}, {'fixture.family'})
        self.assertTrue(all(c['implementation_present'] == c['paired_covered'] == 0
                            for fs in index['summary'].values() for c in fs.values()))

    def test_roles_nested_kinds_optional_packs_and_empty_descriptors_stay_distinct(self):
        inv = coverage.extract(*fixture())
        states = [s for s in inv.surfaces.values() if s.domain == 'block_state']
        self.assertEqual({3, 97}, {s.key[-1] for s in states})
        self.assertTrue(all(s.context['block'] == ['block', 'minecraft:box'] for s in states))
        base = ['resource', ['vanilla'], 'minecraft', 'worldgen/biome', 'plains']
        optional = ['resource', ['embedded', 'minecraft', 'test'], 'minecraft', 'worldgen/biome', 'plains']
        self.assertNotEqual(inv.surfaces[coverage.canonical(base).decode()].content_sha256(),
                            inv.surfaces[coverage.canonical(optional).decode()].content_sha256())
        self.assertIn('["resource",["vanilla"],"minecraft","tags/worldgen/biome","plains"]', inv.surfaces)
        self.assertIn('["datapack_descriptor","registries","minecraft:empty"]', inv.surfaces)
        rows = list(coverage.expand(inv, accounting(inv)))
        self.assertEqual(set(coverage.FACETS['entity_type']), {r['facet'] for r in rows if r['domain'] == 'entity_type'})
        self.assertEqual(set(coverage.FACETS['block_entity_type']), {r['facet'] for r in rows if r['domain'] == 'block_entity_type'})
        self.assertFalse(any('block_entity' in s.context or 'menu' in s.context for s in states))
        self.assertEqual([], coverage.complete_errors(coverage.render(inv, accounting(inv), {}, {'fixture.family'})))


if __name__ == '__main__':
    unittest.main()
