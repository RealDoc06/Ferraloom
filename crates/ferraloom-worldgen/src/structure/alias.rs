//! PoolAliasBinding resolves once per structure start using its own positional
//! random source. Alias resolution never advances jigsaw placement's RNG.
use crate::{Error, Identifier, codec::*, placement::providers::WeightedList};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use serde_json::Value;
use std::collections::BTreeMap;

/// Port of DirectPoolAlias, RandomPoolAlias and RandomGroupPoolAlias.
#[derive(Clone, Debug)]
pub enum PoolAliasBinding {
    Direct {
        alias: Identifier,
        target: Identifier,
    },
    Random {
        alias: Identifier,
        targets: WeightedList<Identifier>,
    },
    RandomGroup(WeightedList<Vec<Self>>),
}
impl PoolAliasBinding {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match kind(v)?.to_string().as_str() {
            "minecraft:direct" => Ok(Self::Direct {
                alias: id(field(v, "alias")?)?,
                target: id(field(v, "target")?)?,
            }),
            "minecraft:random" => Ok(Self::Random {
                alias: id(field(v, "alias")?)?,
                targets: WeightedList::decode(field(v, "targets")?, id)?,
            }),
            "minecraft:random_group" => Ok(Self::RandomGroup(WeightedList::decode(
                field(v, "groups")?,
                |v| list(v)?.iter().map(Self::decode).collect(),
            )?)),
            s => Err(Error(format!("unknown pool alias binding {s}"))),
        }
    }
    fn resolve(
        &self,
        random: &mut impl RandomSource,
        output: &mut BTreeMap<Identifier, Identifier>,
    ) -> Result<(), Error> {
        let (alias, target) = match self {
            Self::Direct { alias, target } => (alias, target),
            Self::Random { alias, targets } => (alias, targets.sample(random)),
            Self::RandomGroup(groups) => {
                for binding in groups.sample(random) {
                    binding.resolve(random, output)?;
                }
                return Ok(());
            }
        };
        if output.insert(alias.clone(), target.clone()).is_some() {
            return Err(Error(format!("duplicate pool alias {alias}")));
        }
        Ok(())
    }
}
/// Port of PoolAliasLookup. Duplicate bindings are an error, as with vanilla's
/// ImmutableMap.Builder; an absent binding returns the original pool key.
#[derive(Clone, Debug, Default)]
pub struct PoolAliasLookup {
    pub mappings: BTreeMap<Identifier, Identifier>,
}
impl PoolAliasLookup {
    pub fn create(
        bindings: &[PoolAliasBinding],
        position: [i32; 3],
        seed: i64,
    ) -> Result<Self, Error> {
        let mut output = Self::default();
        if bindings.is_empty() {
            return Ok(output);
        }
        let mut random = LegacyRandomSource::new(seed).fork_positional().at(
            position[0],
            position[1],
            position[2],
        );
        for binding in bindings {
            binding.resolve(&mut random, &mut output.mappings)?;
        }
        Ok(output)
    }
    pub fn lookup<'a>(&'a self, alias: &'a Identifier) -> &'a Identifier {
        self.mappings.get(alias).unwrap_or(alias)
    }
}
