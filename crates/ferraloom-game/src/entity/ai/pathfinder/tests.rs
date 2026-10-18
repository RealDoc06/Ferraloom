use super::*;
use ferraloom_util::{
    BlockPos,
    collections::{HashSet, Int2ObjectOpenHashMap},
};
use serde_json::{Value, json};
use std::convert::Infallible;
fn pos(v: &Value) -> BlockPos {
    BlockPos::new(
        v[0].as_i64().unwrap() as i32,
        v[1].as_i64().unwrap() as i32,
        v[2].as_i64().unwrap() as i32,
    )
}
fn float(v: &Value) -> f32 {
    f32::from_bits(v.as_i64().unwrap() as u32)
}
fn bits(v: f32) -> i32 {
    v.to_bits() as i32
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn node(n: &Node, data: &PathData) -> Value {
    json!({"pos":[n.pos.x,n.pos.y,n.pos.z],"hash":Node::create_hash(n.pos.x,n.pos.y,n.pos.z),"heap":n.heap_idx,"g":bits(n.g),"h":bits(n.h),"f":bits(n.f),"walked":bits(n.walked_distance),"malus":bits(n.cost_malus),"closed":n.closed,"type":n.path_type.id(),"from":n.came_from.map(|id|{let p=data[id].pos;[p.x,p.y,p.z]})})
}
fn path(p: &Path, data: &PathData) -> Value {
    json!({"nodes":p.nodes(data).iter().map(|&id|node(&data[id],data)).collect::<Vec<_>>(),"cursor":p.next_node_index(),"not_started":p.not_started(),"done":p.is_done(data),"dist":bits(p.dist_to_target()),"hash":p.hash_code(data)})
}
fn pair(a: &Path, b: &Path, data: &PathData) -> Value {
    json!({"a":path(a,data),"b":path(b,data),"same":a.same_as(Some(b),data),"equals":a.equals(b,data)})
}
fn rows() -> Vec<Value> {
    serde_json::from_str(include_str!("../../../../tests/java/pathfinder/core.json")).unwrap()
}
#[test]
fn native_node_target_and_path_type_values() {
    let mut count = 0;
    for row in rows() {
        match row["kind"].as_str().unwrap() {
            "node" => {
                count += 1;
                let p = pos(&row["pos"]);
                let mut n = Node::new(p.x, p.y, p.z);
                let to = BlockPos::new(5, 7, -9);
                assert_eq!(Node::create_hash(p.x, p.y, p.z), row["hash"]);
                assert_eq!(bits(n.distance_to(to)), row["distance"]);
                assert_eq!(bits(n.distance_to_sqr(to)), row["squared"]);
                assert_eq!(bits(n.distance_to_xz(to)), row["xz"]);
                assert_eq!(bits(n.distance_manhattan(to)), row["manhattan"]);
                let mut data = PathData::default();
                let link = data.add_node(Node::new(5, 7, -9));
                n.heap_idx = 3;
                n.g = 1.25;
                n.h = -0.0;
                n.f = 9.5;
                n.walked_distance = 6.75;
                n.cost_malus = -2.0;
                n.path_type = PathType::Water;
                n.closed = true;
                n.came_from = Some(link);
                assert_eq!(node(&n.clone_and_move(-5, 200, 8), &data), row["clone"]);
                let mut output = ferraloom_protocol::Writer::new();
                n.write_debug(&mut output);
                assert_eq!(hex(output.as_slice()), row["bytes"]);
                let mut input = ferraloom_protocol::Reader::new(output.as_slice());
                let decoded = Node::read_debug(&mut input).unwrap();
                input.finish().unwrap();
                assert_eq!(decoded, n);
                assert_eq!(decoded.heap_idx, -1);
                assert!(decoded.came_from.is_none());
                let mut round = ferraloom_protocol::Writer::new();
                decoded.write_debug(&mut round);
                assert_eq!(output.as_slice(), round.as_slice());
            }
            "type" => {
                count += 1;
                let id = row["id"].as_i64().unwrap() as i32;
                let value = PathType::by_id(id);
                assert_eq!(i32::from(value.id()), id);
                assert_eq!(bits(value.malus()), row["malus"]);
            }
            "target" => {
                count += 1;
                let mut data = PathData::default();
                let first = data.add_node(Node::new(1, 2, 3));
                let second = data.add_node(Node::new(4, 5, 6));
                let mut target = Target::new(first, &mut data);
                assert_ne!(target.node, first);
                for (i, state) in row["states"].as_array().unwrap().iter().enumerate() {
                    let h = float(&state["h"]);
                    let input = if i == 0 || h.is_nan() || h == 8.0 {
                        first
                    } else {
                        second
                    };
                    target.update_best(h, input);
                    let p = data[target.best_node().unwrap()].pos;
                    assert_eq!(json!([p.x, p.y, p.z]), state["best"]);
                    assert_eq!(target.is_reached(), state["reached"]);
                }
                target.set_reached();
                assert_eq!(target.is_reached(), row["reached"]);
            }
            "type-fallback" => {
                count += 1;
                for value in row["values"].as_array().unwrap() {
                    assert_eq!(
                        i32::from(PathType::by_id(value["id"].as_i64().unwrap() as i32).id()),
                        value["result"]
                    );
                }
            }
            "path-empty" => {
                count += 1;
                let mut data = PathData::default();
                let empty = Path::new(Vec::new(), BlockPos::ZERO, false, &mut data);
                assert_eq!(path(&empty, &data), row["state"]);
            }
            _ => {}
        }
    }
    assert_eq!(count, 38);
}
fn heap_state(heap: &BinaryHeap, ids: &[NodeId], data: &PathData) -> Value {
    json!({"heap":heap.get_heap().iter().map(|&id|data[id].pos.x).collect::<Vec<_>>(),"indices":ids.iter().map(|&id|data[id].heap_idx).collect::<Vec<_>>(),"costs":ids.iter().map(|&id|bits(data[id].f)).collect::<Vec<_>>()})
}
#[test]
fn native_heap_ties_changes_clear_and_growth() {
    let mut count = 0;
    for row in rows() {
        let kind = row["kind"].as_str().unwrap();
        if kind != "heap" && kind != "heap-growth" {
            continue;
        }
        count += 1;
        let mut data = PathData::default();
        let mut heap = BinaryHeap::new();
        if kind == "heap-growth" {
            for i in 0..140 {
                let mut node = Node::new(i, 0, 0);
                node.f = ((i * 31) % 17) as f32;
                let id = data.add_node(node);
                heap.insert(id, &mut data).unwrap();
            }
            let mut popped = Vec::new();
            while !heap.is_empty() {
                let id = heap.pop(&mut data);
                popped.push(data[id].pos.x);
            }
            assert_eq!(json!(popped), row["popped"]);
            continue;
        }
        let ids: Vec<_> = row["costs"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut n = Node::new(i as i32, 0, 0);
                n.f = float(c);
                data.add_node(n)
            })
            .collect();
        for step in row["steps"].as_array().unwrap() {
            let parts: Vec<_> = step["op"].as_str().unwrap().split(':').collect();
            match parts[0] {
                "insert" => {
                    let result = heap.insert(ids[parts[1].parse::<usize>().unwrap()], &mut data);
                    assert_eq!(
                        result.err().map(|_| "OW KNOWS!"),
                        step.get("error").and_then(Value::as_str)
                    );
                }
                "pop" => {
                    let id = heap.pop(&mut data);
                    assert_eq!(data[id].pos.x, step["popped"]);
                }
                "peek" => assert_eq!(data[heap.peek()].pos.x, step["peek"]),
                "change" => heap.change_cost(
                    ids[parts[1].parse::<usize>().unwrap()],
                    parts[2].parse().unwrap(),
                    &mut data,
                ),
                "remove" => heap.remove(ids[parts[1].parse::<usize>().unwrap()], &mut data),
                "clear" => heap.clear(),
                _ => unreachable!(),
            }
            assert_eq!(
                heap_state(&heap, &ids, &data),
                step["state"],
                "{}: {}",
                row["name"],
                step["op"]
            );
        }
    }
    assert_eq!(count, 4);
}
#[test]
fn native_path_copy_shares_nodes_and_list_but_not_cursor() {
    let row = rows()
        .into_iter()
        .find(|r| r["kind"] == "path-alias")
        .unwrap();
    let mut data = PathData::default();
    let ids: Vec<_> = [(1, 2, 3), (2, 3, 4), (4, 5, 6)]
        .into_iter()
        .map(|(x, y, z)| data.add_node(Node::new(x, y, z)))
        .collect();
    let mut a = Path::new(ids.clone(), BlockPos::new(8, 9, 10), true, &mut data);
    let mut b = a.copy();
    assert_eq!(a.nodes_id(), b.nodes_id());
    let mut states = vec![pair(&a, &b, &data)];
    a.advance();
    states.push(pair(&a, &b, &data));
    data[ids[1]].f = 9.5;
    states.push(pair(&a, &b, &data));
    let replacement = data.add_node(Node::new(7, 8, 9));
    a.replace_node(1, replacement, &mut data);
    states.push(pair(&a, &b, &data));
    b.truncate_nodes(2, &mut data);
    states.push(pair(&a, &b, &data));
    b.set_next_node_index(-2);
    states.push(pair(&a, &b, &data));
    a.set_next_node_index(i32::MAX);
    a.advance();
    states.push(pair(&a, &b, &data));
    assert_eq!(json!(states), row["states"]);
}
struct Graph {
    positions: Vec<BlockPos>,
    edges: Vec<Vec<usize>>,
    costs: Vec<f32>,
    null_start: bool,
    calls: Vec<String>,
    cache: Int2ObjectOpenHashMap<NodeId>,
}
impl Graph {
    fn get_node(&mut self, pos: BlockPos, data: &mut PathData) -> NodeId {
        let key = Node::create_hash(pos.x, pos.y, pos.z);
        if let Some(&id) = self.cache.get(&key) {
            return id;
        }
        let id = data.add_node(Node::new(pos.x, pos.y, pos.z));
        self.cache.insert(key, id);
        id
    }
    fn at(&mut self, index: usize, data: &mut PathData) -> NodeId {
        let id = self.get_node(self.positions[index], data);
        data[id].cost_malus = self.costs[index];
        id
    }
}
impl NodeEvaluator for Graph {
    type Error = Infallible;
    fn prepare(&mut self, _: &mut PathData) -> Result<(), Self::Error> {
        self.cache.clear();
        self.calls.push("prepare".into());
        Ok(())
    }
    fn done(&mut self, _: &mut PathData) -> Result<(), Self::Error> {
        self.calls.push("done".into());
        Ok(())
    }
    fn get_start(&mut self, data: &mut PathData) -> Result<Option<NodeId>, Self::Error> {
        self.calls.push("start".into());
        Ok(if self.null_start {
            None
        } else {
            Some(self.at(0, data))
        })
    }
    fn get_target(&mut self, pos: BlockPos, data: &mut PathData) -> Result<Target, Self::Error> {
        self.calls
            .push(format!("target:{},{},{}", pos.x, pos.y, pos.z));
        let id = self.get_node(pos, data);
        Ok(Target::new(id, data))
    }
    fn get_neighbors(
        &mut self,
        node: NodeId,
        out: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), Self::Error> {
        let pos = data[node].pos;
        self.calls
            .push(format!("neighbors:{},{},{}", pos.x, pos.y, pos.z));
        if let Some(index) = self.positions.iter().position(|&p| p == pos) {
            for i in 0..self.edges[index].len() {
                let neighbor = self.at(self.edges[index][i], data);
                if !data[neighbor].closed {
                    out.push(neighbor);
                }
            }
        }
        Ok(())
    }
}
#[test]
fn native_synthetic_search_paths_state_lifecycle_and_debug_bytes() {
    let mut count = 0;
    for row in rows() {
        if row["kind"] != "search" {
            continue;
        }
        count += 1;
        let mut graph = Graph {
            positions: row["positions"]
                .as_array()
                .unwrap()
                .iter()
                .map(pos)
                .collect(),
            edges: serde_json::from_value(row["edges"].clone()).unwrap(),
            costs: row["costs"].as_array().unwrap().iter().map(float).collect(),
            null_start: row["null_start"].as_bool().unwrap(),
            calls: Vec::new(),
            cache: Int2ObjectOpenHashMap::new(),
        };
        let mut data = PathData::default();
        let mut finder = PathFinder::new(row["budget"].as_i64().unwrap() as i32);
        finder.set_capture_debug(true);
        if row["horizontal"] == true {
            finder.distance_metric = DistanceMetric::Horizontal;
        }
        let mut targets = HashSet::new();
        for v in row["targets"].as_array().unwrap() {
            targets.insert(pos(v));
        }
        let result = finder.find_path(
            &mut graph,
            &mut data,
            &targets,
            row["length"].as_f64().unwrap() as f32,
            row["reach"].as_i64().unwrap() as i32,
            row["multiplier"].as_f64().unwrap() as f32,
        );
        if row.get("error").is_some() {
            assert!(matches!(result, Err(SearchError::DuplicateTarget)));
            assert_eq!(json!(graph.calls), row["calls"]);
            continue;
        }
        let path_result = result.unwrap();
        let mut actual = json!({"calls":graph.calls,"path":Value::Null});
        if let Some(p) = path_result {
            actual["path"] = path(&p, &data);
            let target = p.target();
            actual["target"] = json!([target.x, target.y, target.z]);
            actual["reached"] = json!(p.can_reach());
            let mut output = ferraloom_protocol::Writer::new();
            if p.write_debug(&data, &mut output).is_ok() {
                actual["bytes"] = json!(hex(output.as_slice()));
                let mut decoded_data = PathData::default();
                let mut input = ferraloom_protocol::Reader::new(output.as_slice());
                let decoded = Path::read_debug(&mut input, &mut decoded_data).unwrap();
                input.finish().unwrap();
                let mut round = ferraloom_protocol::Writer::new();
                decoded.write_debug(&decoded_data, &mut round).unwrap();
                assert_eq!(
                    output.as_slice(),
                    round.as_slice(),
                    "{} debug round trip",
                    row["name"]
                );
            } else {
                actual["debug_error"] = json!("Missing debug data");
            }
        }
        actual["nodes"] = json!(
            (0..graph.positions.len())
                .map(|i| {
                    let id = graph.at(i, &mut data);
                    node(&data[id], &data)
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(actual, row["result"], "search {}", row["name"]);
    }
    assert_eq!(count, 28);
}
