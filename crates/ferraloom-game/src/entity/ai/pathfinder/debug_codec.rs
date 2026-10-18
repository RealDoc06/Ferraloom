//! Ports of Node/Target/Path DEBUG_STREAM_CODEC. Debug records decode to fresh
//! identities, as vanilla's codecs do; search-only links are not transmitted.
use super::{DebugData, Node, NodeId, Path, PathData, PathType, Target};
use ferraloom_protocol::{Error, Reader, Result, Writer};
use ferraloom_util::{BlockPos, collections::HashMap};
impl Node {
    pub fn write_debug(&self, output: &mut Writer<'_>) {
        output.i32(self.pos.x);
        output.i32(self.pos.y);
        output.i32(self.pos.z);
        output.f32(self.walked_distance);
        output.f32(self.cost_malus);
        output.bool(self.closed);
        output.var_int(i32::from(self.path_type.id()));
        output.f32(self.f);
    }
    pub fn read_debug(input: &mut Reader<'_>) -> Result<Self> {
        let mut node = Self::new(input.i32()?, input.i32()?, input.i32()?);
        node.walked_distance = input.f32()?;
        node.cost_malus = input.f32()?;
        node.closed = input.bool()?;
        node.path_type = PathType::by_id(input.var_int()?);
        node.f = input.f32()?;
        Ok(node)
    }
}
fn write_nodes(ids: &[NodeId], data: &PathData, output: &mut Writer<'_>) {
    output.var_int(ids.len() as i32);
    for &id in ids {
        data[id].write_debug(output);
    }
}
fn read_nodes(input: &mut Reader<'_>, data: &mut PathData) -> Result<Vec<NodeId>> {
    let count = input.var_int()?;
    if count < 0 {
        return Err(Error::invalid("path nodes", "negative collection size"));
    }
    let mut nodes = Vec::new();
    for _ in 0..count {
        nodes.push(data.add_node(Node::read_debug(input)?));
    }
    Ok(nodes)
}
impl Path {
    pub fn write_debug(&self, data: &PathData, output: &mut Writer<'_>) -> Result<()> {
        let debug = self
            .debug_data()
            .filter(|debug| !debug.targets.is_empty())
            .ok_or_else(|| Error::invalid("path", "Missing debug data"))?;
        output.bool(self.can_reach());
        output.i32(self.next_node_index());
        output.i64(self.target().as_long());
        write_nodes(self.nodes(data), data, output);
        write_nodes(&debug.open_set, data, output);
        write_nodes(&debug.closed_set, data, output);
        write_nodes(
            &debug
                .targets
                .iter()
                .map(|target| target.node)
                .collect::<Vec<_>>(),
            data,
            output,
        );
        Ok(())
    }
    pub fn read_debug(input: &mut Reader<'_>, data: &mut PathData) -> Result<Self> {
        let reached = input.bool()?;
        let cursor = input.i32()?;
        let target = BlockPos::of(input.i64()?);
        let nodes = read_nodes(input, data)?;
        let mut path = Self::new(nodes, target, reached, data);
        let open_set = read_nodes(input, data)?;
        let closed_set = read_nodes(input, data)?;
        let targets = read_nodes(input, data)?;
        // Native collection(HashSet::new) deduplicates coordinate-equal targets.
        let mut unique = HashMap::new();
        for id in targets {
            let key = data[id].key();
            if !unique.contains_key(&key) {
                unique.insert(key, id);
            }
        }
        let targets = unique.values().map(|&id| Target::at(id)).collect();
        path.set_next_node_index(cursor);
        path.set_debug(DebugData {
            open_set,
            closed_set,
            targets,
        });
        Ok(path)
    }
}
