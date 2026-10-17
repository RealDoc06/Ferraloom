#!/usr/bin/env python3
"""Extract identifier/tag metadata from pinned official 26.3 reference inputs.

No implementation or asset payloads are copied. Dynamic entry order defines
Ferraloom's IDs; built-in tag members retain the official report's protocol IDs.
"""

import argparse
import hashlib
import json
from pathlib import Path
import zipfile


INNER_JAR_SHA256 = "a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6"
REGISTRY_REPORT_SHA256 = "51f62d56f8bd0e9134ee88dd9226b64e9e56cf6a32377540d44291bf47ecdac6"

# RegistryDataLoader.<clinit>, bci 1083..1657 in the exact inner JAR.
SYNCHRONIZED = """
worldgen/biome chat_type trim_pattern trim_material wolf_variant wolf_sound_variant
pig_variant pig_sound_variant frog_variant cat_variant cat_sound_variant
cow_sound_variant cow_variant chicken_sound_variant chicken_variant
zombie_nautilus_variant painting_variant sulfur_cube_archetype dimension_type
damage_type banner_pattern enchantment jukebox_song instrument test_environment
test_instance dialog world_clock timeline decorated_pot_pattern block_transformer
worldgen/block_state_provider
""".split()

# These default tag domains have no raw IDs in either input ID index.
EXCLUDED_DOMAINS = {
    "villager_trade", "worldgen/structure", "worldgen/world_preset",
    "worldgen/feature", "worldgen/flat_level_generator_preset",
}


def checked_bytes(path: Path, expected: str) -> bytes:
    data = path.read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise ValueError(f"{path}: SHA256 {actual}, expected {expected}")
    return data


def identifier(value: str) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError(f"Invalid identifier: {value!r}")
    return value if ":" in value else f"minecraft:{value}"


def resolve_tags(definitions: dict, ids: dict) -> dict:
    """Resolve one registry's tags; optional missing references alone may vanish."""
    resolved = {}
    visiting = []

    def resolve(tag):
        if tag in resolved:
            return resolved[tag]
        if tag in visiting:
            raise ValueError(f"Tag cycle: {' -> '.join([*visiting, tag])}")
        visiting.append(tag)
        members = set()
        for value in definitions[tag]:
            if isinstance(value, str):
                name, required = value, True
            elif isinstance(value, dict) and set(value) <= {"id", "required"}:
                name, required = value["id"], value.get("required", True)
                if not isinstance(required, bool):
                    raise ValueError(f"Invalid required flag in tag {tag}: {value!r}")
            else:
                raise ValueError(f"Invalid tag member in {tag}: {value!r}")
            reference = isinstance(name, str) and name.startswith("#")
            name = identifier(name[1:] if reference else name)
            if reference and name in definitions:
                members.update(resolve(name))
            elif not reference and name in ids:
                members.add(ids[name])
            elif required:
                raise ValueError(f"Tag {tag}: unresolved required {'tag' if reference else 'entry'} {name}")
        visiting.pop()
        resolved[tag] = sorted(members)
        return resolved[tag]

    for tag in sorted(definitions):
        resolve(tag)
    return resolved


def generate(jar_path: Path, report_path: Path) -> dict:
    # Check the JAR before parsing either input.
    checked_bytes(jar_path, INNER_JAR_SHA256)
    report = json.loads(checked_bytes(report_path, REGISTRY_REPORT_SHA256))
    indices = {
        registry: {name: entry["protocol_id"] for name, entry in data["entries"].items()}
        for registry, data in report.items()
    }
    registries = []
    tags = {}
    excluded = {}
    with zipfile.ZipFile(jar_path) as jar:
        names = sorted(jar.namelist())
        for registry in SYNCHRONIZED:
            prefix = f"data/minecraft/{registry}/"
            entries = [identifier(name[len(prefix):-5]) for name in names
                       if name.startswith(prefix) and name.endswith(".json")]
            if not entries:
                raise ValueError(f"No default entries found for synchronized registry {registry}")
            registry = identifier(registry)
            indices[registry] = {name: index for index, name in enumerate(entries)}
            registries.append({"id": registry, "entries": entries})

        domains = sorted([name.removeprefix("minecraft:") for name in indices]
                         + list(EXCLUDED_DOMAINS), key=lambda name: (-len(name), name))
        prefix = "data/minecraft/tags/"
        for path in names:
            if not path.startswith(prefix) or not path.endswith(".json"):
                continue
            relative = path[len(prefix):-5]
            domain = next((name for name in domains if relative.startswith(name + "/")), None)
            if domain is None:
                raise ValueError(f"Unclassified tag domain: {path}")
            registry = identifier(domain)
            tag = identifier(relative[len(domain) + 1:])
            if domain in EXCLUDED_DOMAINS:
                excluded.setdefault(registry, []).append(tag)
                continue
            data = json.loads(jar.read(path))
            if set(data) - {"replace", "values"} or not isinstance(data.get("values"), list):
                raise ValueError(f"Unsupported tag definition: {path}")
            if "replace" in data and not isinstance(data["replace"], bool):
                raise ValueError(f"Invalid tag replace flag: {path}")
            tags.setdefault(registry, {})[tag] = data["values"]

    return {
        "schema_version": 1,
        "minecraft_version": "26.3",
        "protocol_version": 777,
        "known_pack": {"namespace": "minecraft", "id": "core", "version": "26.3"},
        "sources": {"inner_jar_sha256": INNER_JAR_SHA256,
                    "registry_report_sha256": REGISTRY_REPORT_SHA256},
        "ordering": {"dynamic_entries": "lexicographic identifier order; array index is raw ID",
                     "builtin_entries": "official registry report protocol_id",
                     "tag_members": "sorted unique raw IDs"},
        "registries": registries,
        "tags": [
            {"registry": registry,
             "tags": [{"id": tag, "entries": members} for tag, members in
                      sorted(resolve_tags(definitions, indices[registry]).items())]}
            for registry, definitions in sorted(tags.items())
        ],
        "excluded_tag_domains": [
            {"registry": registry,
             "reason": "Not synchronized and absent from the pinned built-in registry report; no raw-ID index available",
             "tags": names}
            for registry, names in sorted(excluded.items())
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jar", type=Path, required=True)
    parser.add_argument("--registries", type=Path, required=True, help="Official generated registries.json")
    parser.add_argument("--output", type=Path, default=Path("reference/26.3/registries.json"))
    args = parser.parse_args()
    result = generate(args.jar, args.registries)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(result['registries'])} registries, "
          f"{sum(len(registry['entries']) for registry in result['registries'])} entries, "
          f"{sum(len(registry['tags']) for registry in result['tags'])} tags; "
          f"{args.output.stat().st_size} bytes")


if __name__ == "__main__":
    main()
