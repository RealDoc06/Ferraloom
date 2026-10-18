import com.google.gson.*;
import com.mojang.serialization.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.pools.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Real ChunkGenerator status methods with small authored structure generators. */
public final class StructureStatusGolden {
    static final class Elements {static final StructurePoolElement RIGID=StructurePoolElement.single("test:probe").apply(StructureTemplatePool.Projection.RIGID);}
    static final class Probe extends Structure {
        final int index;final String id;final List<String> calls;
        Probe(int index,Holder<Biome> biome,List<String> calls) {super(new StructureSettings(HolderSet.direct(biome),Map.of(),GenerationStep.Decoration.SURFACE_STRUCTURES,TerrainAdjustment.values()[index]));this.index=index;this.id="test:"+(char)('a'+index);this.calls=calls;}
        public Optional<GenerationStub> findGenerationPoint(GenerationContext context) {
            calls.add(id);var random=context.random();if(random.nextInt(5)==index)return Optional.empty();
            var p=new BlockPos(context.chunkPos().getMinBlockX()+random.nextInt(16),12+random.nextInt(17),context.chunkPos().getMinBlockZ()+random.nextInt(16));
            return Optional.of(new GenerationStub(p,b->{int width=8+random.nextInt(40);int depth=8+random.nextInt(40);var rotation=net.minecraft.world.level.block.Rotation.values()[random.nextInt(4)];b.addPiece(new PoolElementStructurePiece(context.structureTemplateManager(),Elements.RIGID,p,index-1,rotation,new BoundingBox(p.getX(),p.getY(),p.getZ(),p.getX()+width-1,p.getY()+7,p.getZ()+depth-1),index%2==0?LiquidSettings.APPLY_WATERLOGGING:LiquidSettings.IGNORE_WATERLOGGING));}));
        }
        public StructureType<?> type(){return StructureType.JIGSAW;}
    }
    static StructureStart initial(Probe structure,ChunkPos p,boolean valid) {
        var position=new BlockPos(p.getMinBlockX()+1,20,p.getMinBlockZ()+2);
        var pieces=valid?List.<StructurePiece>of(new PoolElementStructurePiece(null,Elements.RIGID,position,1,net.minecraft.world.level.block.Rotation.NONE,new BoundingBox(position.getX(),20,position.getZ(),position.getX()+5,27,position.getZ()+5),LiquidSettings.APPLY_WATERLOGGING)):List.<StructurePiece>of();
        return new StructureStart(structure,p,valid?2:9,new PiecesContainer(pieces));
    }
    static void capture(RegistryAccess.Frozen base,Path directory)throws Exception {
        var calls=new ArrayList<String>();var biomes=base.lookupOrThrow(Registries.BIOME);var plains=biomes.getOrThrow(Biomes.PLAINS);
        var structures=new MappedRegistry<Structure>(Registries.STRUCTURE,Lifecycle.stable());var probes=new ArrayList<Probe>();
        for(int i=0;i<5;i++){var probe=new Probe(i,i==4?biomes.getOrThrow(Biomes.OCEAN):plains,calls);probes.add(probe);Registry.register(structures,Identifier.parse(probe.id),probe);}
        var frozen=structures.freeze();
        var registryAccess=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(base.registries().filter(e->!e.key().equals(Registries.STRUCTURE)),java.util.stream.Stream.of(new RegistryAccess.RegistryEntry<>(Registries.STRUCTURE,frozen)))).freeze();
        var input=JsonParser.parseString("[{id:'test:first',structures:[{structure:'test:a',weight:1},{structure:'test:b',weight:2},{structure:'test:c',weight:4}],placement:{type:'minecraft:random_spread',spacing:3,separation:1,salt:1234,frequency:0.8}},{id:'test:second',structures:[{structure:'test:b',weight:2},{structure:'test:d',weight:3}],placement:{type:'minecraft:random_spread',spacing:2,separation:1,salt:456}},{id:'test:third',structures:[{structure:'test:d',weight:1}],placement:{type:'minecraft:dimension_origin'}},{id:'test:ocean',structures:[{structure:'test:e',weight:1}],placement:{type:'minecraft:random_spread',spacing:1,separation:0,salt:55}}]").getAsJsonArray();
        var overrides=new ArrayList<Holder<StructureSet>>();var setNames=new IdentityHashMap<StructureSet,String>();
        for(var e:input){var set=StructureSet.DIRECT_CODEC.parse(RegistryOps.create(JsonOps.INSTANCE,registryAccess),e).getOrThrow();overrides.add(Holder.direct(set));setNames.put(set,e.getAsJsonObject().get("id").getAsString());}
        var generator=new FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,base.lookupOrThrow(Registries.STRUCTURE_SET),base.lookupOrThrow(Registries.PLACED_FEATURE)));
        var noise=base.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));var containers=PalettedContainerFactory.create(registryAccess);
        var context=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registryAccess,null);
        var pack=net.minecraft.world.level.chunk.storage.SerializableChunkData.class.getDeclaredMethod("packStructureData",StructurePieceSerializationContext.class,ChunkPos.class,Map.class,Map.class);pack.setAccessible(true);
        var out=new JsonArray();
        for(long seed:new long[]{0,-1,8675309,Long.MIN_VALUE,Long.MAX_VALUE}) {
            var state=ChunkGeneratorStructureState.createForFlat(RandomState.create(base.lookupOrThrow(Registries.NOISE),seed,noise),seed,new ChunkPos(-2,3),generator.getBiomeSource(),overrides.stream());
            var manager=new StructureManager(null,new WorldOptions(seed,true,false),null);var chunks=new HashMap<Long,ProtoChunk>();var results=new JsonArray();
            for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++){var p=new ChunkPos(x,z);var chunk=new ProtoChunk(p,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),containers,null);chunks.put(p.pack(),chunk);}
            for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++) {
                var p=new ChunkPos(x,z);var chunk=chunks.get(p.pack());if(Math.floorMod(x-z,7)==0)chunk.setStartForStructure(probes.get(2),initial(probes.get(2),p,true));else if(Math.floorMod(x+z,5)==0)chunk.setStartForStructure(probes.get(0),initial(probes.get(0),p,false));
                calls.clear();generator.createStructures(registryAccess,state,manager,chunk,null,Level.OVERWORLD);
                var item=new JsonObject();item.add("position",StructureGolden.ints(x,z));var trace=new JsonArray();calls.forEach(trace::add);item.add("calls",trace);item.addProperty("starts",((CompoundTag)pack.invoke(null,context,p,chunk.getAllStarts(),chunk.getAllReferences())).getCompoundOrEmpty("starts").toString());results.add(item);
            }
            var level=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(StructureStatusGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->{if(method.getName().equals("getChunk")){var chunk=chunks.get(ChunkPos.pack((Integer)args[0],(Integer)args[1]));if(chunk==null)throw new IllegalStateException("missing authored dependency");return chunk;}throw new UnsupportedOperationException(method.toString());});
            var references=new JsonArray();for(int x=-8;x<=8;x++)for(int z=-8;z<=8;z++){var p=new ChunkPos(x,z);var chunk=chunks.get(p.pack());generator.createReferences(level,manager,chunk);generator.createReferences(level,manager,chunk);var item=new JsonObject();item.add("position",StructureGolden.ints(x,z));item.addProperty("references",((CompoundTag)pack.invoke(null,context,p,chunk.getAllStarts(),chunk.getAllReferences())).getCompoundOrEmpty("References").toString());references.add(item);}
            var regionManager=new StructureManager(level,new WorldOptions(seed,true,false),null);
            var beardCases=new JsonArray();
            for(int[] coordinate:new int[][]{{0,0},{-3,4},{2,-1}}) {
                var p=new ChunkPos(coordinate[0],coordinate[1]);var chunk=chunks.get(p.pack());
                var beard=Beardifier.forStructuresInChunk(regionManager,p);var sample=new JsonObject();sample.add("position",StructureGolden.ints(coordinate));
                var order=new JsonArray();chunk.getAllReferences().keySet().forEach(structure->order.add(registryAccess.lookupOrThrow(Registries.STRUCTURE).getKey(structure).toString()));sample.add("order",order);
                var values=new JsonArray();for(int z=p.getMinBlockZ();z<p.getMinBlockZ()+16;z+=3)for(int x=p.getMinBlockX();x<p.getMinBlockX()+16;x+=3)for(int y=0;y<=48;y+=3)values.add(Integer.toUnsignedLong(Float.floatToRawIntBits(beard.sampleValue(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.EMPTY_UNCACHED,x,y,z))));sample.add("values",values);beardCases.add(sample);
            }
            var item=new JsonObject();item.addProperty("seed",seed);item.add("sets",input);item.add("chunks",results);item.add("references",references);item.add("beard",beardCases);var possible=new JsonArray();state.possibleStructureSets().forEach(s->possible.add(setNames.get(s.value())));item.add("possible_sets",possible);out.add(item);
        }
        StructureGolden.write(directory.resolve("structure_statuses.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
