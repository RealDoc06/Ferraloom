# GameTest

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| gametest.command.registration | Expose /test command branches for finding, running, resetting, exporting and verifying tests | `TestCommand#register` | | - | todo |
| gametest.instances.registry | Load namespaced test instances, structure references and test environment definitions | `GameTestInstance; TestEnvironmentDefinition; RegistryDataLoader` | | - | todo |
| gametest.environments.setup-teardown | Set up and tear down test environments with the configured rule, time and function effects | `TestEnvironmentDefinition; GameTestRunner` | | - | todo |
| gametest.instance-block.configuration | Edit and persist test instance block configuration and apply action packets | `TestInstanceBlockEntity; ServerGamePacketListenerImpl#handleTestInstanceBlockAction` | | - | todo |
| gametest.runner.lifecycle | Place rotated structures, wait setup ticks, start test execution and enforce timeout | `GameTestInfo#tick; GameTestInfo#prepareTestStructure` | | - | todo |
| gametest.runner.sequences | Run delayed assertions and event sequences against absolute and relative test coordinates | `GameTestHelper; GameTestSequence` | | - | todo |
| gametest.runner.retries | Retry flaky tests with required success count and maximum attempt constraints | `GameTestInfo; GameTestRunner` | | - | todo |
| gametest.runner.batches | Group tests by environment and execute batches with shared environment lifecycle | `GameTestBatch; GameTestRunner` | | - | todo |
| gametest.runner.results | Report pass, optional failure and required failure and mark test blocks and structures | `GameTestInfo; TestCommand` | | - | todo |
| gametest.server.headless | Run selected tests in the dedicated GameTest server and use failure-aware exit behavior | `GameTestServer` | | - | todo |
| gametest.block.test-modes | Execute start, accept, fail and log test-block interactions with the active test | `TestBlock; TestBlockEntity` | | - | todo |
| gametest.test-environment-definition-type.all-of | `minecraft:all_of` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment-definition-type.clock-time | `minecraft:clock_time` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment-definition-type.difficulty | `minecraft:difficulty` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment-definition-type.function | `minecraft:function` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment-definition-type.game-rules | `minecraft:game_rules` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment-definition-type.timeline-attributes | `minecraft:timeline_attributes` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment-definition-type.weather | `minecraft:weather` (test environment definition type) | `net.minecraft.gametest.framework.TestEnvironmentDefinition#bootstrap` | | - | todo |
| gametest.test-environment.default | `minecraft:default` (test environment) | `data/minecraft/test_environment/default.json`; `TestEnvironmentDefinition#DIRECT_CODEC` | | - | todo |
| gametest.test-function.always-pass | `minecraft:always_pass` (test function) | `net.minecraft.gametest.framework.BuiltinTestFunctions#bootstrap` | | - | todo |
| gametest.test-instance-type.block-based | `minecraft:block_based` (test instance type) | `net.minecraft.gametest.framework.GameTestInstance#bootstrap` | | - | todo |
| gametest.test-instance-type.function | `minecraft:function` (test instance type) | `net.minecraft.gametest.framework.GameTestInstance#bootstrap` | | - | todo |
| gametest.test-instance.always-pass | `minecraft:always_pass` (test instance) | `data/minecraft/test_instance/always_pass.json`; `GameTestInstance#DIRECT_CODEC` | | - | todo |
