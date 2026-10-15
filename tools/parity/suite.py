#!/usr/bin/env python3
"""Run identical scenarios against isolated official and Ferraloom servers."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import gzip
import json
import os
from pathlib import Path
import queue
import re
import signal
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from oracle import ROOT, TARGET, CACHE, acquire, java_command, inventory, compare_registry
from parity import protocol
from parity.bot import run_client_scenarios
from parity.worlds import decode_nbt, compare_terrain

ANSI = re.compile(r"\x1b\[[0-9;]*m")


def harness_manifest() -> dict[str, str]:
    inputs = sorted((ROOT / "tools/parity").glob("*.py")) + sorted((ROOT / "tools/parity").glob("*.ts"))
    inputs += [ROOT / "tools/oracle.py", ROOT / "compatibility/target.json", ROOT / "compatibility/scenario-map.json"]
    return {str(path.relative_to(ROOT)): hashlib.file_digest(path.open("rb"), "sha256").hexdigest() for path in inputs}


def git_state() -> dict:
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True)
    status = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True)
    return {"commit": commit.stdout.strip() if commit.returncode == 0 else None, "dirty": bool(status.stdout.strip())}


def data_version(world: Path) -> int:
    root = decode_nbt(gzip.decompress((world / "level.dat").read_bytes()))
    return root["Data"]["DataVersion"]


def is_server_say(line: str, token: str) -> bool:
    return bool(re.search(r"\[Server\]\s+" + re.escape(token) + r"$", line))


@dataclass(frozen=True)
class Scenario:
    id: str
    features: tuple[str, ...]
    commands: tuple[str, ...]
    predicate: str
    settle: float = 0.15
    precondition: str | None = None
    force_dimension: str | None = None
    expected_last_feedback: str | None = None


DATA_CONDITION_PROVIDERS = (("Block", "block 4 230 4", "id"),
                            ("Entity", "entity @e[type=pig,tag=ferraloom_data_source,limit=1]", "Health"),
                            ("Storage", "storage ferraloom:modify_source", "answer"))

SCENARIOS = [
    Scenario("block.setblock", ("blocks.placement",), ("setblock 0 200 0 stone",), "if block 0 200 0 stone"),
    Scenario("block.state", ("blocks.registry",), ("setblock 0 200 0 oak_stairs[facing=east,half=top]",), "if block 0 200 0 oak_stairs[facing=east,half=top]"),
    Scenario("block.fill", ("players.commands",), ("fill 0 200 0 2 201 2 glass",), " ".join(f"if block {x} {y} {z} glass" for x in range(3) for y in range(200, 202) for z in range(3))),
    Scenario("block.clone", ("players.commands",), ("setblock 0 200 0 diamond_block", "clone 0 200 0 0 200 0 5 200 5"), "if block 5 200 5 diamond_block"),
    Scenario("block.water_flow", ("blocks.fluids",), ("fill 0 200 0 3 204 3 air", "setblock 1 203 1 water"), "if block 1 202 1 water", 1.5),
    Scenario("block.gravity", ("blocks.falling",), ("fill 0 200 0 3 204 3 air", "setblock 1 203 1 sand"), "if block 1 200 1 sand", 1.5),
    Scenario("redstone.lamp", ("blocks.redstone_power",), ("setblock 8 200 0 redstone_lamp", "setblock 9 200 0 redstone_block"), "if block 8 200 0 redstone_lamp[lit=true]", 0.5),
    Scenario("redstone.wire", ("blocks.redstone_power",), ("setblock 8 200 3 redstone_wire", "setblock 9 200 3 redstone_block"), "if block 8 200 3 redstone_wire[power=15]", 0.5),
    Scenario("redstone.piston_push", ("blocks.pistons",), ("fill 0 200 8 4 202 10 air", "setblock 1 200 9 stone", "setblock 0 200 9 piston[facing=east]", "setblock -1 200 9 redstone_block"), "if block 2 200 9 stone", 1.0),
    Scenario("redstone.daylight", ("blocks.redstone_power",), ("time set 6000", "weather clear", "setblock 12 200 0 daylight_detector"), "if block 12 200 0 daylight_detector[power=15]", 1.5),
    Scenario("scoreboard.arithmetic", ("players.scoreboards",), ("scoreboard objectives add parity dummy", "scoreboard players set A parity 7", "scoreboard players set B parity 5", "scoreboard players operation A parity += B parity"), "if score A parity matches 12"),
    Scenario("block.fill_destroy", ("players.commands",), ("fill 0 210 0 1 210 1 stone", "execute store result score FillDestroyed parity run fill 0 210 0 1 210 1 glass destroy"), "if score FillDestroyed parity matches 4 if block 0 210 0 glass if block 0 210 1 glass if block 1 210 0 glass if block 1 210 1 glass"),
    Scenario("block.fill_destroy_sponge", ("players.commands",),
             ("fill 0 224 0 2 224 2 stone", "fill 0 225 0 2 226 2 air",
              "setblock 1 225 0 water", "setblock 0 225 0 stone",
              "execute store result score FillSponge parity run fill 0 225 0 0 225 0 sponge destroy"),
             "if score FillSponge parity matches 1 if block 0 225 0 wet_sponge if block 1 225 0 air"),
    Scenario("block.fill_destroy_shapes", ("players.commands",),
             ("fill 4 224 4 6 224 4 stone", "fill 4 225 4 6 226 4 air",
              "setblock 5 225 4 oak_fence", "setblock 4 225 4 stone",
              "execute store result score FillFence parity run fill 4 225 4 4 225 4 oak_fence destroy"),
             "if score FillFence parity matches 1 if block 4 225 4 oak_fence[east=true] if block 5 225 4 oak_fence[west=true]"),
    Scenario("block.fill_destroy_same_fluid", ("players.commands",),
             ("fill 8 224 4 10 225 6 stone", "setblock 9 225 5 water",
              "execute store result score FillFluidControl parity store success score FillFluidControlSuccess parity run fill 10 225 5 10 225 5 water destroy",
              "execute store result score FillFluidResult parity store success score FillFluidSuccess parity run fill 9 225 5 9 225 5 water destroy"),
             "if score FillFluidControl parity matches 1 if score FillFluidControlSuccess parity matches 1 if score FillFluidResult parity matches 0 if score FillFluidSuccess parity matches 0 if block 9 225 5 water[level=0]",
             expected_last_feedback="No blocks were filled"),
    Scenario("block.fill_destroy_container_replacement", ("players.commands",),
             ("setblock 0 231 0 stone", "setblock 0 232 0 oak_sign",
              "execute store result score SignChest parity run fill 0 232 0 0 232 0 chest destroy"),
             'if score SignChest parity matches 1 if block 0 232 0 chest if data block 0 232 0 {id:"minecraft:chest"}'),
    Scenario("block.fill_destroy_sign_replacement", ("players.commands",),
             ("setblock 2 231 0 stone", "setblock 2 232 0 oak_sign",
              "execute store result score SignSign parity run fill 2 232 0 2 232 0 spruce_sign destroy"),
             'if score SignSign parity matches 1 if block 2 232 0 spruce_sign if data block 2 232 0 {id:"minecraft:sign"}'),
    Scenario("scoreboard.objective_count", ("players.scoreboards",), ("execute store result score ObjectiveFirst parity run scoreboard objectives add counted_a dummy", "execute store result score ObjectiveSecond parity run scoreboard objectives add counted_b dummy", "execute store result score ObjectiveRemovedA parity run scoreboard objectives remove counted_a", "execute store result score ObjectiveRemovedB parity run scoreboard objectives remove counted_b", "scoreboard players remove ObjectiveSecond parity 1", "scoreboard players add ObjectiveRemovedB parity 1"), "if score ObjectiveSecond parity = ObjectiveFirst parity if score ObjectiveRemovedA parity = ObjectiveFirst parity if score ObjectiveRemovedB parity = ObjectiveFirst parity"),
    Scenario("scoreboard.display_set", ("players.scoreboards",), ("scoreboard players set DisplayResult parity 17", "scoreboard players set DisplaySuccess parity 17", "execute store result score DisplayResult parity store success score DisplaySuccess parity run scoreboard objectives setdisplay sidebar parity"), "if score DisplayResult parity matches 0 if score DisplaySuccess parity matches 1"),
    Scenario("scoreboard.trigger_enable", ("players.scoreboards",), ("scoreboard objectives add unlock trigger", "scoreboard players set Candidate unlock 13", "scoreboard players enable Candidate unlock", "execute store success score EnableAgain parity run scoreboard players enable Candidate unlock"), "if score Candidate unlock matches 13 if score EnableAgain parity matches 0"),
    Scenario("command.store_result_false", ("players.commands",), ("gamerule keep_inventory false", "scoreboard players set Stored parity 17", "execute store result score Stored parity run gamerule keep_inventory"), "if score Stored parity matches 0"),
    Scenario("command.store_result_signed", ("players.commands",), ("scoreboard players set Query parity -7", "execute store result score Stored parity run scoreboard players get Query parity"), "if score Stored parity matches -7"),
    Scenario("command.store_success", ("players.commands",), ("execute store success score Stored parity run gamerule keep_inventory",), "if score Stored parity matches 1"),
    Scenario("command.store_failure", ("players.commands",), ("scoreboard players set Stored parity 17", "execute store result score Stored parity run scoreboard players get NeverSet parity"), "if score Stored parity matches 0"),
    Scenario("command.store_chain", ("players.commands",), ("execute store result score Stored parity store success score Succeeded parity run gamerule keep_inventory",), "if score Stored parity matches 0 if score Succeeded parity matches 1"),
    Scenario("command.store_condition", ("players.commands",), ("scoreboard players set Stored parity 17", "execute store result score Stored parity if score A parity matches 999 run gamerule keep_inventory"), "if score Stored parity matches 17"),
    Scenario("data.storage", ("players.commands",), ('data modify storage ferraloom:probe answer set value 42',), "if data storage ferraloom:probe {answer:42}"),
    Scenario("data.modify_merge_recursive", ("players.commands",),
             ('data merge storage ferraloom:merge_source {parts:[{nested:{first:3,replace:4},list:[2],scalar:{inside:7},object:2},{nested:{last:5,replace:6}}]}',
              'data merge storage ferraloom:merge_recursive {untouched:99,object:{sibling:2,nested:{keep:1,replace:0},list:[1],scalar:0,object:{old:1}}}',
              'execute store result score MergeRecursive parity run data modify storage ferraloom:merge_recursive object merge from storage ferraloom:merge_source parts[]',
              'execute store result score MergeListLength parity run data get storage ferraloom:merge_recursive object.list'),
             'if score MergeRecursive parity matches 1 if score MergeListLength parity matches 1 if data storage ferraloom:merge_recursive {untouched:99,object:{sibling:2,nested:{keep:1,first:3,last:5,replace:6},list:[2],scalar:{inside:7},object:2}} if data storage ferraloom:merge_source {parts:[{nested:{first:3,replace:4},list:[2],scalar:{inside:7},object:2},{nested:{last:5,replace:6}}]}'),
    Scenario("data.modify_merge_counts", ("players.commands",),
             ('data merge storage ferraloom:merge_counts_source {parts:[{nested:{value:1,extra:5}},{nested:{value:2}}]}',
              'data merge storage ferraloom:merge_counts {rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5}}},{kind:hit,payload:{keep:2,nested:{value:0}}},{kind:skip,payload:{keep:3,nested:{value:0}}}]}',
              'execute store result score MergeChanged parity run data modify storage ferraloom:merge_counts rows[].payload merge from storage ferraloom:merge_counts_source parts[]',
              'execute store result score MergeMatched parity run data modify storage ferraloom:merge_counts rows[{kind:hit}].payload merge value {added:8}',
              'execute store success score MergeUnchanged parity run data modify storage ferraloom:merge_counts rows[].payload merge from storage ferraloom:merge_counts_source parts[]'),
             'if score MergeChanged parity matches 2 if score MergeMatched parity matches 2 if score MergeUnchanged parity matches 0 if data storage ferraloom:merge_counts {rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5},added:8}},{kind:hit,payload:{keep:2,nested:{value:2,extra:5},added:8}},{kind:skip,payload:{keep:3,nested:{value:2,extra:5}}}]}',
             expected_last_feedback="Nothing changed. The specified properties already have these values"),
    Scenario("data.modify_merge_invalid_source", ("players.commands",),
             ('data merge storage ferraloom:merge_bad_source {mixed:[{value:{added:2}},{value:7}]}',
              'data merge storage ferraloom:merge_bad_target {object:{keep:1},scalar:7,numbers:[I;3,4]}',
              'execute store success score MergeBadSource parity run data modify storage ferraloom:merge_bad_target missing.path merge from storage ferraloom:merge_bad_source mixed[].value'),
             'if score MergeBadSource parity matches 0 unless data storage ferraloom:merge_bad_target missing if data storage ferraloom:merge_bad_target {object:{keep:1},scalar:7,numbers:[I;3,4]} if data storage ferraloom:merge_bad_source {mixed:[{value:{added:2}},{value:7}]}',
             expected_last_feedback="Expected an object: got 7"),
    Scenario("data.modify_merge_partial_failure", ("players.commands",),
             ('data merge storage ferraloom:merge_partial {rows:[{value:{keep:1}},{value:7},{}],keep:9}',
              'execute store success score MergePartial parity run data modify storage ferraloom:merge_partial rows[].value merge value {added:2}'),
             'if score MergePartial parity matches 0 if data storage ferraloom:merge_partial rows[2].value if data storage ferraloom:merge_partial {rows:[{value:{keep:1,added:2}},{value:7},{value:{}}],keep:9}',
             expected_last_feedback="Expected an object: got 7"),
    Scenario("data.modify_merge_primitive_parent", ("players.commands",),
             ('data merge storage ferraloom:merge_array {numbers:[I;3,4]}',
              'execute store success score MergeArrayParent parity run data modify storage ferraloom:merge_array numbers[].child merge value {added:2}'),
             'if score MergeArrayParent parity matches 0 if data storage ferraloom:merge_array {numbers:[I;3,4]}',
             expected_last_feedback="Nothing changed. The specified properties already have these values"),
    Scenario("data.storage_namespaces", ("players.commands",),
             ('data merge storage ferraloom:persist {nested:{answer:42},values:[I;3,4],phase:1}',
              'data merge storage ferraloom_other:path/probe {text:"other namespace",phase:1}',
              'data merge storage .:dot {answer:73}',
              'data modify storage command_storage_minecraft.dat:probe value set value 47',
              'data modify storage ferraloom:removed value set value 9',
              'execute store result score StorageRemoved parity run data remove storage ferraloom:removed value'),
             'if score StorageRemoved parity matches 1 if data storage ferraloom:persist {nested:{answer:42},values:[I;3,4],phase:1} if data storage ferraloom_other:path/probe {text:"other namespace",phase:1} if data storage .:dot {answer:73} if data storage command_storage_minecraft.dat:probe {value:47} unless data storage ferraloom:removed value'),
    Scenario("data.storage_namespace_rejection", ("players.commands",),
             ('scoreboard players set NamespaceEscape parity 99',
              'execute store success score NamespaceEscape parity run data modify storage ..:unsafe value set value 1'),
             'if score NamespaceEscape parity matches 99 if data storage .:dot {answer:73} if data storage ferraloom:persist {nested:{answer:42},phase:1}',
             expected_last_feedback="Invalid ID"),
    Scenario("data.modify_providers", ("players.commands",),
             ("setblock 4 230 4 chest", "setblock 6 229 6 stone",
              'summon pig 6 230 6 {Tags:["ferraloom_data_source"],NoAI:1b,NoGravity:1b,Health:7.0f}',
              "data modify storage ferraloom:modify_source answer set value 42",
              'data modify storage ferraloom:modify_source text set value "abcdef"',
              "data modify storage ferraloom:modify_destination block_x set from block 4 230 4 x",
              "data modify storage ferraloom:modify_destination entity_health set from entity @e[type=pig,tag=ferraloom_data_source,limit=1] Health",
              "data modify storage ferraloom:modify_destination answer set from storage ferraloom:modify_source answer"),
             'if data storage ferraloom:modify_destination {block_x:4,entity_health:7.0f,answer:42} if data storage ferraloom:modify_source {answer:42,text:"abcdef"} if block 4 230 4 chest if entity @e[type=pig,tag=ferraloom_data_source,nbt={Health:7.0f},limit=1]'),
    Scenario("command.data_conditions", ("players.commands",),
             tuple(command for label, provider, path in DATA_CONDITION_PROVIDERS for command in (
                 f"scoreboard players set Missing{label} parity 0",
                 f"execute unless data {provider} missing.nested run scoreboard players set Missing{label} parity 1",
                 f"execute if data {provider} missing.nested run scoreboard players set Missing{label} parity 99",
                 f"scoreboard players set Present{label} parity 0",
                 f"execute if data {provider} {path} run scoreboard players set Present{label} parity 1",
                 f"execute unless data {provider} {path} run scoreboard players set Present{label} parity 99")),
             " ".join(f"if score {kind}{label} parity matches 1" for label, _, _ in DATA_CONDITION_PROVIDERS for kind in ("Missing", "Present"))),
    Scenario("data.modify_insert_string", ("players.commands",),
             ("data modify storage ferraloom:modify_destination values set value [10,20]",
              "data modify storage ferraloom:modify_destination values insert 1 value 15",
              "data modify storage ferraloom:modify_destination values insert -1 from storage ferraloom:modify_source answer",
              "data modify storage ferraloom:modify_destination values insert 0 from block 4 230 4 x",
              "data modify storage ferraloom:modify_destination entity_values set value []",
              "data modify storage ferraloom:modify_destination entity_values insert 0 from entity @e[type=pig,tag=ferraloom_data_source,limit=1] Health",
              "data modify storage ferraloom:modify_destination storage_text set string storage ferraloom:modify_source text 1 4",
              "data modify storage ferraloom:modify_destination block_text set string block 4 230 4 id 10",
              "data modify storage ferraloom:modify_destination entity_text set string entity @e[type=pig,tag=ferraloom_data_source,limit=1] Tags[0] 0 9",
              "execute store result score DataLength parity run data get storage ferraloom:modify_destination values",
              *(f"execute store result score DataIndex{i} parity run data get storage ferraloom:modify_destination values[{i}]" for i in range(5)),
              "execute store success score DataNoChange parity run data modify storage ferraloom:modify_destination answer set from storage ferraloom:modify_source answer",
              "execute store success score DataBadIndex parity run data modify storage ferraloom:modify_destination values insert 99 value 7",
              "kill @e[tag=ferraloom_data_source]"),
             'if data storage ferraloom:modify_destination {entity_values:[7.0f],storage_text:"bcd",block_text:"chest",entity_text:"ferraloom"} if score DataLength parity matches 5 if score DataIndex0 parity matches 4 if score DataIndex1 parity matches 10 if score DataIndex2 parity matches 15 if score DataIndex3 parity matches 20 if score DataIndex4 parity matches 42 if score DataNoChange parity matches 0 if score DataBadIndex parity matches 0 unless entity @e[tag=ferraloom_data_source]'),
    Scenario("entity.summon", ("entities.spawning",), ('summon pig 8 201 8 {Tags:["ferraloom_probe"],NoAI:1b}',), "if entity @e[type=pig,tag=ferraloom_probe,limit=1]"),
    Scenario("entity.damage", ("entities.combat",), ("damage @e[type=pig,tag=ferraloom_probe,limit=1] 5 generic",), "if entity @e[type=pig,tag=ferraloom_probe,nbt={Health:5.0f},limit=1]"),
    Scenario("entity.kill", ("entities.lifecycle",), ("kill @e[tag=ferraloom_probe]",), "unless entity @e[tag=ferraloom_probe]", 1.5, "if entity @e[tag=ferraloom_probe]"),
    Scenario("entity.dying_selector", ("players.commands",),
             ('summon pig 8 201 10 {Tags:["ferraloom_dying_selector"],NoAI:1b}',
              "execute store result score DyingKilled parity run kill @e[type=pig,tag=ferraloom_dying_selector]"),
             "if score DyingKilled parity matches 1 unless entity @e[type=pig,tag=ferraloom_dying_selector]", settle=0),
    Scenario("command.entity_count", ("players.commands",), ('summon pig 14 200 12 {Tags:["ferraloom_count_probe"],NoAI:1b}', 'summon pig 15 200 12 {Tags:["ferraloom_count_probe"],NoAI:1b}', "scoreboard players set EntityCopies parity 0", "execute as @e[type=pig,tag=ferraloom_count_probe] run scoreboard players add EntityCopies parity 1", "kill @e[tag=ferraloom_count_probe]"), "if score EntityCopies parity matches 2"),
    Scenario("world.forceload_query", ("world.chunk_tickets",), ("execute store result score ForceListBefore parity store success score ForceListOk parity run forceload query", "execute store result score ForceQuery parity store success score ForceQueryOk parity run forceload query 0 0", "execute store success score ForceQueryMissing parity run forceload query 160000 160000", "execute in minecraft:the_end store result score ForceEmpty parity store success score ForceEmptyOk parity run forceload query", "execute store result score ForceListAfter parity run forceload query"), "if score ForceListBefore parity = ForceListAfter parity if score ForceListOk parity matches 1 if score ForceQuery parity matches 1 if score ForceQueryOk parity matches 1 if score ForceQueryMissing parity matches 0 if score ForceEmpty parity matches 0 if score ForceEmptyOk parity matches 1"),
    Scenario("world.forceload_counts", ("world.chunk_tickets",), ("execute store result score ForceFirst parity store success score ForceFirstOk parity run forceload add 96 0", "execute store result score ForceMore parity run forceload add 96 0 112 0", "execute store success score ForceDuplicate parity run forceload add 96 0 112 0", "execute store result score ForceRemoved parity run forceload remove 96 0 112 0", "execute store success score ForceMissing parity run forceload remove 96 0 112 0"), "if score ForceFirst parity matches 1 if score ForceFirstOk parity matches 1 if score ForceMore parity matches 1 if score ForceDuplicate parity matches 0 if score ForceRemoved parity matches 2 if score ForceMissing parity matches 0"),
    Scenario("datapack.function", ("world.functions",), ("function ferraloom:probe",), "if score Datapack parity matches 37"),
    Scenario("dimension.nether", ("world.nether",), ("execute in minecraft:the_nether run setblock 0 100 0 gold_block",), "in minecraft:the_nether if block 0 100 0 gold_block", 0.5, force_dimension="minecraft:the_nether"),
    Scenario("block.fill_destroy_nether_sponge", ("players.commands",),
             ("execute in minecraft:the_nether run setblock 1 225 1 stone",
              "execute in minecraft:the_nether store result score FillNetherSponge parity run fill 1 225 1 1 225 1 wet_sponge destroy"),
             "if score FillNetherSponge parity matches 1 in minecraft:the_nether if block 1 225 1 sponge"),
    Scenario("scoreboard.global_dimension", ("players.scoreboards",), ("execute in minecraft:the_nether run scoreboard players set DimensionScore parity 41",), "if score DimensionScore parity matches 41"),
    Scenario("dimension.end", ("world.end",), ("execute in minecraft:the_end run setblock 0 100 0 emerald_block",), "in minecraft:the_end if block 0 100 0 emerald_block", 0.5, force_dimension="minecraft:the_end"),
    Scenario("persistence.entity_write", ("persistence.entity_data",), ('summon pig 8 200 12 {Tags:["ferraloom_saved"],UUID:[I;111,222,333,444],NoAI:1b,PersistenceRequired:1b}', "damage @e[type=pig,tag=ferraloom_saved,limit=1] 3 generic"), 'if entity @e[type=pig,tag=ferraloom_saved,nbt={UUID:[I;111,222,333,444],Health:7.0f}]'),
    Scenario("command.selector_scores", ("players.commands",), ("scoreboard objectives add selector_probe dummy", "scoreboard objectives add selector_unset dummy", "scoreboard players set @e[type=pig,tag=ferraloom_saved,limit=1] selector_probe -7"), "if entity @e[type=pig,tag=ferraloom_saved,scores={selector_probe=-7},limit=1] unless entity @e[type=pig,tag=ferraloom_saved,scores={selector_unset=0}] unless entity @e[type=pig,tag=ferraloom_saved,scores={selector_absent=0}]"),
    Scenario("scoreboard.team", ("players.teams",), ("team add ferraloom", "team join ferraloom @e[type=pig,tag=ferraloom_saved,limit=1]", "team modify ferraloom color gold", "team modify ferraloom friendlyFire false", "team modify ferraloom nametagVisibility never", "team modify ferraloom collisionRule pushOtherTeams"), "if entity @e[type=pig,tag=ferraloom_saved,team=ferraloom,limit=1]", precondition="unless entity @e[type=pig,tag=ferraloom_saved,team=ferraloom]"),
    Scenario("persistence.write", ("persistence.chunk_data",), ("setblock 14 200 14 diamond_block", "save-all flush"), "if block 14 200 14 diamond_block", 0.5),
]


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def pin_binary(source: Path, run_dir: Path) -> Path:
    """All subprocesses use this copy even if Cargo replaces the source binary."""
    pinned = run_dir / "bin/ferraloom"
    pinned.parent.mkdir()
    shutil.copy2(source.resolve(), pinned)
    return pinned


def datapack(directory: Path) -> None:
    pack = directory / "world/datapacks/ferraloom-parity"
    functions = pack / "data/ferraloom/function"
    functions.mkdir(parents=True)
    (pack / "pack.mcmeta").write_text(json.dumps({"pack": {"description": "Ferraloom compatibility fixture", "pack_format": 121, "min_format": 121, "max_format": 121}}))
    (functions / "probe.mcfunction").write_text("scoreboard players set Datapack parity 37\n")


class Server:
    def __init__(self, kind: str, directory: Path, args: argparse.Namespace):
        self.kind, self.directory, self.args = kind, directory, args
        self.port = free_port()
        self.process: subprocess.Popen[str] | None = None
        self.lines: queue.Queue[str] = queue.Queue()
        self.log_path = directory / "console.log"
        self.forced_stop = False
        self.reader: threading.Thread | None = None

    def configure(self) -> None:
        self.directory.mkdir(parents=True)
        datapack(self.directory)
        if self.kind == "vanilla":
            (self.directory / "eula.txt").write_text("# Explicit local test acceptance supplied via --accept-eula\neula=true\n")
            (self.directory / "server.properties").write_text(
                f"server-ip=127.0.0.1\nserver-port={self.port}\nonline-mode=false\nenforce-secure-profile=false\n"
                "level-seed=12345678\nview-distance=2\nsimulation-distance=2\nmax-players=20\n"
                "spawn-protection=0\nnetwork-compression-threshold=256\nmax-tick-time=60000\n"
                "white-list=false\nallow-flight=true\npause-when-empty-seconds=-1\n"
                "motd=Ferraloom compatibility reference\nenable-rcon=false\n")
        else:
            (self.directory / "pumpkin.toml").write_text(
                'seed = "12345678"\nspawn_protection = 0\n'
                f'[networking.java]\naddress = "127.0.0.1:{self.port}"\nonline_mode = false\nencryption = false\n'
                'view_distance = 2\nsimulation_distance = 2\nmax_players = 20\n'
                '[networking.bedrock]\nenabled = false\n[networking.lan_broadcast]\nenabled = false\n'
                '[commands]\nuse_tty = false\n[logging]\ncolor = false\nfile = ""\ntimestamp = false\n'
                '[telemetry]\nenabled = false\n[plugins]\nenabled = false\n')
            with (self.directory / "pumpkin.toml").open("a") as config:
                config.write('[world.chunk]\ntype = "anvil"\n[world.chunk.compression]\nalgorithm = "ZLib"\nlevel = 6\n')

    def start(self) -> None:
        self.forced_stop = False
        if self.kind == "vanilla":
            command = java_command(self.args.java) + ["-jar", str(acquire()), "nogui"]
        else:
            command = [str(self.args.rust_binary.resolve()), "--data-dir", str(self.directory),
                       "--bind", f"127.0.0.1:{self.port}", "--offline", "--workers", str(self.args.workers)]
        self.lines = queue.Queue()
        self.process = subprocess.Popen(command, cwd=self.directory, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                        bufsize=1, start_new_session=True)
        process = self.process
        lines = self.lines

        def read() -> None:
            assert process.stdout is not None
            with self.log_path.open("a") as log:
                for raw in process.stdout:
                    line = ANSI.sub("", raw.rstrip())
                    log.write(line + "\n")
                    log.flush()
                    lines.put(line)

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        deadline = time.monotonic() + self.args.startup_timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"{self.kind} exited {process.returncode}; see {self.log_path}")
            try:
                protocol.status_fields(self.port)
                self.command("list")
                return
            except (OSError, EOFError, ValueError, AssertionError):
                time.sleep(0.2)
        raise TimeoutError(f"{self.kind} startup exceeded {self.args.startup_timeout}s; see {self.log_path}")

    def command(self, command: str, timeout: float = 15) -> list[str]:
        assert self.process is not None and self.process.stdin is not None
        token = f"FERRALOOM_END_{uuid.uuid4().hex}"
        self.process.stdin.write(command + "\nsay " + token + "\n")
        self.process.stdin.flush()
        lines: list[str] = []
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"Server exited {self.process.returncode} while executing {command}")
            try:
                line = self.lines.get(timeout=min(0.25, max(0.001, deadline - time.monotonic())))
            except queue.Empty:
                continue
            lines.append(line)
            if is_server_say(line, token):
                return lines
        raise TimeoutError(f"Command timed out: {command}")

    def stop(self) -> int | None:
        process = self.process
        if process is None:
            return None
        if process.poll() is None:
            try:
                assert process.stdin is not None
                process.stdin.write("stop\n")
                process.stdin.flush()
                process.wait(timeout=45)
            except (OSError, subprocess.TimeoutExpired):
                self.forced_stop = True
                self.signal(signal.SIGTERM)
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    self.signal(signal.SIGKILL)
                    process.wait(timeout=5)
        if self.reader is not None:
            self.reader.join(timeout=2)
        return process.returncode

    def signal(self, signum: int) -> None:
        if self.process is not None:
            try:
                os.killpg(self.process.pid, signum)
            except ProcessLookupError:
                pass

    def save_flush(self) -> None:
        """Wait for completion feedback; Rust's command schedules an async save."""
        lines = self.command("save-all flush")
        deadline = time.monotonic() + 30
        while not any(re.search(r"\bSaved the game$", line) for line in lines):
            if time.monotonic() >= deadline:
                raise TimeoutError("save-all flush did not report completion")
            try:
                lines.append(self.lines.get(timeout=0.2))
            except queue.Empty:
                continue


def recorded(test_id: str, features: tuple[str, ...], action) -> dict:
    started = time.monotonic()
    try:
        detail = action()
        result = {"result": "failed" if isinstance(detail, dict) and detail.get("result") == "failed" else "passed", "detail": detail}
    except Exception as error:
        result = {"result": "failed", "error": f"{type(error).__name__}: {error}"}
    result.update({"id": test_id, "features": list(features), "duration_seconds": round(time.monotonic() - started, 3)})
    print(f"  {test_id}: {result['result']}", flush=True)
    return result


def run_scenario(server: Server, scenario: Scenario) -> dict:
    if scenario.force_dimension is not None:
        server.command(f"execute in {scenario.force_dimension} run forceload add 0 0")
        wait_loaded(server, scenario.force_dimension)
    if scenario.precondition is not None:
        check_predicate(server, scenario.precondition)
    last_output = []
    for command in scenario.commands:
        last_output = server.command(command)
    if scenario.expected_last_feedback is not None:
        protocol.require(any(line.endswith(scenario.expected_last_feedback) for line in last_output),
                         f"Missing final command feedback {scenario.expected_last_feedback!r}: {last_output[-6:]}")
    time.sleep(scenario.settle)
    check_predicate(server, scenario.predicate)
    detail = {"predicate": scenario.predicate, "commands": list(scenario.commands)}
    if scenario.expected_last_feedback is not None:
        detail.update(expected_last_feedback=scenario.expected_last_feedback, last_command_output=last_output)
    return detail


def stop_by_signal(server: Server, timeout: float = 45) -> dict:
    protocol.require(server.process is not None, "Missing server process")
    server.signal(signal.SIGTERM)
    result = recorded("lifecycle.sigterm_exit", ("lifecycle.shutdown",),
                      lambda: check_signal_exit(server.kind, server.process.wait(timeout=timeout), server.forced_stop))
    if result["result"] != "passed":
        # A console stop would save the marker and falsely credit SIGTERM.
        server.forced_stop = True
        server.signal(signal.SIGKILL)
        server.process.wait(timeout=5)
    server.stop()
    return result


def check_predicate(server: Server, predicate: str) -> None:
    token = "FERRALOOM_PASS_" + uuid.uuid4().hex
    output = server.command(f"execute {predicate} run say {token}")
    if not any(is_server_say(line, token) for line in output):
        raise AssertionError(f"Predicate failed: {predicate}; output: {output[-6:]}")


def wait_loaded(server: Server, dimension: str = "minecraft:overworld", *, x: int = 0, z: int = 0) -> None:
    deadline = time.monotonic() + 60
    while True:
        try:
            check_predicate(server, f"in {dimension} if loaded {x} 100 {z}")
            return
        except AssertionError:
            if time.monotonic() >= deadline:
                raise AssertionError(f"Forced chunk never loaded: {dimension} {x} {z}") from None
            time.sleep(0.2)


def run(server: Server, quick: bool) -> list[dict]:
    results: list[dict] = []
    try:
        server.configure()
        if not quick:
            results.append(recorded("blocks.registry_tables", ("blocks.registry",), lambda: registry_tables(server)))
        server.start()
        if server.args.rust_binary.is_file():
            results.append(recorded("lifecycle.world_lock", ("persistence.level_data",), lambda: world_lock(server)))
        for name, feature, action in [
            ("network.status_fields", "network.status", protocol.status_fields),
            ("network.status_other_version", "network.status", protocol.status_other_version),
            ("network.incompatible_login", "network.handshake", protocol.incompatible_login),
            ("network.ping_before_status", "network.status", protocol.ping),
            ("network.duplicate_status", "network.status", protocol.duplicate_status),
            ("network.fragmented", "network.framing", protocol.fragmented_status),
            ("network.concurrent_status", "network.framing", protocol.concurrent_status),
            ("network.malformed_length", "network.framing", protocol.malformed_length),
        ]:
            results.append(recorded(name, (feature,), lambda action=action: action(server.port)))
        if not quick:
            server.command("forceload add -16 -16 31 31", timeout=60)
            wait_loaded(server)
            wait_loaded(server, x=-1, z=9)
            server.save_flush()
            terrain = server.directory.parent / "terrain" / server.kind
            terrain.mkdir(parents=True)
            shutil.copy2(server.directory / "world/level.dat", terrain / "level.dat")
            for relative in ("region", "dimensions/minecraft/overworld/region"):
                source = server.directory / "world" / relative
                if source.is_dir():
                    shutil.copytree(source, terrain / relative)
            server.command("fill -2 199 -2 15 199 15 stone")
            results.extend(run_client_scenarios(server))
            for scenario in SCENARIOS:
                results.append(recorded(scenario.id, scenario.features, lambda scenario=scenario: run_scenario(server, scenario)))
            for block, destroy in (("repeater", False), ("repeater", True), ("comparator", False)):
                mode = "fill_destroy" if destroy else "setblock"
                results.append(recorded(f"redstone.{block}_remove_{mode}", ("blocks.redstone_logic",),
                                        lambda block=block, destroy=destroy: gate_removal(server, block, destroy)))
            for remove_hook in (False, True):
                component = "hook" if remove_hook else "string"
                results.append(recorded(f"redstone.tripwire_remove_{component}", ("blocks.redstone_logic",),
                                        lambda remove_hook=remove_hook: tripwire_removal(server, remove_hook)))
            results.append(recorded("blocks.fluid_tick_ring", ("blocks.fluids", "world.chunk_tickets"), lambda: fluid_tick_ring(server)))
            results.append(recorded("world.chunk_frozen_load", ("world.chunk_tickets",), lambda: frozen_chunk_load(server)))
            server.command("gamerule keep_inventory true")
            server.command("gamerule advance_time false")
            server.command("time set 4321")
            server.command("save-off")
            unflushed = Scenario("persistence.unflushed_write", ("persistence.chunk_data",), ("setblock 13 200 14 emerald_block",), "if block 13 200 14 emerald_block")
            results.append(recorded(unflushed.id, unflushed.features, lambda: run_scenario(server, unflushed)))
            server.command("data modify storage ferraloom:persist phase set value 2")
            server.command("data modify storage ferraloom_other:path/probe phase set value 2")
            results.append(recorded("persistence.command_storage_manual", ("persistence.map_data",), lambda: command_storage_state(server, 2)))
            server.command("data modify storage ferraloom:persist phase set value 3")
            server.command("data modify storage ferraloom_other:path/probe phase set value 3")
            exit_code = server.stop()
            results.append(recorded("lifecycle.clean_shutdown", ("lifecycle.shutdown",), lambda: check_exit(exit_code, server.forced_stop)))
            server.start()
            server.command("forceload add 0 0")
            wait_loaded(server)
            persisted = Scenario("persistence.restart", ("persistence.chunk_data",), (), "if block 14 200 14 diamond_block")
            results.append(recorded(persisted.id, persisted.features, lambda: run_scenario(server, persisted)))
            unflushed_read = Scenario("persistence.shutdown_restart", ("persistence.chunk_data",), (), "if block 13 200 14 emerald_block")
            results.append(recorded(unflushed_read.id, unflushed_read.features, lambda: run_scenario(server, unflushed_read)))
            results.append(recorded("persistence.metadata_restart", ("persistence.level_data",), lambda: check_metadata(server)))
            results.append(recorded("persistence.scoreboard_restart", ("persistence.scoreboard_data",), lambda: check_predicate(server, "if score A parity matches 12 if score B parity matches 5 if score Query parity matches -7")))
            results.append(recorded("persistence.entity_restart", ("persistence.entity_data",), lambda: wait_saved_entity(server, 7)))
            results.append(recorded("persistence.scoreboard_team_restart", ("persistence.scoreboard_data",), lambda: check_team_state(server)))
            results.append(recorded("persistence.scoreboard_display_restart", ("persistence.scoreboard_data",), lambda: check_display_state(server)))
            results.append(recorded("persistence.command_storage_restart", ("persistence.map_data",), lambda: command_storage_state(server, 3)))
            server.command("data modify storage ferraloom:persist phase set value 4")
            server.command("data modify storage ferraloom_other:path/probe phase set value 4")
            entity_signal_write = Scenario("persistence.entity_signal_write", ("persistence.entity_data",), ("damage @e[type=pig,tag=ferraloom_saved,limit=1] 2 generic",), 'if entity @e[type=pig,tag=ferraloom_saved,nbt={UUID:[I;111,222,333,444],Health:5.0f}]')
            results.append(recorded(entity_signal_write.id, entity_signal_write.features, lambda: run_scenario(server, entity_signal_write)))
            signal_write = Scenario("persistence.signal_write", ("persistence.chunk_data",), ("setblock 12 200 14 lapis_block",), "if block 12 200 14 lapis_block")
            results.append(recorded(signal_write.id, signal_write.features, lambda: run_scenario(server, signal_write)))
            signal_result = stop_by_signal(server)
            results.append(signal_result)
            signal_read = Scenario("persistence.signal_restart", ("persistence.chunk_data",), (), "if block 12 200 14 lapis_block")
            if signal_result["result"] == "passed":
                server.start()
                server.command("forceload add 0 0")
                wait_loaded(server)
                results.append(recorded(signal_read.id, signal_read.features, lambda: run_scenario(server, signal_read)))
                results.append(recorded("persistence.entity_signal_restart", ("persistence.entity_data",), lambda: wait_saved_entity(server, 5)))
                results.append(recorded("persistence.command_storage_signal_restart", ("persistence.map_data",), lambda: command_storage_state(server, 4)))
            else:
                results.append({"id": signal_read.id, "features": list(signal_read.features), "result": "skipped",
                                "reason": "SIGTERM did not complete cleanly; no shutdown durability claim"})
                results.append({"id": "persistence.entity_signal_restart", "features": ["persistence.entity_data"], "result": "skipped",
                                "reason": "SIGTERM did not complete cleanly; no entity durability claim"})
                results.append({"id": "persistence.command_storage_signal_restart", "features": ["persistence.map_data"], "result": "skipped",
                                "reason": "SIGTERM did not complete cleanly; no command storage durability claim"})
    except Exception as error:
        results.append({"id": "harness.startup_or_setup", "features": [], "result": "failed", "error": f"{type(error).__name__}: {error}"})
    finally:
        server.stop()
    return results


def world_lock(server: Server) -> dict:
    """Ferraloom must respect both vanilla and its own live world record locks."""
    contender = subprocess.run([str(server.args.rust_binary.resolve()), "--data-dir", str(server.directory),
                               "--bind", f"127.0.0.1:{free_port()}", "--offline", "--workers", "1"],
                              cwd=server.directory, capture_output=True, text=True, timeout=20)
    output = contender.stdout + contender.stderr
    protocol.require(contender.returncode != 0 and "already locked" in output, f"Concurrent server did not reject the held world: {contender.returncode}; {output[-600:]}")
    return {"holder": server.kind, "contender": "rust", "world_lock_rejected": True}


def registry_tables(server: Server) -> dict:
    reference = json.loads((CACHE / "generated/reports/blocks.json").read_text())
    facts = {"blocks": len(reference), "states": sum(len(block["states"]) for block in reference.values()),
             "scope": "IDs, defaults and all property tuples; mechanics excluded"}
    if server.kind == "rust":
        passed = compare_registry(server.directory / "rust-registry.json", server.directory / "registry-comparison.json", server.args.rust_binary)
        protocol.require(passed, "Runtime registry differs from official block tables")
    return facts


def check_exit(code: int | None, forced: bool) -> dict:
    protocol.require(code == 0 and not forced, f"Unclean shutdown: exit={code}, forced={forced}")
    return {"exit_code": code}


def check_signal_exit(kind: str, code: int | None, forced: bool) -> dict:
    # HotSpot completes its shutdown hooks and reports 128 + SIGTERM on Unix.
    accepted = (0, 128 + signal.SIGTERM) if kind == "vanilla" else (0,)
    protocol.require(code in accepted and not forced, f"Unclean SIGTERM shutdown: exit={code}, forced={forced}")
    return {"exit_code": code, "signal": "SIGTERM", "durability": "checked separately by persistence.signal_restart"}


def game_time(server: Server) -> int:
    lines = server.command("time query gametime")
    values = [match.group(1) for line in lines
              if (match := re.search(r"The game time is (-?\d+) tick\(s\)$", line))]
    protocol.require(len(values) == 1, f"Missing or ambiguous game time: {lines[-6:]}")
    return int(values[0])


def step_ticks(server: Server, count: int) -> None:
    """Advance an already frozen reference by exactly the requested game ticks."""
    before = game_time(server)
    feedback = server.command(f"tick step {count}")
    protocol.require(any(line.endswith(f"Stepping {count} tick(s)") for line in feedback), "Tick step was not acknowledged")
    deadline = time.monotonic() + 15
    after = game_time(server)
    while after < before + count and time.monotonic() < deadline:
        time.sleep(0.1)
        after = game_time(server)
    protocol.require(after == before + count, f"Expected exactly {count} stepped ticks, got {after - before}")
    query = server.command("tick query")
    protocol.require(any(line.endswith("The game is frozen") for line in query), "Tick stepping did not remain frozen")


def gate_removal(server: Server, block: str, destroy: bool) -> dict:
    """Observe loss of indirect power beyond the gate's immediate neighbors."""
    frozen = server.command("tick freeze")
    try:
        protocol.require(any(line.endswith("The game is frozen") for line in frozen), "Freeze was not acknowledged")
        setup = ("fill -1 240 7 4 242 9 air", "fill -1 239 7 4 239 9 stone",
                 "setblock 3 240 8 redstone_lamp", "setblock 2 240 8 stone",
                 f"setblock 1 240 8 {block}[facing=west]", "setblock 0 240 8 redstone_block")
        for command in setup:
            server.command(command)
        step_ticks(server, 10)
        precondition = f"if block 1 240 8 {block}[facing=west,powered=true] if block 3 240 8 redstone_lamp[lit=true]"
        check_predicate(server, precondition)
        replacement = "gold_block" if destroy else "air"
        command = "fill 1 240 8 1 240 8 gold_block destroy" if destroy else "setblock 1 240 8 air"
        server.command(f"execute store result score GateRemoved parity run {command}")
        step_ticks(server, 10)
        predicate = f"if score GateRemoved parity matches 1 if block 1 240 8 {replacement} if block 3 240 8 redstone_lamp[lit=false]"
        check_predicate(server, predicate)
        return {"setup": list(setup), "precondition": precondition, "command": command, "predicate": predicate,
                "setup_ticks": 10, "after_removal_ticks": 10}
    finally:
        server.command("tick unfreeze")


def tripwire_removal(server: Server, remove_hook: bool) -> dict:
    """Require a connected line before removing string or one of its hooks."""
    frozen = server.command("tick freeze")
    try:
        protocol.require(any(line.endswith("The game is frozen") for line in frozen), "Freeze was not acknowledged")
        setup = ("fill -1 240 7 5 242 9 air", "fill -1 239 7 5 239 9 stone",
                 "setblock 0 240 8 stone", "setblock 4 240 8 stone",
                 "setblock 1 240 8 tripwire_hook[facing=east]", "setblock 3 240 8 tripwire_hook[facing=west]",
                 "setblock 2 240 8 tripwire")
        for command in setup:
            server.command(command)
        step_ticks(server, 20)
        precondition = "if block 1 240 8 tripwire_hook[facing=east,attached=true,powered=false] if block 3 240 8 tripwire_hook[facing=west,attached=true,powered=false] if block 2 240 8 tripwire[attached=true]"
        check_predicate(server, precondition)
        removed_x = 1 if remove_hook else 2
        command = f"setblock {removed_x} 240 8 air"
        server.command(f"execute store result score TripwireRemoved parity run {command}")
        removal_predicate = None
        if not remove_hook:
            removal_predicate = "if block 1 240 8 tripwire_hook[attached=true,powered=true] if block 3 240 8 tripwire_hook[attached=true,powered=true]"
            check_predicate(server, removal_predicate)
        step_ticks(server, 20)
        predicate = f"if score TripwireRemoved parity matches 1 if block {removed_x} 240 8 air if block 3 240 8 tripwire_hook[attached=false,powered=false]"
        if not remove_hook:
            predicate += " if block 1 240 8 tripwire_hook[attached=false,powered=false]"
        check_predicate(server, predicate)
        return {"setup": list(setup), "precondition": precondition, "command": command, "predicate": predicate,
                "removal_predicate": removal_predicate, "setup_ticks": 20, "after_removal_ticks": 20}
    finally:
        server.command("tick unfreeze")


def command_storage_state(server: Server, phase: int) -> dict:
    """Check live namespaced contents and vanilla-format files after an explicit save."""
    predicate = (f'if data storage ferraloom:persist {{nested:{{answer:42}},values:[I;3,4],phase:{phase}}} '
                 f'if data storage ferraloom_other:path/probe {{text:"other namespace",phase:{phase}}} '
                 'if data storage .:dot {answer:73} '
                 'if data storage command_storage_minecraft.dat:probe {value:47} '
                 'unless data storage ferraloom:removed value')
    check_predicate(server, predicate)
    server.save_flush()
    namespaces = {}
    for namespace, key, expected in (("ferraloom", "persist", {"nested": {"answer": 42}, "values": (3, 4), "phase": phase}),
                                     ("ferraloom_other", "path/probe", {"text": "other namespace", "phase": phase}),
                                     (".", "dot", {"answer": 73}),
                                     ("command_storage_minecraft.dat", "probe", {"value": 47})):
        path = server.directory / "world/data" / namespace / "command_storage.dat"
        saved = decode_nbt(gzip.decompress(path.read_bytes()))
        protocol.require(saved.get("DataVersion") == 5023, f"Wrong command storage data version: {saved}")
        contents = saved["data"]["contents"]
        protocol.require(contents.get(key) == expected, f"Saved command storage differs for {namespace}:{key}: {contents.get(key)}")
        protocol.require("removed" not in contents, "Empty command storage key was retained")
        namespaces[namespace] = {"key": key, "contents": expected, "empty_key_absent": True}
    protocol.require((server.directory / "world/data/minecraft").is_dir(), "Missing vanilla namespace directory for the legacy filename collision check")
    return {"phase": phase, "predicate": predicate, "namespaces": namespaces, "data_version": 5023}


def fluid_tick_ring(server: Server) -> dict:
    """A forced chunk's neighbor block-ticks without becoming entity-ticking."""
    server.command("forceload add 96 0", timeout=60)
    wait_loaded(server, x=96, z=0)
    frozen = server.command("tick freeze")
    try:
        protocol.require(any(line.endswith("The game is frozen") for line in frozen), "Freeze was not acknowledged")
        check_predicate(server, "unless loaded 114 200 8")
        for command in ("fill 110 200 7 119 201 9 air", "fill 110 199 7 119 199 9 stone",
                        "fill 110 200 7 119 200 7 stone", "fill 110 200 9 119 200 9 stone",
                        "setblock 110 200 8 stone", "setblock 119 200 8 stone", "setblock 111 200 8 water"):
            server.command(command)
        check_predicate(server, "if block 111 200 8 water[level=0] if block 112 200 8 air if block 114 200 8 air")
        before = game_time(server)
        feedback = server.command("tick step 40")
        protocol.require(any(line.endswith("Stepping 40 tick(s)") for line in feedback), "Tick step was not acknowledged")
        deadline = time.monotonic() + 15
        after = game_time(server)
        while after < before + 40 and time.monotonic() < deadline:
            time.sleep(0.1)
            after = game_time(server)
        protocol.require(after == before + 40, f"Expected exactly 40 stepped ticks, got {after - before}")
        query = server.command("tick query")
        protocol.require(any(line.endswith("The game is frozen") for line in query), "Tick stepping did not remain frozen")
        check_predicate(server, "if block 111 200 8 water[level=0] if block 114 200 8 water[level=3] unless loaded 114 200 8")
        return {"forced_chunk": {"x": 6, "z": 0}, "target_chunk": {"x": 7, "z": 0},
                "stepped_ticks": after - before, "source": [111, 200, 8], "target": [114, 200, 8],
                "target_water_level": 3, "target_entity_ticking": False}
    finally:
        server.command("forceload remove 96 0")
        server.command("tick unfreeze")


def frozen_chunk_load(server: Server) -> dict:
    """Generation and chunk activation continue while simulation is paused."""
    frozen = server.command("tick freeze")
    try:
        protocol.require(any(line.endswith("The game is frozen") for line in frozen),
                         f"Server did not acknowledge the freeze: {frozen[-6:]}")
        server.command("forceload add 64 64", timeout=60)
        wait_loaded(server, x=64, z=64)
        server.command("setblock 64 200 64 gold_block")
        check_predicate(server, "if loaded 64 200 64 if block 64 200 64 gold_block")
        query = server.command("tick query")
        protocol.require(any(line.endswith("The game is frozen") for line in query),
                         f"Simulation resumed during chunk activation: {query[-6:]}")
        return {"chunk": {"x": 4, "z": 4}, "frozen_during_activation": True, "block": "minecraft:gold_block"}
    finally:
        server.command("forceload remove 64 64")
        server.command("tick unfreeze")


def saved_scoreboard(server: Server) -> dict:
    server.save_flush()
    path = server.directory / "world/data/minecraft/scoreboard.dat"
    root = decode_nbt(gzip.decompress(path.read_bytes()))
    protocol.require(root.get("DataVersion") == 5023, "Scoreboard was not saved in the target format")
    return root["data"]


def check_team_state(server: Server) -> dict:
    check_predicate(server, "if entity @e[type=pig,tag=ferraloom_saved,team=ferraloom,limit=1]")
    teams = saved_scoreboard(server).get("Teams", [])
    team = next((team for team in teams if team.get("Name") == "ferraloom"), None)
    protocol.require(team is not None, f"Saved team missing: {teams}")
    expected = {"TeamColor": "gold", "AllowFriendlyFire": 0, "NameTagVisibility": "never",
                "CollisionRule": "pushOtherTeams", "Players": ["0000006f-0000-00de-0000-014d000001bc"]}
    protocol.require(all(team.get(key) == value for key, value in expected.items()),
                     f"Saved team properties differ: {team}")
    return {"team": "ferraloom", "saved_properties": expected,
            "scope": "live membership and saved settings; combat, rendering and collision behavior excluded"}


def check_display_state(server: Server) -> dict:
    feedback = server.command("scoreboard objectives setdisplay sidebar parity")
    protocol.require(any(line.endswith("Nothing changed. That display slot is already showing that objective") for line in feedback),
                     f"Display slot was not restored: {feedback[-6:]}")
    saved = saved_scoreboard(server)
    protocol.require(saved.get("DisplaySlots", {}).get("sidebar") == "parity", "Saved sidebar objective was lost")
    objective = next((objective for objective in saved.get("Objectives", []) if objective.get("Name") == "unlock"), {})
    score = next((score for score in saved.get("PlayerScores", [])
                  if score.get("Name") == "Candidate" and score.get("Objective") == "unlock"), {})
    protocol.require(objective.get("CriteriaName") == "trigger" and score.get("Score") == 13 and not score.get("Locked", False),
                     f"Trigger criterion or unlocked score was lost: {objective}; {score}")
    return {"slot": "sidebar", "objective": "parity", "trigger": "unlock", "score": 13, "locked": False,
            "scope": "restored server display state and saved trigger settings; client rendering excluded"}


def check_metadata(server: Server) -> dict:
    rule = server.command("gamerule keep_inventory")
    protocol.require(any(re.search(r"Game rule keep_inventory is currently set to true$", line) for line in rule),
                     f"Non-default game rule was lost: {rule[-6:]}")
    clock = server.command("time query time")
    protocol.require(any(re.search(r"Clock minecraft:overworld is at 4321 tick\(s\)$", line) for line in clock),
                     f"Non-default clock was lost: {clock[-6:]}")
    return {"keep_inventory": True, "daytime": 4321}


def wait_saved_entity(server: Server, health: int, *, timeout: float = 10) -> dict:
    """Wait for asynchronous entity loading, checking uniqueness as well as NBT."""
    # The counter is test infrastructure, independent of scoreboard persistence.
    objective = "fe" + uuid.uuid4().hex[:12]
    server.command(f"scoreboard objectives add {objective} dummy")
    deadline = time.monotonic() + timeout
    predicate = (f"if score EntityCopies {objective} matches 1 "
                 f"if entity @e[type=pig,tag=ferraloom_saved,nbt={{UUID:[I;111,222,333,444],Health:{health}.0f}}]")
    while True:
        server.command(f"scoreboard players set EntityCopies {objective} 0")
        server.command(f"execute as @e[type=pig,tag=ferraloom_saved] run scoreboard players add EntityCopies {objective} 1")
        try:
            check_predicate(server, predicate)
            return {"count": 1, "uuid": [111, 222, 333, 444], "health": health, "predicate": predicate}
        except AssertionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.2)


def cross_world(source: str, destination: str, run_dir: Path, args: argparse.Namespace, source_cases: list[dict]) -> dict:
    """Load the other implementation's saved overworld without format conversion."""
    test_id = f"persistence.{source}_to_{destination}"
    if not any(case["id"] == "persistence.write" and case["result"] == "passed" for case in source_cases):
        return {"id": test_id, "features": ["persistence.chunk_data", "persistence.level_data"],
                "result": "skipped", "reason": "Source persistence.write did not establish the saved marker"}
    server = Server(destination, run_dir / f"migration-{source}-to-{destination}", args)

    def check() -> dict:
        try:
            source_world = run_dir / source / "world"
            protocol.require((source_world / "level.dat").is_file(), "Source did not save level.dat")
            source_version = data_version(source_world)
            protocol.require(source_version == 5023, f"Source world is not the target format: {source_version}")
            server.configure()
            shutil.copytree(source_world, server.directory / "world", dirs_exist_ok=True)
            server.start()
            server.command("forceload add 0 0")
            wait_loaded(server)
            predicate = "if block 14 200 14 diamond_block"
            check_predicate(server, predicate)
            check_metadata(server)
            check_predicate(server, "if score A parity matches 12 if score B parity matches 5 if score Query parity matches -7")
            wait_saved_entity(server, 5)
            check_team_state(server)
            check_display_state(server)
            command_storage_state(server, 4)
            code = server.stop()
            check_exit(code, server.forced_stop)
            errors = [line for line in server.log_path.read_text().splitlines()
                      if re.search(r"\bERROR\b|Failed to parse saved data|Couldn't load chunk", line)]
            protocol.require(not errors, f"Destination reported world load/save errors: {errors[:5]}")
            destination_version = data_version(server.directory / "world")
            protocol.require(destination_version == source_version, "Loading rewrote the data version")
            return {"source": source, "destination": destination, "predicate": predicate, "source_data_version": source_version,
                    "destination_data_version": destination_version, "external_conversion": False, "dimension": "overworld",
                    "keep_inventory": True, "daytime": 4321, "load_save_errors": 0,
                    "entity_uuid": [111, 222, 333, 444], "entity_health": 5,
                    "scoreboard_scores_teams_and_display": True, "command_storage_namespaces": True}
        finally:
            server.stop()

    return recorded(test_id, ("persistence.chunk_data", "persistence.level_data"), check)


def compare_command_trees(servers: dict) -> dict:
    """Compare advertised syntax, without treating advertisement as implementation."""
    trees = {}
    for kind in ("vanilla", "rust"):
        case = next((case for case in servers.get(kind, []) if case["id"] == "network.command_tree"), None)
        protocol.require(case is not None and case["result"] == "passed", f"Missing valid {kind} command tree")
        trees[kind] = case["detail"]
    roots = {kind: set(tree["roots"]) for kind, tree in trees.items()}
    paths = {kind: {re.sub(r"<[^>]+>", "<argument>", path) for path in tree["literal_paths"]}
             for kind, tree in trees.items()}
    missing_roots = sorted(roots["vanilla"] - roots["rust"])
    missing_paths = sorted(paths["vanilla"] - paths["rust"])
    return {"result": "failed" if missing_roots or missing_paths else "passed",
            "scope": "root and literal paths with argument names normalized; redirects not expanded; execution excluded",
            "missing_roots": missing_roots, "extra_roots": sorted(roots["rust"] - roots["vanilla"]),
            "missing_literal_paths": missing_paths,
            "vanilla_roots": len(roots["vanilla"]), "rust_roots": len(roots["rust"]),
            "vanilla_literal_paths": len(paths["vanilla"]), "rust_literal_paths": len(paths["rust"])}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java", default=os.environ.get("JAVA", "java"))
    parser.add_argument("--rust-binary", type=Path, default=ROOT / "target/debug/ferraloom")
    parser.add_argument("--accept-eula", action="store_true", help="Accept Minecraft EULA for these isolated local reference runs")
    parser.add_argument("--server", choices=["both", "vanilla", "rust"], default="both")
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--startup-timeout", type=float, default=180)
    parser.add_argument("--quick", action="store_true", help="Run network scenarios only")
    parser.add_argument("--record", action="store_true", help="Record known gaps without failing the process; report still marks every failure")
    parser.add_argument("--output", type=Path, default=ROOT / "compatibility/results/latest.json")
    args = parser.parse_args()
    if args.server in ("both", "vanilla") and not args.accept_eula:
        parser.error("Vanilla execution requires explicit --accept-eula (https://aka.ms/MinecraftEULA)")
    if args.workers < 1:
        parser.error("--workers must be at least 1")
    if args.server in ("both", "rust") and not args.rust_binary.is_file():
        parser.error(f"Build Ferraloom first: missing {args.rust_binary}")
    (ROOT / ".cache/runs").mkdir(parents=True, exist_ok=True)
    if not args.quick:
        inventory(args.java, ROOT / "compatibility/vanilla-inventory.json")
    run_dir = Path(tempfile.mkdtemp(prefix="parity-", dir=ROOT / ".cache/runs"))
    if args.rust_binary.is_file():
        args.rust_binary = pin_binary(args.rust_binary, run_dir)
    report = {"schema_version": 1, "target_version": TARGET["version"], "protocol": TARGET["protocol"],
              "server_sha1": TARGET["server_sha1"], "rust_upstream": TARGET["rust_upstream"],
              "created_at": datetime.now(timezone.utc).isoformat(),
              "environment": {"host": socket.gethostname(), "platform": sys.platform, "auth": "isolated offline loopback", "seed": "12345678", "workers": args.workers},
              "scope": "Selected scenarios only; passing does not establish full vanilla parity.", "servers": {}}
    report["harness_manifest"] = harness_manifest()
    report["test_suite_sha256"] = hashlib.sha256(json.dumps(report["harness_manifest"], sort_keys=True).encode()).hexdigest()
    report["source_state"] = git_state()
    if args.rust_binary.is_file():
        report["rust_binary_sha256"] = hashlib.file_digest(args.rust_binary.open("rb"), "sha256").hexdigest()
    for kind in (["vanilla", "rust"] if args.server == "both" else [args.server]):
        print(f"Running {kind} on this host's loopback; logs: {run_dir / kind / 'console.log'}", flush=True)
        report["servers"][kind] = run(Server(kind, run_dir / kind, args), args.quick)
    if args.server == "both" and not args.quick:
        report["cross_world"] = [cross_world(source, destination, run_dir, args, report["servers"][source])
                                 for source, destination in [("vanilla", "rust"), ("rust", "vanilla")]]
        report["terrain"] = recorded("worldgen.underground_chunk", ("world.terrain",),
                                     lambda: compare_terrain(run_dir / "terrain/vanilla", run_dir / "terrain/rust"))
        report["command_tree"] = recorded("commands.advertised_syntax", ("players.commands",),
                                          lambda: compare_command_trees(report["servers"]))
    vanilla = {case["id"]: case for case in report["servers"].get("vanilla", [])}
    rust = {case["id"]: case for case in report["servers"].get("rust", [])}
    report["comparison"] = [{"id": key,
                             "vanilla": vanilla.get(key, {}).get("result", "not_run"),
                             "rust": rust.get(key, {}).get("result", "not_run"),
                             "result": "passed" if vanilla.get(key, {}).get("result") == rust.get(key, {}).get("result") == "passed" else "failed"}
                            for key in sorted(vanilla.keys() | rust.keys())]
    failed = [f"{kind}:{case['id']}" for kind, cases in report["servers"].items() for case in cases if case["result"] != "passed"]
    failed.extend(case["id"] for case in report.get("cross_world", []) if case["result"] != "passed")
    if report.get("terrain", {}).get("result") == "failed":
        failed.append("worldgen.underground_chunk")
    if report.get("command_tree", {}).get("result") == "failed":
        failed.append("commands.advertised_syntax")
    if report["harness_manifest"] != harness_manifest():
        failed.append("harness.changed_during_run")
    if args.rust_binary.is_file() and report["rust_binary_sha256"] != hashlib.file_digest(args.rust_binary.open("rb"), "sha256").hexdigest():
        failed.append("rust_binary.changed_during_run")
    report["failed"] = failed
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2).replace(str(ROOT), "$REPO") + "\n")
    print(f"Report: {args.output}; {len(failed)} failed scenarios. Raw logs: {run_dir}")
    return 0 if args.record or not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
