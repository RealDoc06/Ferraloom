import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.arguments.blocks.BlockStateArgument;
import net.minecraft.core.BlockPos;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.*;
import static org.mockito.Mockito.*;

/** Actual BlockInput.place with native block entities and traced world notifications. */
public final class BlockInputVectors {
    record Case(String block, String initial, String input, boolean setResult, boolean entity, String pending) {
        Case(String block,String initial,String input,boolean setResult,boolean entity) { this(block,initial,input,setResult,entity,null); }
    }
    static <T> net.minecraft.core.Registry<T> copyRegistry(net.minecraft.core.HolderLookup.RegistryLookup<T> lookup) {
        var registry=new net.minecraft.core.MappedRegistry<T>((net.minecraft.resources.ResourceKey)lookup.key(),com.mojang.serialization.Lifecycle.stable());
        lookup.listElements().forEach(h->net.minecraft.core.Registry.register(registry,h.key(),h.value()));
        return registry.freeze();
    }
    static CompoundTag tag(String value) throws Exception { return TagParser.parseCompoundFully(value); }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var lookup=VanillaRegistries.createWorldLookup();
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        var builtin=net.minecraft.core.RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY);
        var access=new net.minecraft.core.RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(
            builtin.registries().map(e->e.value()),
            java.util.stream.Stream.of(copyRegistry(lookup.lookupOrThrow(net.minecraft.core.registries.Registries.BANNER_PATTERN)))) .toList());
        var context=CommandBuildContext.simple(lookup,FeatureFlags.DEFAULT_FLAGS);
        var pos=new BlockPos(8,5,8);
        var output=new JsonArray();
        for(var test:List.of(
            new Case("chest","{}","{}",false,true),
            new Case("chest","{}","{unknown:1,id:bad,x:99,y:99,z:99}",false,true),
            new Case("chest","{}","{CustomName:'named',Items:[{Slot:0b,id:'minecraft:stone',count:3}]}",true,true),
            new Case("chest","{Items:[{Slot:0b,id:'minecraft:stone',count:3}]}","{}",false,true),
            new Case("chest","{components:{'minecraft:custom_data':{before:1b}}}","{}",false,true),
            new Case("chest","{}","{components:{'minecraft:custom_data':{after:2b}}}",false,true),
            new Case("chest","{CustomName:'same'}","{CustomName:'same'}",false,true),
            new Case("oak_sign","{}","{front_text:{messages:['alpha','beta','',''],color:red,has_glowing_text:true},is_waxed:true}",true,true),
            new Case("oak_sign","{is_waxed:true}","{}",false,true),
            new Case("white_banner","{}","{patterns:[{pattern:'minecraft:stripe_bottom',color:red}],CustomName:'banner'}",true,true,"BannerBlockEntity factory not ported"),
            new Case("white_banner","{patterns:[{pattern:'minecraft:stripe_bottom',color:red}]}","{}",false,true,"BannerBlockEntity factory not ported"),
            new Case("spawner","{}","{Delay:100s,MinSpawnDelay:20,MaxSpawnDelay:30,SpawnCount:2,RequiredPlayerRange:0,SpawnRange:1}",true,true),
            new Case("spawner","{Delay:100s}","{Delay:100s}",false,true),
            new Case("spawner","{}","{SpawnData:{entity:{id:'minecraft:pig'}}}",false,true,"spawner loadWithComponents SpawnData callback"),
            new Case("stone","{}","{CustomName:'ignored'}",true,false),
            new Case("stone","{}","{CustomName:'ignored'}",false,false)
        )) {
            var state=BlockStateArgument.block(context).parse(new com.mojang.brigadier.StringReader(test.block)).getState();
            var level=mock(ServerLevel.class); var chunks=mock(ServerChunkCache.class); var trace=new JsonArray();
            doReturn(access).when(level).registryAccess(); doReturn(chunks).when(level).getChunkSource();
            doReturn(state).when(level).getBlockState(pos);
            doAnswer(i->{trace.add("send");return null;}).when(level).sendBlockUpdated(eq(pos),eq(state),eq(state),anyInt());
            BlockEntity receiver=null;
            if(test.entity) {
                receiver=spy(((EntityBlock)state.getBlock()).newBlockEntity(pos,state));
                receiver.loadWithComponents(TagValueInput.create(net.minecraft.util.ProblemReporter.DISCARDING,access,tag(test.initial)));
                receiver.setLevel(level);
                var captured=receiver;
                doAnswer(i->{trace.add("save");return i.callRealMethod();}).when(captured).saveWithoutMetadata(any(ValueOutput.class));
                doAnswer(i->{trace.add("load");return i.callRealMethod();}).when(captured).loadWithComponents(any(ValueInput.class));
                doAnswer(i->{trace.add("changed");return i.callRealMethod();}).when(captured).setChanged();
            }
            doReturn(receiver).when(level).getBlockEntity(pos);
            doAnswer(i->{trace.add("set");return test.setResult;}).when(level).setBlock(eq(pos),eq(state),eq(818));
            doAnswer(i->{trace.add("block_changed");return null;}).when(chunks).blockChanged(pos);
            var row=new Gson().toJsonTree(test).getAsJsonObject();
            var before=receiver==null?null:receiver.saveWithoutMetadata(access); trace.asList().clear();
            // The comparison saves happen inside the real BlockInput method.
            final var calls=trace;
            var block=BlockStateArgument.block(context).parse(new com.mojang.brigadier.StringReader(test.block+test.input));
            row.addProperty("affected",block.place(level,pos,818));row.add("trace",calls.deepCopy());
            if(receiver!=null) {
                row.addProperty("before",before.toString());
                row.addProperty("after",receiver.saveWithoutMetadata(access).toString());
                row.addProperty("full",receiver.saveWithFullMetadata(access).toString());
                var packet=receiver.getUpdatePacket();
                if(packet instanceof net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket data)row.addProperty("packet",data.getTag().toString());
            }
            output.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(output)+"\n");
        System.out.println("Captured "+output.size()+" native BlockInput cases");
    }
}
