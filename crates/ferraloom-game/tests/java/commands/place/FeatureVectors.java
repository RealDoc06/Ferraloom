import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.arguments.*;
import net.minecraft.core.*;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.PlaceCommand;
import net.minecraft.server.level.*;
import net.minecraft.server.permissions.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.levelgen.feature.Feature;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Real PlaceCommand with a controlled registered Feature#place receiver. */
public final class FeatureVectors {
    record Case(String command, boolean placed, boolean hasChunk, boolean inWorld, List<Integer> missing, int permission) {
        Case(String command) {this(command,true,true,true,List.of(),4);}
    }
    static List<Case> cases() {
        return List.of(new Case("place feature minecraft:oak"),new Case("place feature minecraft:oak 0 80 0"),
            new Case("place feature minecraft:oak ~ ~1 ~-2"),new Case("place feature minecraft:oak ^1 ^2 ^3"),
            new Case("place feature minecraft:oak 0 80 0",false,true,true,List.of(),4),
            new Case("place feature minecraft:oak 0 80 0",true,false,false,List.of(),4),
            new Case("place feature minecraft:oak 0 80 0",true,true,false,List.of(),4),
            new Case("place feature minecraft:oak 0 80 0",true,true,true,List.of(-16,0,-16),4),
            new Case("place feature minecraft:oak 0 80 0",true,true,true,List.of(16,0,16),4),
            new Case("place feature minecraft:oak",true,true,true,List.of(),1),
            new Case("place feature minecraft:unknown_native_feature"),new Case("place feature minecraft:oak 0 80"));
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var context=CommandBuildContext.simple(LootVectors.load(),FeatureFlags.DEFAULT_FLAGS);var rows=new JsonArray();
        for(var test:cases()) {
            var row=PlaceVectors.JSON.toJsonTree(test).getAsJsonObject();var trace=new JsonArray();var feedback=new JsonArray();
            var level=mock(ServerLevel.class);var source=mock(CommandSourceStack.class);var cache=mock(ServerChunkCache.class);var generator=mock(ChunkGenerator.class);var random=RandomSource.create(42);
            doReturn(level).when(source).getLevel();doReturn(PlaceVectors.SOURCE).when(source).getPosition();doReturn(new Vec2(-20,35)).when(source).getRotation();
            doReturn(EntityAnchorArgument.Anchor.EYES).when(source).getAnchor();doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(test.permission))).when(source).permissions();doReturn(CommandResultCallback.EMPTY).when(source).callback();
            var actor=mock(Entity.class);doReturn(actor).when(source).getEntity();doAnswer(i->{trace.add("eye_height");return 1.25f;}).when(actor).getEyeHeight();
            doReturn(cache).when(level).getChunkSource();doReturn(generator).when(cache).getGenerator();doReturn(random).when(level).getRandom();
            doAnswer(i->{var c=PlaceVectors.call("has_chunk");c.add("pos",PlaceVectors.pos(i.getArgument(0)));trace.add(c);return test.hasChunk;}).when(level).hasChunkAt(any());
            doAnswer(i->{var c=PlaceVectors.call("in_world");c.add("pos",PlaceVectors.pos(i.getArgument(0)));trace.add(c);return test.inWorld;}).when(level).isInWorldBounds(any());
            doAnswer(i->{BlockPos p=i.getArgument(0);var c=PlaceVectors.call("loaded");c.add("pos",PlaceVectors.pos(p));trace.add(c);return !test.missing.equals(List.of(p.getX(),p.getY(),p.getZ()));}).when(level).isLoaded(any());
            var feature=mock(Feature.class);var holder=mock(Holder.Reference.class);doReturn(feature).when(holder).value();doReturn(Optional.of("minecraft:oak")).when(holder).getRegisteredNameIfPresent();
            doAnswer(i->{var c=PlaceVectors.call("feature");c.addProperty("id","minecraft:oak");c.add("pos",PlaceVectors.pos(i.getArgument(3)));c.addProperty("draw",((RandomSource)i.getArgument(2)).nextLong());trace.add(c);return test.placed;}).when(feature).place(eq(level),eq(generator),eq(random),any());
            doAnswer(i->{trace.add("success");var c=new JsonObject();c.addProperty("broadcast",(boolean)i.getArgument(1));c.add("message",PlaceVectors.component(((Supplier<Component>)i.getArgument(0)).get()));feedback.add(c);return null;}).when(source).sendSuccess(any(),anyBoolean());
            var dispatcher=new CommandDispatcher<CommandSourceStack>();PlaceCommand.register(dispatcher,context);
            try(var arguments=mockStatic(ResourceOrIdArgument.class,CALLS_REAL_METHODS)) {
                arguments.when(()->ResourceOrIdArgument.getFeature(any(),eq("feature"))).thenReturn(holder);
                try {row.addProperty("value",dispatcher.execute(test.command,source));}
                catch(CommandSyntaxException error) {row.addProperty("error",error.getRawMessage().getString());row.addProperty("cursor",error.getCursor());row.add("error_component",PlaceVectors.component(error.getRawMessage() instanceof Component c?c:Component.literal(error.getRawMessage().getString())));}
            }
            row.add("trace",trace);row.add("feedback",feedback);row.addProperty("next_long",random.nextLong());rows.add(row);
        }
        Files.writeString(Path.of(args[0]),PlaceVectors.JSON.toJson(rows)+"\n");System.out.println("Captured "+rows.size()+" native feature placement cases");
    }
}
