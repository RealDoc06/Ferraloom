use super::{CompiledFunction, FunctionParseError, read_lines};
use crate::commands::{CommandSourceStack, Commands};
use ferraloom_data::datapack::{Identifier, ResourceManager, load_tags};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
pub struct FunctionLoadError {
    pub id: Identifier,
    pub error: FunctionParseError,
}
/// Port of ServerFunctionLibrary's published function and tag definitions. The
/// live server owns one library; function executions retain immutable entries.
#[derive(Debug, Default)]
pub struct FunctionLibrary {
    functions: BTreeMap<Identifier, Arc<CompiledFunction>>,
    tags: BTreeMap<Identifier, Vec<Arc<CompiledFunction>>>,
}
impl FunctionLibrary {
    pub fn load(
        resources: &ResourceManager,
        commands: &Commands,
        compilation: CommandSourceStack,
    ) -> (
        Self,
        Vec<FunctionLoadError>,
        Vec<ferraloom_data::datapack::TagDiagnostic>,
    ) {
        let mut errors = Vec::new();
        let mut functions = BTreeMap::new();
        for resource in resources.list("function/", ".mcfunction") {
            let id = resource
                .with_path(
                    resource
                        .path()
                        .strip_prefix("function/")
                        .unwrap()
                        .strip_suffix(".mcfunction")
                        .unwrap(),
                )
                .expect("function resource identifier");
            let lines = read_lines(
                resources
                    .get(&resource)
                    .expect("listed function resource")
                    .bytes,
            );
            match CompiledFunction::from_lines(id.clone(), commands, compilation.clone(), &lines) {
                Ok(function) => {
                    functions.insert(id, Arc::new(function));
                }
                Err(error) => errors.push(FunctionLoadError { id, error }),
            }
        }
        let loaded = load_tags(resources, "function", |id, _| {
            functions.contains_key(id).then(|| id.clone())
        });
        (
            Self {
                tags: loaded
                    .values
                    .into_iter()
                    .map(|(tag, ids)| {
                        (
                            tag,
                            ids.into_iter()
                                .map(|id| {
                                    functions
                                        .get(&id)
                                        .expect("resolved function tag entry")
                                        .clone()
                                })
                                .collect(),
                        )
                    })
                    .collect(),
                functions,
            },
            errors,
            loaded.diagnostics,
        )
    }
    pub fn get(&self, id: &Identifier) -> Option<&Arc<CompiledFunction>> {
        self.functions.get(id)
    }
    /// Native getTag returns an empty collection even for an unknown tag.
    pub fn tag(&self, id: &Identifier) -> &[Arc<CompiledFunction>] {
        self.tags.get(id).map_or(&[], Vec::as_slice)
    }
    pub fn names(&self) -> impl Iterator<Item = &Identifier> {
        self.functions.keys()
    }
    pub fn tag_names(&self) -> impl Iterator<Item = &Identifier> {
        self.tags.keys()
    }
}
