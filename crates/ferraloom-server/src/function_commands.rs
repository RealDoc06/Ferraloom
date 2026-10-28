//! FunctionCommand lookup and argument reads at the native exclusive call site.
use super::*;
use ferraloom_game::commands::{
    builtins::{component, format_list},
    functions::CompiledFunction,
};

impl MinecraftServer {
    pub(in crate::server) fn prepare_function_call(
        &mut self,
        connection: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<Vec<Arc<CompiledFunction>>> {
        let name = command.string("name").expect("parsed function name");
        let (tag, name) = name
            .strip_prefix('#')
            .map_or((false, name), |name| (true, name));
        let id = name.parse::<ferraloom_data::datapack::Identifier>()?;
        let library = self.functions.library();
        let functions = if tag {
            library.tag(&id).to_vec()
        } else {
            vec![library.get(&id).cloned().ok_or_else(|| {
                exceptions::error("arguments.function.unknown", &[id.to_string()])
            })?]
        };
        if functions.is_empty() {
            return Err(exceptions::error_with_arguments(
                "commands.function.scheduled.no_functions",
                vec![component(Component::literal(id.to_string()))],
            )
            .into());
        }
        // PlainTextFunction ignores the value, but native resolves it first and
        // still reports a missing path, invalid receiver, or non-compound tag.
        let _arguments = self.function_arguments(command)?;
        let message = if functions.len() == 1 {
            Component::translatable(
                "commands.function.scheduled.single",
                vec![component(Component::literal(functions[0].id().to_string()))],
            )
        } else {
            Component::translatable(
                "commands.function.scheduled.multiple",
                vec![component(format_list(functions.iter().map(|function| {
                    Component::literal(function.id().to_string())
                })))],
            )
        };
        self.command_success(connection, &command.source, message, true);
        Ok(functions)
    }
}

/// Port of ServerFunctionLibrary's startup compilation and tag publication.
pub(in crate::server) fn load_functions(
    resources: &ferraloom_data::datapack::ResourceManager,
    commands: &Commands,
    permission: i32,
) -> ferraloom_game::commands::functions::ServerFunctionManager {
    use ferraloom_game::commands::functions::{FunctionLibrary, ServerFunctionManager};
    let (library, errors, tag_errors) = FunctionLibrary::load(
        resources,
        commands,
        CommandSourceStack::server(permission, Vec3::ZERO),
    );
    for error in errors {
        eprintln!("Failed to load function {}: {}", error.id, error.error);
    }
    for error in tag_errors {
        eprintln!(
            "Failed to load function tag {} from {}: {}",
            error.tag, error.source, error.message
        );
    }
    ServerFunctionManager::new(library)
}
