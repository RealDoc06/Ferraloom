use super::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io;

#[derive(Deserialize)]
struct Vector {
    mode: String,
    strict: bool,
    filter: String,
    destination: i32,
    limit: i32,
    loaded: bool,
    debug: bool,
    changed: bool,
    entity: bool,
    value: Option<i32>,
    error: Option<String>,
    feedback: Option<String>,
    broadcast: Option<bool>,
    trace: Vec<Value>,
}
struct Host<'a> {
    vector: &'a Vector,
    trace: Vec<Value>,
}
fn point(p: BlockPos) -> [i32; 3] {
    [p.x, p.y, p.z]
}
fn event(call: &str, p: BlockPos) -> Value {
    json!({"call":call,"pos":point(p)})
}
impl CloneAccess for Host<'_> {
    type Error = io::Error;
    type Filter = String;
    fn max_block_modifications(&self) -> i32 {
        self.vector.limit
    }
    fn has_chunks_at(
        &mut self,
        _: &Identifier,
        begin: BlockPos,
        end: BlockPos,
    ) -> Result<bool, io::Error> {
        self.trace
            .push(json!({"call":"loaded","pos":point(begin),"end":point(end)}));
        Ok(self.vector.loaded)
    }
    fn is_debug(&mut self, _: &Identifier) -> Result<bool, io::Error> {
        self.trace.push(json!("debug"));
        Ok(self.vector.debug)
    }
    fn block_state(&mut self, _: &Identifier, pos: BlockPos) -> Result<BlockState, io::Error> {
        self.trace.push(event("state", pos));
        Ok(match pos.x {
            0 => Block::STONE.default_state(),
            1 => Block::TORCH.default_state(),
            _ => Block::AIR.default_state(),
        })
    }
    fn matches(
        &mut self,
        filter: &String,
        _: &Identifier,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<bool, io::Error> {
        self.trace.push(event("filter", pos));
        Ok(match filter.as_str() {
            "none" => false,
            "masked" => !state.is_air(),
            "stone" => state == Block::STONE.default_state(),
            _ => true,
        })
    }
    fn block_entity(
        &mut self,
        _: &Identifier,
        pos: BlockPos,
    ) -> Result<Option<BlockEntityData>, io::Error> {
        self.trace.push(event("entity", pos));
        if self.vector.entity && pos.x == 1 {
            self.trace.push(json!("save_custom"));
            let mut custom = CompoundTag::new();
            custom.insert("value", ferraloom_nbt::Tag::Int(42));
            Ok(Some(BlockEntityData {
                custom,
                components: DataComponentMap::default(),
            }))
        } else {
            Ok(None)
        }
    }
    fn is_solid_render(
        &mut self,
        _: &Identifier,
        _: BlockPos,
        state: BlockState,
    ) -> Result<bool, io::Error> {
        Ok(state == Block::STONE.default_state())
    }
    fn is_collision_shape_full_block(
        &mut self,
        _: &Identifier,
        _: BlockPos,
        _: BlockState,
    ) -> Result<bool, io::Error> {
        Ok(false)
    }
    fn set_block(
        &mut self,
        _: &Identifier,
        pos: BlockPos,
        state: BlockState,
        flags: u32,
    ) -> Result<bool, io::Error> {
        let name = if state == Block::STONE.default_state() {
            "stone"
        } else if state == Block::TORCH.default_state() {
            "torch"
        } else if state == Block::BARRIER.default_state() {
            "barrier"
        } else {
            "air"
        };
        self.trace
            .push(json!({"call":"set","pos":point(pos),"state":name,"flags":flags}));
        Ok(self.vector.changed)
    }
    fn restore_block_entity(
        &mut self,
        _: &Identifier,
        pos: BlockPos,
        data: &BlockEntityData,
    ) -> Result<(), io::Error> {
        assert_eq!(data.custom.get_int("value"), Some(42));
        assert!(data.components.entries.is_empty());
        self.trace.push(event("entity", pos));
        self.trace
            .extend([json!("load_custom"), json!("components"), json!("changed")]);
        Ok(())
    }
    fn update_neighbors_on_block_set(
        &mut self,
        _: &Identifier,
        pos: BlockPos,
        _: BlockState,
    ) -> Result<(), io::Error> {
        self.trace.push(event("neighbors", pos));
        Ok(())
    }
    fn copy_block_ticks(
        &mut self,
        _: &Identifier,
        _: &Identifier,
        _: TickArea,
        offset: BlockPos,
    ) -> Result<(), io::Error> {
        self.trace.push(event("ticks", offset));
        Ok(())
    }
}

#[test]
fn native_clone_modes_filter_order_flags_and_errors() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/clone/vectors.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 17);
    for vector in &vectors {
        let mut host = Host {
            vector,
            trace: Vec::new(),
        };
        let position = |x| Position {
            dimension: Identifier::parse("minecraft:overworld").unwrap(),
            position: BlockPos::new(x, 80, 0),
        };
        let mode = match vector.mode.as_str() {
            "MOVE" => Mode::Move,
            "FORCE" => Mode::Force,
            _ => Mode::Normal,
        };
        let result = execute(
            &mut host,
            position(0),
            position(2),
            position(vector.destination),
            &vector.filter,
            mode,
            vector.strict,
        );
        match result {
            Ok(result) => {
                assert_eq!(Some(result.value), vector.value);
                assert_eq!(
                    Some(crate::language::plain_text(&result.feedback[0].message)),
                    vector.feedback
                );
                assert_eq!(Some(result.feedback[0].broadcast), vector.broadcast);
                host.trace.push(json!("success"));
            }
            Err(CloneError::Syntax(error)) => {
                assert_eq!(Some(error.raw_message.text().to_owned()), vector.error)
            }
            Err(error) => panic!("{error}"),
        }
        assert_eq!(
            host.trace, vector.trace,
            "{} strict={} filter={}",
            vector.mode, vector.strict, vector.filter
        );
    }
}
