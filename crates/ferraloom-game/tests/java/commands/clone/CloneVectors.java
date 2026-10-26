import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.function.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.CloneCommands;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.pattern.BlockInWorld;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.ticks.LevelTicks;
import static org.mockito.Mockito.*;

/** Invoke the real CloneCommands engine, controlling only its live level receivers. */
public final class CloneVectors {
    record Case(String mode, boolean strict, String filter, int destination, int limit, boolean loaded, boolean debug, boolean changed, boolean entity) { Case(String mode,boolean strict,String filter,int destination,int limit,boolean loaded,boolean debug,boolean changed){this(mode,strict,filter,destination,limit,loaded,debug,changed,false);} }
    static final Gson JSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static JsonArray point(BlockPos p) {var a=new JsonArray();a.add(p.getX());a.add(p.getY());a.add(p.getZ());return a;}
    static JsonObject event(String name, BlockPos p) {var e=new JsonObject();e.addProperty("call",name);e.add("pos",point(p));return e;}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var registries=net.minecraft.core.RegistryAccess.EMPTY;
        Class<?> pair=Class.forName("net.minecraft.server.commands.CloneCommands$DimensionAndPosition");
        var constructor=pair.getDeclaredConstructors()[0];constructor.setAccessible(true);
        Class<? extends Enum> modeType=(Class<? extends Enum>)Class.forName("net.minecraft.server.commands.CloneCommands$Mode");
        var method=CloneCommands.class.getDeclaredMethod("clone",CommandSourceStack.class,pair,pair,pair,Predicate.class,modeType,boolean.class);method.setAccessible(true);
        var rows=new JsonArray();
        var cases=new ArrayList<Case>();
        for(var mode:List.of("NORMAL","FORCE","MOVE")) for(boolean strict:List.of(false,true)) cases.add(new Case(mode,strict,"all",8,100,true,false,true));
        cases.addAll(List.of(new Case("NORMAL",false,"masked",8,100,true,false,true),new Case("NORMAL",false,"stone",8,100,true,false,true),new Case("NORMAL",false,"none",8,100,true,false,true),new Case("NORMAL",false,"all",1,0,false,true,true),new Case("FORCE",false,"all",8,2,false,true,true),new Case("FORCE",false,"all",8,100,false,true,true),new Case("NORMAL",false,"all",8,100,true,true,true),new Case("NORMAL",false,"all",8,100,true,false,false),new Case("MOVE",false,"all",1,100,true,false,true)));
        cases.add(new Case("NORMAL",false,"all",8,100,true,false,true,true));
        cases.add(new Case("MOVE",true,"all",8,100,true,false,true,true));
        for(var test:cases) {
            var row=JSON.toJsonTree(test).getAsJsonObject();var trace=new JsonArray();var level=mock(ServerLevel.class);var source=mock(CommandSourceStack.class);var rules=mock(GameRules.class);var ticks=mock(LevelTicks.class);
            doReturn(registries).when(source).registryAccess();doReturn(registries).when(level).registryAccess();doReturn(true).when(level).hasChunkAt(any());doReturn(level).when(source).getLevel();doReturn(rules).when(level).getGameRules();doReturn(test.limit).when(rules).get(GameRules.MAX_BLOCK_MODIFICATIONS);doReturn(ticks).when(level).getBlockTicks();
            doAnswer(i->{var e=event("loaded",i.getArgument(0));e.add("end",point(i.getArgument(1)));trace.add(e);return test.loaded;}).when(level).hasChunksAt(any(BlockPos.class),any(BlockPos.class));
            doAnswer(i->{trace.add("debug");return test.debug;}).when(level).isDebug();
            var stone=mock(BlockState.class);var torch=mock(BlockState.class);var air=mock(BlockState.class);
            doReturn(true).when(stone).isSolidRender();doReturn(false).when(torch).isSolidRender();doReturn(false).when(torch).isCollisionShapeFullBlock(any(),any());doReturn(true).when(air).isAir();
            var states=Map.of(0,stone,1,torch,2,air);
            doAnswer(i->{BlockPos p=i.getArgument(0);trace.add(event("state",p));return states.getOrDefault(p.getX(),air);}).when(level).getBlockState(any());
            var be=mock(net.minecraft.world.level.block.entity.BlockEntity.class);
            doReturn(net.minecraft.util.ProblemReporter.Collector.EMPTY_ROOT).when(be).problemPath();
            doReturn(net.minecraft.core.component.DataComponentMap.EMPTY).when(be).components();
            doAnswer(i->{trace.add("save_custom");((net.minecraft.world.level.storage.ValueOutput)i.getArgument(0)).putInt("value",42);return null;}).when(be).saveCustomOnly(any(net.minecraft.world.level.storage.ValueOutput.class));
            doAnswer(i->{trace.add("load_custom");int value=((net.minecraft.world.level.storage.ValueInput)i.getArgument(0)).getIntOr("value",-1);if(value!=42)throw new AssertionError(value);return null;}).when(be).loadCustomOnly(any());
            doAnswer(i->{trace.add("components");return null;}).when(be).setComponents(any());
            doAnswer(i->{trace.add("changed");return null;}).when(be).setChanged();
            doAnswer(i->{BlockPos p=i.getArgument(0);trace.add(event("entity",p));return test.entity&&(p.getX()==1||p.getX()==test.destination+1)?be:null;}).when(level).getBlockEntity(any());
            Predicate<BlockInWorld> filter=b->{var e=event("filter",b.getPos());trace.add(e);return switch(test.filter){case "masked"->!b.getState().isAir();case "stone"->b.getState()==stone;case "none"->false;default->true;};};
            doAnswer(i->{var e=event("set",i.getArgument(0));BlockState s=i.getArgument(1);e.addProperty("state",s==stone?"stone":s==torch?"torch":s==air?"air":s==Blocks.BARRIER.defaultBlockState()?"barrier":"air");e.addProperty("flags",(int)i.getArgument(2));trace.add(e);return test.changed;}).when(level).setBlock(any(),any(),anyInt());
            doAnswer(i->{trace.add(event("neighbors",i.getArgument(0)));return null;}).when(level).updateNeighboursOnBlockSet(any(),any());
            doAnswer(i->{var e=event("ticks",i.getArgument(2));trace.add(e);return null;}).when(ticks).copyAreaFrom(any(),any(),any());
            doAnswer(i->{trace.add("success");var c=((Supplier<Component>)i.getArgument(0)).get();row.addProperty("feedback",c.getString());row.addProperty("broadcast",(boolean)i.getArgument(1));return null;}).when(source).sendSuccess(any(),anyBoolean());
            try {row.addProperty("value",(int)method.invoke(null,source,constructor.newInstance(level,new BlockPos(0,80,0)),constructor.newInstance(level,new BlockPos(2,80,0)),constructor.newInstance(level,new BlockPos(test.destination,80,0)),filter,Enum.valueOf(modeType,test.mode),test.strict));}
            catch(InvocationTargetException error){var cause=error.getCause();if(cause instanceof com.mojang.brigadier.exceptions.CommandSyntaxException syntax)row.addProperty("error",syntax.getRawMessage().getString());else throw error;}
            row.add("trace",trace);rows.add(row);
        }
        Files.writeString(Path.of(args[0]),JSON.toJson(rows)+"\n");System.out.println("Captured "+rows.size()+" native clone cases");
    }
}
