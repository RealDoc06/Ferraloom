//! BlockInput NBT is loaded within the placement call, before the next command.
use super::harness::{Case, Check, Step};
use serde_json::json;

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (index, (block, entity, payload, replacement)) in [
        ("chest", "chest", "CustomName:'atomic chest',Items:[{Slot:0b,id:'minecraft:stone',count:3}]", "CustomName:'changed'"),
        ("oak_sign", "sign", "front_text:{messages:['alpha','beta','',''],color:red,has_glowing_text:true},is_waxed:true", "is_waxed:false"),
        ("white_banner", "banner", "patterns:[{pattern:'minecraft:stripe_bottom',color:red}],CustomName:'atomic banner'", "CustomName:'changed'"),
        // SpawnData's additional sendBlockUpdated callback is BE-owned and pending.
        ("spawner", "mob_spawner", "Delay:100s,MinSpawnDelay:20,MaxSpawnDelay:30,SpawnCount:2,RequiredPlayerRange:0,SpawnRange:1", "Delay:120s,RequiredPlayerRange:0"),
    ].into_iter().enumerate() {
        let x = 2 + index as i32 * 3;
        let tag = format!("{{{payload},components:{{'minecraft:custom_data':{{placement:1b}}}},id:'minecraft:wrong',x:999,y:999,z:999}}");
        let replacement = format!("{{{replacement}}}");
        let state = format!("minecraft:{block}");
        let query = |x, z| Step::feedback(format!("data get block {x} -63 {z}"), "commands.data.block.query");
        let mut add = |name: &str, steps| cases.push(Case {
            name: format!("placement.nbt.{block}.{name}"), path: None, pending: None, steps,
        });
        add("setblock", vec![Step::feedback(format!("setblock {x} -63 2 {state}{tag} strict"), "commands.setblock.success"), query(x, 2)]);
        add("setblock-noop", vec![Step::feedback(format!("setblock {x} -63 2 {state}{tag} strict"), "commands.setblock.failed"), query(x, 2)]);
        add("setblock-nbt-only", vec![Step::feedback(format!("setblock {x} -63 2 {state}{replacement} strict"), "commands.setblock.success"), query(x, 2)]);
        let fill = format!("fill {x} -63 6 {} -63 6", x + 1);
        let mut first = Step::feedback(format!("{fill} {state}{tag} strict"), "commands.fill.success");
        first.checks.push(Check::Argument { key: "commands.fill.success".into(), index: 0, value: json!(2) });
        add("fill", vec![first, query(x, 6), query(x + 1, 6)]);
        add("fill-noop", vec![Step::feedback(format!("{fill} {state}{tag} strict"), "commands.fill.failed"), query(x, 6), query(x + 1, 6)]);
        let mut changed = Step::feedback(format!("{fill} {state}{replacement} strict"), "commands.fill.success");
        changed.checks.push(Check::Argument { key: "commands.fill.success".into(), index: 0, value: json!(2) });
        add("fill-nbt-only", vec![changed, query(x, 6), query(x + 1, 6)]);
        let mut filter = Step::feedback(format!("{fill} minecraft:stone replace {state}{{id:'minecraft:{entity}',x:{x}}} strict"), "commands.fill.success");
        filter.checks.push(Check::Argument { key: "commands.fill.success".into(), index: 0, value: json!(1) });
        filter.checks.push(Check::Blocks { min: [x, -63, 6], max: [x, -63, 6], state: "minecraft:stone".into(), interior: None });
        add("fill-full-metadata-predicate", vec![filter, Step::feedback(format!("data get block {x} -63 6"), "commands.data.block.invalid"), query(x + 1, 6)]);
    }
    cases
}
