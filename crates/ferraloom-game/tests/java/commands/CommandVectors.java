import com.google.gson.*;
import com.mojang.brigadier.arguments.*;
import com.mojang.brigadier.tree.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.synchronization.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundCommandsPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.permissions.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.*;

/** Runs against the pinned server jar; output is data, never decompiled code. */
public final class CommandVectors {
    static final Gson JSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static CommandSourceStack source(int level) {
        return Commands.createCompilationContext(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(level)));
    }
    static <T> net.minecraft.core.HolderLookup.RegistryLookup<T> emptyRegistry(net.minecraft.resources.ResourceKey<? extends net.minecraft.core.Registry<T>> key) {
        return new net.minecraft.core.MappedRegistry<T>(key, com.mojang.serialization.Lifecycle.stable()).freeze();
    }
    static void write(Path out, String name, Object value) throws Exception {
        Files.writeString(out.resolve(name), JSON.toJson(value) + "\n");
    }
    static RootCommandNode<CommandSourceStack> filter(CommandNode<CommandSourceStack> original, int level) {
        var root = new RootCommandNode<CommandSourceStack>();
        var converted = new IdentityHashMap<CommandNode<CommandSourceStack>,CommandNode<CommandSourceStack>>();
        converted.put(original, root);
        fill(original, root, source(level), converted);
        return root;
    }
    static void fill(CommandNode<CommandSourceStack> from, CommandNode<CommandSourceStack> to,
            CommandSourceStack source, Map<CommandNode<CommandSourceStack>,CommandNode<CommandSourceStack>> converted) {
        for (var child : from.getChildren()) if (child.canUse(source)) {
            var builder = child.createBuilder();
            if (builder.getRedirect() != null) builder.redirect(converted.get(builder.getRedirect()));
            var node = builder.build();
            converted.put(child, node);
            to.addChild(node);
            fill(child, node, source, converted);
        }
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var baseRegistries = VanillaRegistries.createWorldLookup();
        // ResourceOrIdArgument returns null when a registry is absent. Supply empty
        // runtime registries so vectors exercise reference/codec validation instead.
        var additional = java.util.stream.Stream.of(
            emptyRegistry(net.minecraft.core.registries.Registries.LOOT_TABLE),
            emptyRegistry(net.minecraft.core.registries.Registries.PREDICATE),
            emptyRegistry(net.minecraft.core.registries.Registries.ITEM_MODIFIER),
            emptyRegistry(net.minecraft.core.registries.Registries.CONTEXT_FLOAT_PROVIDER),
            emptyRegistry(net.minecraft.core.registries.Registries.CONTEXT_INT_PROVIDER),
            emptyRegistry(net.minecraft.core.registries.Registries.SLOT_SOURCE)
        ).filter(registry -> baseRegistries.lookup(registry.key()).isEmpty());
        var registries = net.minecraft.core.HolderLookup.Provider.create(java.util.stream.Stream.concat(baseRegistries.listRegistries(), additional));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p -> p.apply());
        var elements = new JsonObject();
        registries.listRegistries().forEach(registry -> {
            var entries = new JsonArray();
            registry.listElements().forEach(holder -> entries.add(holder.key().identifier().toString()));
            elements.add(registry.key().identifier().toString(), entries);
        });
        // Exercise real argument parsers with a registry view containing no bound tags.
        // Datagen lookup otherwise invents every requested tag holder.
        var runtimeRegistries = new net.minecraft.core.HolderLookup.Provider() {
            public java.util.stream.Stream<net.minecraft.resources.ResourceKey<? extends net.minecraft.core.Registry<?>>> listRegistryKeys() { return registries.listRegistryKeys(); }
            public <T> Optional<net.minecraft.core.HolderLookup.RegistryLookup<T>> lookup(net.minecraft.resources.ResourceKey<? extends net.minecraft.core.Registry<? extends T>> key) {
                return registries.lookup(key).map(parent -> new net.minecraft.core.HolderLookup.RegistryLookup.Delegate<T>() {
                    public net.minecraft.core.HolderLookup.RegistryLookup<T> parent() { return parent; }
                    public java.util.stream.Stream<net.minecraft.core.HolderSet.Named<T>> listTags() { return java.util.stream.Stream.empty(); }
                    public Optional<net.minecraft.core.HolderSet.Named<T>> get(net.minecraft.tags.TagKey<T> tag) {
                        return Optional.empty();
                    }
                });
            }
        };
        var tags = new JsonObject();
        runtimeRegistries.listRegistries().forEach(registry -> {
            var boundTags = new JsonObject();
            registry.listTags().forEach(tag -> {
                var members = new JsonArray();
                tag.stream().forEach(holder -> holder.unwrapKey().ifPresent(key -> members.add(key.identifier().toString())));
                boundTags.add(tag.key().location().toString(), members);
            });
            tags.add(registry.key().identifier().toString(), boundTags);
        });
        var context = CommandBuildContext.simple(runtimeRegistries, FeatureFlags.DEFAULT_FLAGS);
        var commands = new Commands(Commands.CommandSelection.DEDICATED, context);
        var dispatcher = commands.getDispatcher();
        var out = Path.of(args[0]);
        Files.createDirectories(out);
        write(out, "registry-elements.json", elements);
        write(out, "registry-tags.json", tags);
        var nodes = new ArrayList<CommandNode<CommandSourceStack>>();
        var ids = new IdentityHashMap<CommandNode<CommandSourceStack>,Integer>();
        var queue = new ArrayDeque<CommandNode<CommandSourceStack>>();
        queue.add(dispatcher.getRoot());
        while (!queue.isEmpty()) {
            var node = queue.remove();
            if (ids.containsKey(node)) continue;
            ids.put(node, nodes.size()); nodes.add(node);
            queue.addAll(node.getChildren());
            if (node.getRedirect() != null) queue.add(node.getRedirect());
        }
        var tree = new JsonArray();
        for (var node : nodes) {
            var entry = new JsonObject();
            entry.addProperty("name", node.getName());
            entry.addProperty("kind", node instanceof RootCommandNode ? "root" : node instanceof LiteralCommandNode ? "literal" : "argument");
            entry.addProperty("executable", node.getCommand() != null);
            entry.addProperty("fork", node.isFork());
            entry.addProperty("restricted", !node.canUse(Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS)));
            var permissions = new JsonArray();
            for (int level = 0; level <= 4; level++) permissions.add(node.canUse(source(level)));
            entry.add("permissions", permissions);
            if (node.getRedirect() != null) entry.addProperty("redirect", ids.get(node.getRedirect()));
            var children = new JsonArray();
            node.getChildren().forEach(child -> children.add(ids.get(child)));
            entry.add("children", children);
            if (node instanceof ArgumentCommandNode<CommandSourceStack,?> arg) {
                var serialized = ArgumentUtils.serializeNodeToJson(dispatcher, node);
                entry.add("parser", serialized.get("parser"));
                if (serialized.has("properties")) entry.add("properties", serialized.get("properties"));
                var examples = new JsonArray(); arg.getType().getExamples().forEach(examples::add);
                entry.add("examples", examples);
                if (arg.getCustomSuggestions() != null) entry.addProperty("suggestions", SuggestionProviders.getName(arg.getCustomSuggestions()).toString());
            }
            tree.add(entry);
        }
        write(out, "tree.json", tree);
        var argumentTypes = new LinkedHashMap<String,ArgumentType<?>>();
        var propertiesByType = new LinkedHashMap<String,JsonObject>();
        for (var node : nodes) if (node instanceof ArgumentCommandNode<CommandSourceStack,?> argument) {
            var serialized = ArgumentUtils.serializeNodeToJson(dispatcher, node);
            var parser = serialized.get("parser").getAsString();
            argumentTypes.putIfAbsent(parser, argument.getType());
            propertiesByType.putIfAbsent(parser, serialized.has("properties") ? serialized.getAsJsonObject("properties") : new JsonObject());
        }
        argumentTypes.put("brigadier:long", LongArgumentType.longArg());
        argumentTypes.put("minecraft:angle", net.minecraft.commands.arguments.AngleArgument.angle());
        argumentTypes.put("minecraft:item_slots", net.minecraft.commands.arguments.SlotsArgument.slots());
        var suggestionVectors = new JsonArray();
        var staticSuggestions = Set.of("brigadier:bool", "minecraft:gamemode", "minecraft:entity_anchor", "minecraft:operation", "minecraft:hex_color", "minecraft:team_color", "minecraft:scoreboard_slot", "minecraft:template_mirror", "minecraft:template_rotation", "minecraft:heightmap", "minecraft:swing_animation", "minecraft:block_pos", "minecraft:column_pos", "minecraft:vec3", "minecraft:vec2");
        for (var entry : argumentTypes.entrySet()) if (staticSuggestions.contains(entry.getKey())) {
            for (var input : List.of("", "a", "r", "dark_", "~", "~ ", "1", "1 ", "1 2", "^", "^ ", "1 2 ")) {
                var builder = new com.mojang.brigadier.suggestion.SuggestionsBuilder(input, 0);
                var commandContext = new com.mojang.brigadier.context.CommandContextBuilder<>(dispatcher, source(4), dispatcher.getRoot(), 0).build(input);
                var suggestions = entry.getValue().listSuggestions(commandContext, builder).join();
                var vector = new JsonObject();
                vector.addProperty("parser", entry.getKey()); vector.addProperty("input", input);
                var values = new JsonArray();
                suggestions.getList().forEach(suggestion -> values.add(suggestion.getText()));
                vector.add("suggestions", values); suggestionVectors.add(vector);
            }
        }
        write(out, "suggestion-vectors.json", suggestionVectors);
        var wireVectors = new JsonArray();
        for (var entry : argumentTypes.entrySet()) {
            var template = ArgumentTypeInfos.unpack(entry.getValue());
            var buf = new FriendlyByteBuf(Unpooled.buffer());
            buf.writeVarInt(BuiltInRegistries.COMMAND_ARGUMENT_TYPE.getId(template.type()));
            ((ArgumentTypeInfo)template.type()).serializeToNetwork(template, buf);
            var bytes = new byte[buf.readableBytes()]; buf.readBytes(bytes); buf.release();
            var vector = new JsonObject();
            vector.addProperty("parser", entry.getKey());
            vector.add("properties", propertiesByType.getOrDefault(entry.getKey(), new JsonObject()));
            vector.addProperty("wire", HexFormat.of().formatHex(bytes));
            wireVectors.add(vector);
        }
        write(out, "argument-wire.json", wireVectors);
        var vectors = new JsonArray();
        for (var entry : argumentTypes.entrySet()) {
            var inputs = new LinkedHashSet<String>(entry.getValue().getExamples());
            inputs.addAll(List.of("", " ", "?", "-1", "1.5", "~ ~ ~", "0 1", "0 1 2", "^ ^ ^", "~ ^ ~", "@a", "@e[type=minecraft:player,limit=1]"));
            if (entry.getKey().equals("minecraft:entity")) inputs.addAll(List.of("@", "@z", "@a[", "@a[limit=0]", "@a[x=1,x=2]", "@a[name=!A,name=B]", "@e[type=#minecraft:undead,type=minecraft:player]", "@a[distance=3..1]", "@a[scores={foo=1..2}]", "@a[advancements={minecraft:story/root={foo=true}}]"));
            if (entry.getKey().equals("minecraft:item_stack")) inputs.addAll(List.of("stone[]", "stone[damage=1]", "stone[!damage]", "stone[damage=1,damage=2]", "stone[creative_slot_lock={}]", "stone[unknown=1]", "stone[damage=]", "stone[damage=1,", "stone[ damage=1 ]", "stone[custom_name='hello']"));
            if (entry.getKey().equals("minecraft:particle")) {
                BuiltInRegistries.PARTICLE_TYPE.keySet().forEach(id -> { inputs.add(id.toString()); inputs.add(id+"{}"); });
                inputs.addAll(List.of("dust{color:[1f,0f,0.5f],scale:1f}", "dust{color:16711680,scale:0.001f}", "dust{color:1,scale:5f}", "dust{color:1,scale:'x'}", "dust{color:'red',scale:1f}", "dust_color_transition{from_color:1,to_color:2,scale:2f}", "geyser{water_blocks:3}", "geyser{water_blocks:0}", "geyser_base{water_blocks:3,burst_impulse_base:1f}", "dragon_breath{power:2.5f}", "effect{color:[0.5f,0.25f,1f],power:2f}", "entity_effect{color:[1f,0f,0f,0.5f]}", "sculk_charge{roll:0.5f}", "shriek{delay:-2}", "trail{target:[1d,2d,3d],color:1,duration:5}", "trail{target:[1d,2d],color:1,duration:0}", "item{item:'minecraft:stone'}", "item{item:{id:'minecraft:stone',count:2}}", "block{block_state:'minecraft:stone'}", "block{block_state:{Name:'minecraft:oak_log',Properties:{axis:'x'}}}", "block{block_state:{id:'minecraft:oak_log',properties:{axis:'x'}}}", "vibration{destination:{type:'minecraft:block',pos:[1,2,3]},arrival_in_ticks:5}", "flame{ignored:1}"));
            }
            if (entry.getKey().equals("minecraft:item_predicate")) inputs.addAll(List.of("*", "*[]", "stone[]", "stone[damage]", "stone[!damage]", "stone[damage=1]", "stone[count=1]", "*[count~{min:1,max:5}]", "*[damage~{damage:{min:1}}]", "*[damage|custom_name]", "*[!damage,custom_name='foo']", "*[custom_data~{foo:1}]", "*[creative_slot_lock]", "*[unknown]", "*[unknown~{}]", "*[damage=]", "*[damage", "*[damage,]", "*[damage|]", "*[!!damage]", "*[count={min:5,max:1}]", "*[count='a']", " stone [ damage = 1 ]", "*[count]"));
            if (entry.getKey().equals("minecraft:component")) inputs.addAll(List.of("{}", "[]", "[1]", "{text:1}", "{text:'x',extra:[]}", "{text:'x',bold:'yes'}", "{type:'unknown',text:'x'}", "{type:'text'}", "{translate:'chat.type.text',with:['A','B']}", "{selector:'@a',separator:', '}", "{text:'x',color:'red',bold:false,extra:[' y']}", "{text:'x',click_event:{action:'suggest_command',command:'/help'},insertion:'x'}"));
            if (entry.getKey().equals("minecraft:style")) inputs.addAll(List.of("[]", "'x'", "{bold:'yes'}", "{bold:2,italic:false}", "{color:'invalid'}", "{color:'#12abEF'}", "{insertion:1}", "{bold:'yes',italic:'no'}", "{font:'minecraft:default'}", "{shadow_color:[1f,0f,0.5f,1f]}", "{click_event:{action:'suggest_command',command:'/help'}}"));
            if (entry.getKey().equals("minecraft:objective_criteria")) inputs.addAll(List.of("dummy", "trigger", "health", "teamkill.red", "minecraft.mined:minecraft.stone", "minecraft.custom:minecraft.jump"));
            for (var input : inputs) {
                var reader = new com.mojang.brigadier.StringReader(input);
                var vector = new JsonObject();
                vector.addProperty("parser", entry.getKey());
                vector.add("properties", propertiesByType.getOrDefault(entry.getKey(), new JsonObject()));
                vector.addProperty("input", input);
                try {
                    Object value = entry.getValue().parse(reader);
                    vector.addProperty("ok", true);
                    if (value instanceof Number || value instanceof Boolean || value instanceof String) vector.add("value", JSON.toJsonTree(value));
                    if (value instanceof net.minecraft.core.particles.ParticleOptions particle) {
                        var bytes = new net.minecraft.network.RegistryFriendlyByteBuf(Unpooled.buffer(), net.minecraft.core.RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
                        net.minecraft.core.particles.ParticleTypes.STREAM_CODEC.encode(bytes, particle);
                        var encoded = new byte[bytes.readableBytes()]; bytes.readBytes(encoded); bytes.release();
                        vector.addProperty("particle_wire", HexFormat.of().formatHex(encoded));
                    }
                    if (value instanceof net.minecraft.network.chat.Component component) vector.add("value", net.minecraft.network.chat.ComponentSerialization.CODEC.encodeStart(runtimeRegistries.createSerializationContext(com.mojang.serialization.JsonOps.INSTANCE), component).getOrThrow());
                    if (value instanceof net.minecraft.network.chat.Style style) vector.add("value", net.minecraft.network.chat.Style.Serializer.CODEC.encodeStart(runtimeRegistries.createSerializationContext(com.mojang.serialization.JsonOps.INSTANCE), style).getOrThrow());
                    if (value instanceof net.minecraft.commands.arguments.coordinates.WorldCoordinates coordinates) {
                        var coordinatesJson = new JsonArray();
                        for (var coordinate : List.of(coordinates.x(), coordinates.y(), coordinates.z())) {
                            var c = new JsonObject(); c.addProperty("relative", coordinate.relative()); c.addProperty("value", coordinate.value()); coordinatesJson.add(c);
                        }
                        vector.add("world_coordinates", coordinatesJson);
                    }
                    if (value instanceof net.minecraft.commands.arguments.coordinates.LocalCoordinates coordinates) vector.add("local_coordinates", JSON.toJsonTree(List.of(coordinates.left(), coordinates.up(), coordinates.forwards())));
                } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
                    vector.addProperty("ok", false); vector.addProperty("error_cursor", e.getCursor()); vector.addProperty("message", e.getRawMessage().getString());
                } catch (RuntimeException e) {
                    vector.addProperty("runtime_exception", e.getClass().getName()); vector.addProperty("runtime_message", e.getMessage());
                }
                vector.addProperty("cursor", reader.getCursor());
                vectors.add(vector);
            }
        }
        write(out, "parser-vectors.json", vectors);
        var inspector = new ClientboundCommandsPacket.NodeInspector<CommandSourceStack>() {
            public Identifier suggestionId(ArgumentCommandNode<CommandSourceStack,?> node) {
                return node.getCustomSuggestions() == null ? null : SuggestionProviders.getName(node.getCustomSuggestions());
            }
            public boolean isExecutable(CommandNode<CommandSourceStack> node) { return node.getCommand() != null; }
            public boolean isRestricted(CommandNode<CommandSourceStack> node) {
                return !node.canUse(Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS));
            }
        };
        for (int level : new int[]{0,4}) {
            var buf = new FriendlyByteBuf(Unpooled.buffer());
            ClientboundCommandsPacket.STREAM_CODEC.encode(buf, new ClientboundCommandsPacket(filter(dispatcher.getRoot(), level), inspector));
            var bytes = new byte[buf.readableBytes()]; buf.readBytes(bytes); buf.release();
            Files.write(out.resolve("tree-level-" + level + ".bin"), bytes);
        }
        var enabled = net.minecraft.world.flag.FeatureFlagSet.of(FeatureFlags.VANILLA,FeatureFlags.MINECART_IMPROVEMENTS);
        var experimental = new Commands(Commands.CommandSelection.DEDICATED, CommandBuildContext.simple(runtimeRegistries, enabled));
        for (int level : new int[]{0,4}) {
            var buf = new FriendlyByteBuf(Unpooled.buffer());
            ClientboundCommandsPacket.STREAM_CODEC.encode(buf, new ClientboundCommandsPacket(filter(experimental.getDispatcher().getRoot(), level), inspector));
            var bytes = new byte[buf.readableBytes()]; buf.readBytes(bytes); buf.release();
            Files.write(out.resolve("tree-minecart-level-"+level+".bin"),bytes);
        }
        var savedRules = new JsonArray();
        for (var input : List.of("{}", "1", "[]", "{keep_inventory:true}", "{keep_inventory:0.5d}", "{keep_inventory:256}", "{keep_inventory:-0.5d}", "{'minecraft:keep_inventory':1b,'minecraft:random_tick_speed':2.9d}", "{'minecraft:keep_inventory':'true'}", "{'minecraft:keep_inventory':true,'minecraft:unknown':1}", "{'minecraft:keep_inventory':true,'minecraft:random_tick_speed':-1}", "{'minecraft:random_tick_speed':-1}", "{'minecraft:max_minecart_speed':0}", "{'minecraft:random_tick_speed':2147483648L}", "{'minecraft:random_tick_speed':-0.5d}")) {
            var tag = net.minecraft.nbt.TagParser.create(net.minecraft.nbt.NbtOps.INSTANCE).parseFully(input);
            var result = GameRuleMap.CODEC.parse(net.minecraft.nbt.NbtOps.INSTANCE,tag);
            var vector = new JsonObject();vector.addProperty("input",input);vector.addProperty("success",result.isSuccess());
            result.error().ifPresent(error -> vector.addProperty("error",error.message()));
            result.resultOrPartial(error -> {}).ifPresent(value -> vector.add("partial",GameRuleMap.CODEC.encodeStart(com.mojang.serialization.JsonOps.INSTANCE,value).getOrThrow()));
            savedRules.add(vector);
        }
        write(out,"saved-rule-vectors.json",savedRules);
        var ruleParsing = new JsonArray();
        for (var rule : BuiltInRegistries.GAME_RULE) {
            var values = new LinkedHashSet<>(List.of("", " ", "true", "false", "TRUE", "true false", "0", "-1", "1", "1.5", "0001", "2147483647", "2147483648"));
            if (rule.argument() instanceof IntegerArgumentType integer) {
                values.add(Integer.toString(integer.getMinimum()));
                values.add(Long.toString((long)integer.getMinimum()-1));
                values.add(Integer.toString(integer.getMaximum()));
                values.add(Long.toString((long)integer.getMaximum()+1));
            }
            for (var input : values) {
                var result = rule.deserialize(input);
                var vector = new JsonObject(); vector.addProperty("rule", rule.getIdentifier().toString()); vector.addProperty("input", input);
                result.result().ifPresent(value -> vector.add("value", JSON.toJsonTree(value)));
                result.error().ifPresent(failure -> {
                    vector.addProperty("error", failure.message());
                    failure.partialValue().ifPresent(value -> vector.add("partial", JSON.toJsonTree(value)));
                });
                ruleParsing.add(vector);
            }
        }
        write(out, "rule-parsing.json", ruleParsing);
        var rules = new JsonArray();
        for (var rule : BuiltInRegistries.GAME_RULE) {
            var entry = new JsonObject();
            entry.addProperty("name", rule.getIdentifier().toString());
            entry.addProperty("category", rule.category().id().toString());
            entry.add("default", JSON.toJsonTree(rule.defaultValue()));
            entry.addProperty("enabled", rule.isEnabled(FeatureFlags.DEFAULT_FLAGS));
            if (rule.argument() instanceof IntegerArgumentType integer) {
                entry.addProperty("min", integer.getMinimum()); entry.addProperty("max", integer.getMaximum());
            }
            rules.add(entry);
        }
        write(out, "rules.json", rules);
        write(out, "level-unions.json", List.of(
            ((LevelBasedPermissionSet)LevelBasedPermissionSet.OWNER.union(LevelBasedPermissionSet.ALL)).level().id(),
            ((LevelBasedPermissionSet)LevelBasedPermissionSet.ALL.union(LevelBasedPermissionSet.OWNER)).level().id()));
    }
}
