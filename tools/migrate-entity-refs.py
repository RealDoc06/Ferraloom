#!/usr/bin/env python3
"""Migrate legacy EntityRef fixture literals after the namespace micro lands.

Run from any worktree: python3 tools/migrate-entity-refs.py
Then run rustfmt on the reported files. --check reports remaining changes.
The namespace is explicit: pure model/manual-insertion fixtures use FIXTURE;
worldgen/IO fixtures which inspect allocated serials use their actual Level.
Production construction is deliberately unavailable; use next_entity_ref.
"""
from pathlib import Path
import argparse
import re

ROOT = Path(__file__).resolve().parent.parent
GAME = "crates/ferraloom-game/src/"
SCOPED = {
    GAME + "level/entity/worldgen.rs": "level.region_id()",
    GAME + "level/entity/persistent/tests.rs": "shared.region_id()",
}
SERIALS = {
    "level/entity/mod.rs": ("id", "entity"),
    "level/entity/persistent/tests.rs": ("id",),
    "level/entity/tick_tests.rs": ("id",),
    "entity/tests.rs": ("id", "entity"),
    "entity/relationships.rs": ("entity",),
    "entity/player/contact/tests.rs": ("entity",),
    "entity/tracking/tracked_entity/tests.rs": ("id",),
    "entity/predicate_facts.rs": ("id",),
    "advancements/tests.rs": ("entity",),
}


def migrate(path: Path, source: str) -> str:
    relative = path.relative_to(ROOT).as_posix()
    # These calls copied an existing handle. Keep the whole identity; make
    # its existing local mutable for the legacy &mut EntityRef forwarding API.
    for match in reversed(list(re.finditer(r"\bEntityRef\(id\.0\)", source))):
        source = source[:match.start()] + "id" + source[match.end():]
        binding = source.rfind("let id =", 0, match.start())
        if binding < 0:
            raise ValueError(f"{relative}: existing handle needs an explicit mutable local")
        source = source[:binding] + source[binding:].replace("let id =", "let mut id =", 1)
    # A function call cannot be a Rust pattern. Keep the original match order.
    source = re.sub(r"Some\(EntityRef\((\d+)\)\)(\s*)=>",
                    r"Some(entity) if entity == EntityRef(\1)\2=>", source)
    namespace = SCOPED.get(relative, "crate::level::RegionId::FIXTURE")
    source = re.sub(r"\bEntityRef\(", f"EntityRef::fixture({namespace}, ", source)
    if relative.startswith(GAME):
        for binding in SERIALS.get(relative[len(GAME):], ()):
            # Do not rewrite packet.id.0 or other unrelated tuple fields.
            source = re.sub(rf"(?<![\w.]){binding}\.0\b", f"{binding}.serial()", source)
        source = source.replace("next_entity_ref().0", "next_entity_ref().serial()")
    if relative == GAME + "level/entity/mod.rs":
        # This fixture deliberately imports a serial in the SAME birth namespace.
        source = source.replace("let existing = EntityRef::fixture(crate::level::RegionId::FIXTURE, 40);",
                                "let existing = EntityRef::fixture(level.region_id(), 40);")
        # The allocator test deliberately observes known allocated serials.
        source = re.sub(r"assert_eq!\(level\.next_entity_ref\(\), EntityRef::fixture\(crate::level::RegionId::FIXTURE, (\d+)\)\)",
                        r"assert_eq!(level.next_entity_ref().serial(), \1)", source)
    if relative == "crates/ferraloom-server/tests/generation.rs":
        # The game library's cfg(test) fixture helper is not available to an
        # integration target. The unused actor is a real allocated reference.
        source = source.replace("EntityRef::fixture(crate::level::RegionId::FIXTURE, 0)", "unused_actor")
        marker = "    let state = Block::STONE.default_state();\n"
        if "unused_actor" in source and "let unused_actor" not in source:
            source = source.replace(marker, "    let unused_actor = level.next_entity_ref();\n" + marker)
    return source


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    changed = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        source = path.read_text()
        updated = migrate(path, source)
        if source != updated:
            changed.append(path.relative_to(ROOT))
            if not args.check:
                path.write_text(updated)
    for path in changed:
        print(path)
    if args.check and changed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
