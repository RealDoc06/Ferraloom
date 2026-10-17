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
        var registries = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p -> p.apply());
        var context = CommandBuildContext.simple(registries, FeatureFlags.DEFAULT_FLAGS);
        var commands = new Commands(Commands.CommandSelection.DEDICATED, context);
        var dispatcher = commands.getDispatcher();
        var out = Path.of(args[0]);
        Files.createDirectories(out);
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
