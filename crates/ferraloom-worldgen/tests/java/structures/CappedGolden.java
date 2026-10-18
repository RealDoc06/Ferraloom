import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import org.mockito.Mockito;
import net.minecraft.core.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Capped finalization: positional shuffle, changed-NBT counting and settings RNG. */
public final class CappedGolden {
    static String rule(String predicate,String state,String modifier) {return "{processor_type:'minecraft:rule',rules:[{input_predicate:"+predicate+",location_predicate:{predicate_type:'minecraft:always_true'},output_state:'minecraft:"+state+"'"+(modifier.isEmpty()?"":",block_entity_modifier:"+modifier)+"}]}";}
    static String cap(String delegate,String limit) {return "{processor_type:'minecraft:capped',delegate:"+delegate+",limit:"+limit+"}";}
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        String randomStone=rule("{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.4}","stone","{type:'minecraft:append_static',data:{touched:1}}");
        String loot=rule("{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.6}","suspicious_gravel","{type:'minecraft:append_loot',loot_table:'minecraft:archaeology/ocean_ruin_warm'}");
        String dirt=rule("{predicate_type:'minecraft:always_true'}","dirt","");String gold=rule("{predicate_type:'minecraft:always_true'}","gold_block","");
        String[] definitions={"["+cap(randomStone,"3")+"]","["+cap(dirt,"{type:'minecraft:uniform',min_inclusive:1,max_inclusive:6}")+"]","["+cap("{processor_type:'minecraft:nop'}","4")+"]","["+cap(gold,"1")+"]","["+cap(gold,"100")+"]","[{processor_type:'minecraft:block_ignore',blocks:['minecraft:air']},"+cap(randomStone,"2")+","+cap(dirt,"3")+"]","["+cap("{processor_type:'minecraft:block_ignore',blocks:['minecraft:stone']}","3")+"]","["+cap(cap(gold,"3"),"5")+"]","["+cap(loot,"4")+"]","["+cap("{processor_type:'minecraft:block_rot',integrity:0.5}","3")+"]"};
        var ops=RegistryOps.create(JsonOps.INSTANCE,registries);var out=new JsonArray();
        for(int scenario=0;scenario<12;scenario++)for(long seed:new long[]{0,-1,8675309})for(var rotation:net.minecraft.world.level.block.Rotation.values())for(var mirror:net.minecraft.world.level.block.Mirror.values())for(int rng=0;rng<3;rng++) {
            String definition=definitions[scenario<10?scenario:scenario==10?0:8];boolean sparseNbt=scenario>=10;
            var processors=new ArrayList<StructureProcessor>();for(var value:JsonParser.parseString(definition).getAsJsonArray())processors.add(StructureProcessorType.SINGLE_CODEC.parse(ops,value).getOrThrow());
            var blocks=new ArrayList<StructureTemplate.StructureBlockInfo>();for(int i=0;i<21;i++) {var tag=new CompoundTag();tag.putInt("keep",3);if(i%4==0)tag.putInt("touched",1);var block=i%5==0?Blocks.AIR:i%3==0?Blocks.DIRT:Blocks.STONE;blocks.add(new StructureTemplate.StructureBlockInfo(new BlockPos(i%7-3,i%3-1,i/7-1),block.defaultBlockState(),sparseNbt&&i%2!=0?null:tag));}
            org.mockito.stubbing.Answer<Object> forbidden=call->{throw new UnsupportedOperationException("Unapproved capped level call: "+call.getMethod());};var server=Mockito.mock(ServerLevel.class,forbidden);Mockito.doReturn(seed).when(server).getSeed();
            var level=(ServerLevelAccessor)java.lang.reflect.Proxy.newProxyInstance(CappedGolden.class.getClassLoader(),new Class<?>[]{ServerLevelAccessor.class},(proxy,method,args)->switch(method.getName()) {case "getLevel"->server;case "getBlockState"->Blocks.STONE.defaultBlockState();default->throw new UnsupportedOperationException(method.toString());});
            var shared=new LegacyRandomSource(1234);var fixed=new LegacyRandomSource(999);var settings=new StructurePlaceSettings().setRotation(rotation).setMirror(mirror).setRotationPivot(new BlockPos(1,0,2)).setBoundingBox(new BoundingBox(0,63,0,15,70,15));if(rng==1)settings.setRandom(shared);if(rng==2)settings.setRandom(fixed);for(var processor:processors)settings.addProcessor(processor);
            var position=new BlockPos(0,64,0);var reference=new BlockPos(7,65,-3);var processed=StructureTemplate.processBlockInfos(level,position,reference,settings,blocks);var results=new JsonArray();for(var block:processed) {var value=new JsonObject();value.add("pos",StructureGolden.ints(block.pos().getX(),block.pos().getY(),block.pos().getZ()));value.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(block.state()));if(block.nbt()!=null)value.addProperty("nbt",block.nbt().toString());results.add(value);}
            var item=new JsonObject();item.add("definition",JsonParser.parseString(definition));item.addProperty("sparse_nbt",sparseNbt);item.addProperty("seed",seed);item.addProperty("rotation",rotation.ordinal());item.addProperty("mirror",mirror.ordinal());item.addProperty("rng",rng);item.add("blocks",results);item.addProperty("next",shared.nextInt());if(rng==2)item.addProperty("fixed_next",fixed.nextInt());out.add(item);Mockito.framework().clearInlineMocks();
        }
        StructureGolden.write(directory.resolve("capped_processors.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
