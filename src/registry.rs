use std::{collections::BTreeMap, fs::File, io::BufWriter, io::Write, path::Path};

use anyhow::{Context, Result};
use pumpkin_data::{Block, BlockId, packet::CURRENT_MC_VERSION};
use serde::Serialize;

#[derive(Serialize)]
struct Registry {
    version: String,
    protocol: i32,
    blocks: BTreeMap<String, BlockDefinition>,
}

#[derive(Serialize)]
struct BlockDefinition {
    id: u16,
    states: Vec<State>,
}

#[derive(Serialize)]
struct State {
    id: u16,
    solid_render: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<BTreeMap<&'static str, &'static str>>,
}

/// Export engine identities and solidity flags alongside vanilla's block report shape.
pub fn export(path: &Path) -> Result<()> {
    let mut blocks = BTreeMap::new();
    for raw_id in 0..BlockId::COUNT {
        let id = BlockId::new(raw_id).context("Invalid block ID in engine registry")?;
        let block = Block::from_id(id);
        let states = block
            .states
            .iter()
            .map(|state| State {
                id: state.id.as_u16(),
                solid_render: state.is_solid_render(),
                default: (state.id == block.default_state.id).then_some(true),
                properties: block
                    .properties(state.id)
                    .map(|properties| {
                        properties
                            .to_props()
                            .into_iter()
                            .collect::<BTreeMap<_, _>>()
                    })
                    .filter(|properties| !properties.is_empty()),
            })
            .collect();
        blocks.insert(
            format!("minecraft:{}", block.name),
            BlockDefinition {
                id: id.as_u16(),
                states,
            },
        );
    }
    let registry = Registry {
        version: CURRENT_MC_VERSION.to_string(),
        protocol: CURRENT_MC_VERSION.protocol_version(),
        blocks,
    };
    let file = File::create(path)
        .with_context(|| format!("Cannot create registry export {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &registry)
        .context("Cannot serialize engine registry")?;
    writer.flush().context("Cannot finish registry export")?;
    Ok(())
}
