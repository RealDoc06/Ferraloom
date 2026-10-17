# Functions and macros

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| functions-macros.loader.lines | Parse mcfunction lines, comments and continued lines and reject invalid command syntax | `CommandFunction; FunctionBuilder; ServerFunctionLibrary` | | - | todo |
| functions-macros.execution.queue | Execute function entries in their command-source context with chain-length and fork limits | `ServerFunctionManager; ExecutionContext` | | - | todo |
| functions-macros.tags.load-tick | Run minecraft:load after resource reload and minecraft:tick each server tick | `ServerFunctionManager#tick` | | - | todo |
| functions-macros.schedule.delayed | Schedule functions or function tags by game time with append, replace and clear behavior | `ScheduleCommand; TimerQueue` | | - | todo |
| functions-macros.macros.parameters | Parse dollar-prefixed macro lines and substitute named arguments from compounds | `FunctionBuilder#addMacro; MacroFunction#instantiate` | | - | todo |
| functions-macros.macros.value-format | Format numeric, string and other NBT arguments for command substitution | `MacroFunction#stringify` | | - | todo |
| functions-macros.macros.cache-errors | Cache instantiated macro functions and reject missing parameters or invalid substituted syntax | `MacroFunction#instantiate; MacroFunction#substituteAndParse` | | - | todo |
| functions-macros.return.values | Return explicit values or failures and propagate function execution results | `ReturnCommand; CommandQueueEntry; ExecutionContext` | | - | todo |
| functions-macros.permissions.compilation | Compile and run functions with function-permission-level and command-source checks | `MinecraftServer#getFunctionCompilationPermissions; ServerFunctionManager` | | - | todo |
| functions-macros.storage.context | Read macro argument compounds from entity, block or command storage sources | `FunctionCommand; CommandStorage` | | - | todo |
