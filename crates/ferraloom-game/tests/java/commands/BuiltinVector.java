import com.google.gson.*;
import com.mojang.authlib.GameProfile;
import com.mojang.brigadier.*;
import com.mojang.serialization.JsonOps;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.network.chat.*;
import net.minecraft.server.*;
import net.minecraft.server.commands.*;
import net.minecraft.server.level.*;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.GameRules;
import static org.mockito.Mockito.*;

/** Executes registered gamerule, seed and list commands with exact feedback components. */
public final class BuiltinVector {
    record Spec(String command,long seed,List<String> names) {}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        com.mojang.brigadier.exceptions.CommandSyntaxException.BUILT_IN_EXCEPTIONS=new BrigadierExceptions();
        var gson=new GsonBuilder().setPrettyPrinting().create();var vectors=new JsonArray();
        var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        var context=CommandBuildContext.simple(registries,FeatureFlags.DEFAULT_FLAGS);
        var rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);
        var dispatcher=new CommandDispatcher<CommandSourceStack>();GameRuleCommand.register(dispatcher,context);SeedCommand.register(dispatcher,true);ListPlayersCommand.register(dispatcher);
        var specs=new ArrayList<Spec>();
        for(long seed:new long[]{0L,-1L,4294967297L,Long.MIN_VALUE,Long.MAX_VALUE}) specs.add(new Spec("seed",seed,List.of()));
        for(var names:List.of(List.<String>of(),List.of("Doc"),List.of("Doc","Alex","Guest"))) for(String command:List.of("list","list uuids")) specs.add(new Spec(command,0L,names));
        for(String command:List.of("gamerule keep_inventory","gamerule keep_inventory true","gamerule minecraft:keep_inventory","gamerule keep_inventory true","gamerule random_tick_speed 7","gamerule random_tick_speed","gamerule max_snow_accumulation_height 9")) specs.add(new Spec(command,0L,List.of()));
        for(var spec:specs) {
            var source=mock(CommandSourceStack.class);var server=mock(MinecraftServer.class);var level=mock(ServerLevel.class);var list=mock(PlayerList.class);
            doReturn(server).when(source).getServer();doReturn(level).when(source).getLevel();doReturn(list).when(server).getPlayerList();doReturn(20).when(list).getMaxPlayers();
            doReturn(spec.seed).when(level).getSeed();doReturn(rules).when(level).getGameRules();
            doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4))).when(source).permissions();
            var players=new ArrayList<ServerPlayer>();
            for(String name:spec.names) {
                var player=mock(ServerPlayer.class,CALLS_REAL_METHODS);var uuid=UUID.nameUUIDFromBytes(("OfflinePlayer:"+name).getBytes(StandardCharsets.UTF_8));
                doReturn(new GameProfile(uuid,name)).when(player).getGameProfile();doReturn(Component.literal(name)).when(player).getName();doReturn(uuid).when(player).getUUID();doReturn(EntityTypes.PLAYER).when(player).getType();doReturn(null).when(player).getTeam();players.add(player);
            }
            doReturn(players).when(list).getPlayers();
            var feedback=new JsonArray();
            doAnswer(call->{var entry=new JsonObject();entry.add("message",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,((Supplier<Component>)call.getArgument(0)).get()).getOrThrow());entry.addProperty("broadcast",(boolean)call.getArgument(1));feedback.add(entry);return null;}).when(source).sendSuccess(any(),anyBoolean());
            var vector=new JsonObject();vector.addProperty("command",spec.command);vector.addProperty("seed",spec.seed);vector.add("players",gson.toJsonTree(spec.names));
            try { vector.addProperty("value",dispatcher.execute(spec.command,source)); }
            catch(com.mojang.brigadier.exceptions.CommandSyntaxException error) { vector.add("error",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,(Component)error.getRawMessage()).getOrThrow()); }
            vector.add("feedback",feedback);vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]),gson.toJson(vectors)+"\n");
    }
}
