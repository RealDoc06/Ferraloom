import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.core.*;
import net.minecraft.network.chat.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.PlaceCommand;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.permissions.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.StructureBlockEntity;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Real PlaceCommand parsing, settings, seed calls, loaded-area traversal and feedback. */
public final class PlaceVectors {
    static final Gson JSON=new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static final Vec3 SOURCE=new Vec3(80.25,64.5,-16.75);
    record Case(String command, boolean found, boolean placed, boolean hasChunk,
                boolean inWorld, List<Integer> missing, List<Integer> size, int permission) {
        Case(String command) {this(command,true,true,true,true,List.of(),List.of(17,2,18),4);}
    }
    static JsonElement component(Component value) {
        return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,value).getOrThrow();
    }
    static JsonElement pos(BlockPos pos) {return JSON.toJsonTree(List.of(pos.getX(),pos.getY(),pos.getZ()));}
    static JsonObject call(String name) {var value=new JsonObject();value.addProperty("call",name);return value;}
    static List<Case> cases() {
        var values=new ArrayList<Case>();
        for(String suffix:List.of(""," 0 80 0"," ~ ~1 ~-2"," ^1 ^2 ^3",
                " 0 80 0 clockwise_90"," 0 80 0 180 left_right",
                " 0 80 0 none none 0.5"," 0 80 0 none none 0.25 -42",
                " 0 80 0 counterclockwise_90 front_back 0.75 42 strict",
                " 0 80 0 none none 1 2147483647 strict"," 0 80 0 none none 0 -2147483648"))
            values.add(new Case("place template fixture:sample"+suffix));
        for(String rotation:List.of("none","clockwise_90","180","counterclockwise_90"))
            for(String mirror:List.of("none","front_back","left_right"))
                values.add(new Case("place template fixture:sample -1 80 -1 "+rotation+" "+mirror+" 1 42"));
        for(String command:List.of("place template fixture:sample 0 80 0 none none -0.1",
                "place template fixture:sample 0 80 0 none none 1.1",
                "place template fixture:sample 0 80 0 bad_rotation",
                "place template fixture:sample 0 80 0 none bad_mirror",
                "place template fixture:sample 0 80","place template Fixture:bad"))
            values.add(new Case(command));
        values.add(new Case("place template fixture:missing",false,true,true,true,List.of(),List.of(17,2,18),4));
        values.add(new Case("place template fixture:sample",true,false,true,true,List.of(),List.of(17,2,18),4));
        values.add(new Case("place template fixture:sample 0 80 0",true,true,false,false,List.of(),List.of(17,2,18),4));
        values.add(new Case("place template fixture:sample 0 80 0",true,true,true,false,List.of(),List.of(17,2,18),4));
        values.add(new Case("place template fixture:sample 0 80 0",true,true,true,true,List.of(16,0,0),List.of(17,2,18),4));
        values.add(new Case("place template fixture:sample 0 80 0 clockwise_90",true,true,true,true,List.of(16,0,16),List.of(17,2,18),4));
        values.add(new Case("place template fixture:sample",true,true,true,false,List.of(),List.of(0,0,0),4));
        values.add(new Case("place template fixture:sample 0 80 0",true,true,true,true,List.of(),List.of(-17,2,-18),4));
        values.add(new Case("place template fixture:sample",true,true,true,true,List.of(),List.of(17,2,18),1));
        return values;
    }
    static JsonArray capture(CommandBuildContext context) throws Exception {
        var rows=new JsonArray();
        for(var test:cases()) {
            var row=JSON.toJsonTree(test).getAsJsonObject();var trace=new JsonArray();var feedback=new JsonArray();
            var level=mock(ServerLevel.class);var source=mock(CommandSourceStack.class);
            doReturn(level).when(source).getLevel();doReturn(SOURCE).when(source).getPosition();
            doReturn(new Vec2(-20,35)).when(source).getRotation();
            doReturn(net.minecraft.commands.arguments.EntityAnchorArgument.Anchor.EYES).when(source).getAnchor();
            doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(test.permission))).when(source).permissions();
            doReturn(CommandResultCallback.EMPTY).when(source).callback();
            var actor=mock(Entity.class);doReturn(actor).when(source).getEntity();
            doAnswer(i->{trace.add("eye_height");return 1.25f;}).when(actor).getEyeHeight();
            var manager=mock(StructureTemplateManager.class);doReturn(manager).when(level).getStructureTemplateManager();
            var template=mock(StructureTemplate.class);
            doAnswer(i->{var c=call("template");c.addProperty("id",i.getArgument(0).toString());trace.add(c);return test.found?Optional.of(template):Optional.empty();}).when(manager).get(any());
            doAnswer(i->{trace.add("size");return new Vec3i(test.size.get(0),test.size.get(1),test.size.get(2));}).when(template).getSize();
            doAnswer(i->{var c=call("has_chunk");c.add("pos",pos(i.getArgument(0)));trace.add(c);return test.hasChunk;}).when(level).hasChunkAt(any());
            doAnswer(i->{var c=call("in_world");c.add("pos",pos(i.getArgument(0)));trace.add(c);return test.inWorld;}).when(level).isInWorldBounds(any());
            doAnswer(i->{BlockPos p=i.getArgument(0);var c=call("loaded");c.add("pos",pos(p));trace.add(c);return !test.missing.equals(List.of(p.getX(),p.getY(),p.getZ()));}).when(level).isLoaded(any());
            doAnswer(i->{
                BlockPos p=i.getArgument(1),ref=i.getArgument(2);StructurePlaceSettings s=i.getArgument(3);RandomSource r=i.getArgument(4);
                var c=call("place");c.add("pos",pos(p));c.add("reference",pos(ref));c.addProperty("rotation",s.getRotation().name());c.addProperty("mirror",s.getMirror().name());
                c.addProperty("known_shape",s.getKnownShape());c.addProperty("ignore_entities",s.isIgnoreEntities());c.addProperty("finalize_entities",s.shouldFinalizeEntities());c.addProperty("waterlogging",s.shouldApplyWaterlogging());c.addProperty("flags",(int)i.getArgument(5));
                var processors=new JsonArray();for(var processor:s.getProcessors()) processors.add(BlockRotProcessor.MAP_CODEC.codec().encodeStart(JsonOps.INSTANCE,(BlockRotProcessor)processor).getOrThrow());c.add("processors",processors);
                c.addProperty("settings_first_long",s.getRandom(p).nextLong());c.addProperty("placement_first_long",r.nextLong());trace.add(c);return test.placed;
            }).when(template).placeInWorld(eq(level),any(),any(),any(),any(),anyInt());
            doAnswer(i->{trace.add("success");var c=new JsonObject();c.addProperty("broadcast",(boolean)i.getArgument(1));c.add("message",component(((Supplier<Component>)i.getArgument(0)).get()));feedback.add(c);return null;}).when(source).sendSuccess(any(),anyBoolean());
            var dispatcher=new CommandDispatcher<CommandSourceStack>();PlaceCommand.register(dispatcher,context);
            try(var randomFactory=mockStatic(StructureBlockEntity.class,CALLS_REAL_METHODS)) {
                randomFactory.when(()->StructureBlockEntity.createRandom(anyLong())).thenAnswer(i->{long seed=i.getArgument(0);var c=call("random");c.addProperty("seed",seed);trace.add(c);return RandomSource.create(seed==0?42:seed);});
                try {row.addProperty("value",dispatcher.execute(test.command,source));}
                catch(CommandSyntaxException error) {row.addProperty("error",error.getRawMessage().getString());row.addProperty("cursor",error.getCursor());row.add("error_component",component(error.getRawMessage() instanceof Component c?c:Component.literal(error.getRawMessage().getString())));}
            }
            row.add("trace",trace);row.add("feedback",feedback);rows.add(row);
        }
        return rows;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var lookup=LootVectors.load();
        var rows=capture(CommandBuildContext.simple(lookup,FeatureFlags.DEFAULT_FLAGS));
        Files.writeString(Path.of(args[0]),JSON.toJson(rows)+"\n");System.out.println("Captured "+rows.size()+" native PlaceCommand template vectors");
        captureBlockEntityCallbacks(lookup,Path.of(args[1]));
    }
    static void captureBlockEntityCallbacks(HolderLookup.Provider lookup,Path output) throws Exception {
        var input=net.minecraft.nbt.TagParser.parseCompoundFully("{size:[2,1,1],palette:[{id:'minecraft:chest'}],blocks:[{pos:[0,0,0],state:0,nbt:{id:'minecraft:chest',LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:99L}},{pos:[1,0,0],state:0,nbt:{id:'minecraft:chest',LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:99L}}]}");
        var template=new StructureTemplate();template.load(lookup.lookupOrThrow(net.minecraft.core.registries.Registries.BLOCK),input);
        var trace=new JsonArray();var level=mock(ServerLevel.class);var origin=new BlockPos(8,5,8);
        var access=new RegistryAccess.ImmutableRegistryAccess(lookup.listRegistries().map(r->(Registry<?>)r).toList());
        doReturn(access).when(level).registryAccess();
        doReturn(net.minecraft.world.level.material.Fluids.EMPTY.defaultFluidState()).when(level).getFluidState(any());
        doAnswer(i->{var c=call("set");c.add("pos",pos(i.getArgument(0)));c.addProperty("state",Block.getId(i.getArgument(1)));c.addProperty("flags",(int)i.getArgument(2));trace.add(c);return true;}).when(level).setBlock(any(),any(),anyInt());
        var receivers=new HashMap<BlockPos,net.minecraft.world.level.block.entity.ChestBlockEntity>();
        for(int x=8;x<=9;x++) {
            var p=new BlockPos(x,5,8);var receiver=mock(net.minecraft.world.level.block.entity.ChestBlockEntity.class);receivers.put(p,receiver);
            doAnswer(i->{var c=call("load");c.add("pos",pos(p));net.minecraft.world.level.storage.ValueInput data=i.getArgument(0);c.addProperty("seed",data.getLongOr("LootTableSeed",0));trace.add(c);return null;}).when(receiver).loadWithComponents(any());
            doAnswer(i->{var c=call("changed");c.add("pos",pos(p));trace.add(c);return null;}).when(receiver).setChanged();
        }
        doAnswer(i->receivers.get(i.getArgument(0))).when(level).getBlockEntity(any());
        var random=RandomSource.create(42);var traced=mock(RandomSource.class);
        doAnswer(i->{long seed=random.nextLong();var c=call("random");c.addProperty("value",seed);trace.add(c);return seed;}).when(traced).nextLong();
        var placed=template.placeInWorld(level,origin,origin,new StructurePlaceSettings().setKnownShape(true),traced,818);
        var result=new JsonObject();result.addProperty("input",input.toString());result.addProperty("placed",placed);result.add("trace",trace);result.addProperty("next_long",random.nextLong());
        Files.writeString(output,JSON.toJson(result)+"\n");
    }
}
