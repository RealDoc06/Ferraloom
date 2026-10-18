import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.arguments.blocks.*;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.pattern.BlockInWorld;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import static org.mockito.Mockito.*;

/** Real command/BlockInput methods with a controlled world and traced level calls. */
public final class PlacementVector {
    record Case(String command,int limit,boolean debug,int shape) {
        Case(String command) {this(command,32768,false,-1);}
    }
    static JsonArray trace;
    static void call(String kind,BlockPos pos,BlockState state,Integer flags) {
        var entry = new JsonObject();entry.addProperty("call",kind);
        var coordinates = new JsonArray();coordinates.add(pos.getX());coordinates.add(pos.getY());coordinates.add(pos.getZ());entry.add("pos",coordinates);
        if (state!=null) entry.addProperty("state",Block.getId(state));
        if (flags!=null) entry.addProperty("flags",flags);
        trace.add(entry);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        var context=CommandBuildContext.simple(registries,FeatureFlags.DEFAULT_FLAGS);
        var level=mock(ServerLevel.class);var source=mock(CommandSourceStack.class);
        var world=new HashMap<BlockPos,BlockState>();
        var rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);
        doReturn(level).when(source).getLevel();doReturn(rules).when(level).getGameRules();
        doAnswer(i -> world.getOrDefault((BlockPos)i.getArgument(0),Blocks.AIR.defaultBlockState())).when(level).getBlockState(any());
        doAnswer(i -> world.getOrDefault((BlockPos)i.getArgument(0),Blocks.AIR.defaultBlockState()).isAir()).when(level).isEmptyBlock(any());
        doAnswer(i -> {
            BlockPos pos=((BlockPos)i.getArgument(0)).immutable();BlockState state=i.getArgument(1);int flags=i.getArgument(2);
            call("set",pos,state,flags);var old=world.getOrDefault(pos,Blocks.AIR.defaultBlockState());
            if(state.isAir())world.remove(pos);else world.put(pos,state);return old!=state;
        }).when(level).setBlock(any(),any(),anyInt());
        doAnswer(i -> {
            BlockPos pos=((BlockPos)i.getArgument(0)).immutable();call("destroy",pos,null,null);
            return world.remove(pos)!=null;
        }).when(level).destroyBlock(any(),eq(true));
        doAnswer(i -> {call("neighbors",i.getArgument(0),i.getArgument(1),null);return null;}).when(level).updateNeighboursOnBlockSet(any(),any());
        var set=Arrays.stream(SetBlockCommand.class.getDeclaredMethods()).filter(m->m.getName().equals("setBlock")).findFirst().orElseThrow();set.setAccessible(true);
        var fill=Arrays.stream(FillCommand.class.getDeclaredMethods()).filter(m->m.getName().equals("fillBlocks")).findFirst().orElseThrow();fill.setAccessible(true);
        Class setMode=Arrays.stream(SetBlockCommand.class.getDeclaredClasses()).filter(c->c.getSimpleName().equals("Mode")).findFirst().orElseThrow();
        Class fillMode=Arrays.stream(FillCommand.class.getDeclaredClasses()).filter(c->c.getSimpleName().equals("Mode")).findFirst().orElseThrow();
        var vectors=new JsonArray();
        for(var test:List.of(
            new Case("setblock 0 1 0 stone"),new Case("setblock 0 1 0 stone"),
            new Case("setblock 1 1 0 oak_log[axis=x] strict"),new Case("setblock 1 1 0 oak_log[axis=x] strict"),
            new Case("setblock 0 1 0 stone keep"),new Case("setblock 2 1 0 stone keep"),
            new Case("setblock 0 1 0 air destroy"),new Case("setblock 0 1 0 air destroy"),
            new Case("fill 0 3 0 2 3 0 stone"),new Case("fill 0 3 0 2 3 0 stone"),
            new Case("fill 3 3 0 5 3 0 stone strict"),new Case("fill 0 3 0 5 3 0 stone keep"),
            new Case("fill 0 10 0 2 12 2 stone outline"),new Case("setblock 1 11 1 stone"),
            new Case("fill 0 10 0 2 12 2 stone hollow"),new Case("fill 0 10 0 2 12 2 stone hollow"),
            new Case("fill 0 20 0 0 20 0 stone destroy"),new Case("fill 0 20 0 0 20 0 stone destroy"),
            new Case("fill 0 3 0 5 3 0 air replace stone"),
            new Case("fill 0 3 0 5 3 0 stone",2,false,-1),
            new Case("setblock 0 30 0 stone",32768,true,-1),new Case("fill 0 30 0 1 30 0 stone",32768,true,-1),
            new Case("setblock 0 40 0 oak_log[axis=x]",32768,false,Block.getId(Blocks.OAK_LOG.defaultBlockState())),
            new Case("setblock 0 41 0 stone",32768,false,Block.getId(Blocks.AIR.defaultBlockState()))
        )) {
            var vector=new Gson().toJsonTree(test).getAsJsonObject();trace=new JsonArray();vector.add("trace",trace);
            doReturn(test.debug).when(level).isDebug();rules.set(GameRules.MAX_BLOCK_MODIFICATIONS,test.limit,null);
            var feedback=new JsonArray();vector.add("feedback",feedback);
            doAnswer(i -> {
                var entry=new JsonObject();entry.addProperty("broadcast",(boolean)i.getArgument(1));
                entry.add("message",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,((Supplier<Component>)i.getArgument(0)).get()).getOrThrow());feedback.add(entry);return null;
            }).when(source).sendSuccess(any(),anyBoolean());
            var words=test.command.split(" ");boolean isFill=words[0].equals("fill");int stateIndex=isFill?7:4;
            var block=BlockStateArgument.block(context).parse(new com.mojang.brigadier.StringReader(words[stateIndex]));
            var from=new BlockPos(Integer.parseInt(words[1]),Integer.parseInt(words[2]),Integer.parseInt(words[3]));
            String mode=words.length>stateIndex+1?words[stateIndex+1]:"replace";
            Predicate<BlockInWorld> predicate=mode.equals("keep")?b->b.getState().isAir():null;
            if (isFill && mode.equals("replace") && words.length>stateIndex+2) predicate=BlockPredicateArgument.blockPredicate(context).parse(new com.mojang.brigadier.StringReader(words[stateIndex+2]));
            String internal=Set.of("replace","keep","strict").contains(mode)?"REPLACE":mode.toUpperCase(Locale.ROOT);
            try(var blocks=mockStatic(Block.class,CALLS_REAL_METHODS)) {
                blocks.when(()->Block.updateFromNeighbourShapes(any(),eq(level),any())).thenAnswer(i->{
                    BlockState state=i.getArgument(0);BlockPos pos=i.getArgument(2);call("shapes",pos,state,null);
                    return test.shape<0?state:Block.stateById(test.shape);
                });
                try {
                    Object value;
                    if (isFill) {
                        var to=new BlockPos(Integer.parseInt(words[4]),Integer.parseInt(words[5]),Integer.parseInt(words[6]));
                        value=fill.invoke(null,source,BoundingBox.fromCorners(from,to),block,Enum.valueOf(fillMode,internal),predicate,mode.equals("strict"));
                    } else value=set.invoke(null,source,from,block,Enum.valueOf(setMode,internal),predicate,mode.equals("strict"));
                    vector.addProperty("value",(int)value);
                } catch(java.lang.reflect.InvocationTargetException failure) {
                    var error=(com.mojang.brigadier.exceptions.CommandSyntaxException)failure.getCause();vector.addProperty("error",error.getRawMessage().getString());
                }
            }
            var states=new JsonArray();world.entrySet().stream().sorted(Comparator.comparingInt((Map.Entry<BlockPos,BlockState> e)->e.getKey().getX()).thenComparingInt(e->e.getKey().getY()).thenComparingInt(e->e.getKey().getZ())).forEach(e->{
                var state=new JsonObject();var pos=new JsonArray();pos.add(e.getKey().getX());pos.add(e.getKey().getY());pos.add(e.getKey().getZ());state.add("pos",pos);state.addProperty("state",Block.getId(e.getValue()));states.add(state);
            });vector.add("blocks",states);vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
