import com.google.gson.*;
import java.nio.file.*;
import java.io.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.network.chat.Component;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.chunk.ChunkGeneratorStructureState;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.structure.placement.*;
import net.minecraft.world.level.levelgen.densityfunction.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pools.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;
import net.minecraft.world.level.block.Block;
import com.mojang.serialization.JsonOps;

/** Authored scenarios run against vanilla; no decompiled sources or assets. */
public final class StructureGolden {
    static final Gson JSON = new GsonBuilder().disableHtmlEscaping().create();
    static JsonArray ints(int... values) { var a=new JsonArray(); for(int v:values)a.add(v); return a; }
    static void write(Path path, JsonElement value) throws Exception {
        byte[] bytes=JSON.toJson(value).getBytes(java.nio.charset.StandardCharsets.UTF_8);
        if(Files.exists(path))try(var in=new java.util.zip.GZIPInputStream(Files.newInputStream(path))) {if(Arrays.equals(bytes,in.readAllBytes()))return;}
        try(var out=new GZIPOutputStream(Files.newOutputStream(path))) { out.write(bytes); }
    }
    static void beardifier(Path directory) throws Exception {
        var out = new JsonArray();
        var b = new BoundingBox(-3,61,2,11,76,14);
        var affected = b.inflatedBy(24);
        for (var adjustment:TerrainAdjustment.values()) for(int delta: new int[]{0,3,-2}) {
            var junctions = List.of(new JigsawJunction(4,65,-2,1,StructureTemplatePool.Projection.RIGID), new JigsawJunction(-9,62,7,-3,StructureTemplatePool.Projection.TERRAIN_MATCHING));
            var rigids = List.of(new Beardifier.Rigid(b,adjustment,delta),new Beardifier.Rigid(new BoundingBox(9,58,3,17,68,18),TerrainAdjustment.BEARD_BOX,2));
            var beard = new Beardifier(rigids,junctions,affected);
            var item = new JsonObject(); item.addProperty("adjustment",adjustment.getSerializedName()); item.addProperty("delta",delta);
            var scalars = new JsonArray();
            // Kernel ends, ground offsets, edges and all quadrants.
            for(int z=-16;z<=30;z+=2)for(int x=-18;x<=30;x+=2)for(int y=45;y<=87;y++)scalars.add(Integer.toUnsignedLong(Float.floatToRawIntBits(beard.sampleValue(SamplerContext.EMPTY_UNCACHED,x,y,z))));
            item.add("scalars",scalars);
            var volumes = new JsonArray();
            for(var volume:List.of(new DensityVolume(10,15,9,-28,35,-22,3,4,5),new DensityVolume(5,6,7,1,62,5),new DensityVolume(3,4,2,100,99,102,5,7,3))) {
                var v = new JsonObject(); v.add("size",ints(volume.sizeX(),volume.sizeY(),volume.sizeZ())); v.add("min",ints(volume.minBlockX(),volume.minBlockY(),volume.minBlockZ())); v.add("step",ints(volume.stepBlockX(),volume.stepBlockY(),volume.stepBlockZ()));
                var values = new JsonArray();var ctx=SamplerContext.builder().build();
                try(var buffer=ctx.acquireBuffer(volume)) {beard.sampleVolume(ctx,buffer,volume);for(int i=0;i<volume.size();i++)values.add(Integer.toUnsignedLong(Float.floatToRawIntBits(buffer.get(i))));}
                v.add("values",values);volumes.add(v);
            }
            item.add("volumes",volumes);out.add(item);
        }
        write(directory.resolve("beardifier.json.gz"),out);
    }
    static RegistryAccess.Frozen load() {
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        var manager=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(manager,builtins).forEach(Registry.PendingTags::apply);
        return RegistryDataLoader.load(manager,builtins.registries().<HolderLookup.RegistryLookup<?>>map(e -> e.value()).toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
    }
    static void placements(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var out=new JsonArray(); var sets=registries.lookupOrThrow(Registries.STRUCTURE_SET);
        var source=new FixedBiomeSource(registries.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS));
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        for(long seed:new long[]{0,-1,8675309,Long.MIN_VALUE,Long.MAX_VALUE}) {
            var random=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings);
            var state=ChunkGeneratorStructureState.createForNormal(random,seed,new ChunkPos(37,-49),source,sets);
            for(var entry:sets.listElements().sorted(Comparator.comparing(h->h.key().identifier())).toList()) {
                var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("set",entry.key().identifier().toString());
                var placement=entry.value().placement();var candidates=new JsonArray();
                if(placement instanceof RandomSpreadStructurePlacement spread) {
                    for(int x:new int[]{-30000000,-4097,-129,-128,-33,-32,-1,0,1,31,32,33,127,128,4096,29999999})for(int z:new int[]{-777,-33,-1,0,1,32,12345}) {
                        var candidate=spread.getPotentialStructureChunk(seed,x,z);candidates.add(ints(x,z,candidate.x(),candidate.z()));
                    }
                }
                item.add("candidates",candidates);var starts=new JsonArray();
                for(int x=-64;x<=64;x++)for(int z=-64;z<=64;z++)if(placement.isStructureChunk(state,x,z))starts.add(ints(x,z));
                item.add("placement_chunks",starts);
                if(placement instanceof ConcentricRingsStructurePlacement rings) {
                    var positions=state.getRingPositionsFor(rings);var list=new JsonArray();if(positions!=null)for(var p:positions)list.add(ints(p.x(),p.z()));item.add("rings",list);
                }
                out.add(item);
            }
        }
        write(directory.resolve("placements.json.gz"),out);
        var frequencies=new JsonArray();
        for(var method:AbstractSpreadingStructurePlacement.FrequencyReductionMethod.values())for(long seed:new long[]{0,-1,8675309,Long.MIN_VALUE,Long.MAX_VALUE})for(float probability:new float[]{0,0.004f,0.2f,0.5f,1}) {
            var item=new JsonObject();item.addProperty("method",method.getSerializedName());item.addProperty("seed",seed);item.addProperty("probability",probability);var values=new JsonArray();
            for(int x=-97;x<=97;x+=7)for(int z=-83;z<=83;z+=11)values.add(method.shouldGenerate(seed,1234567,x,z,probability));item.add("values",values);frequencies.add(item);
        }
        write(directory.resolve("frequencies.json.gz"),frequencies);
    }
    static void rules(RegistryAccess.Frozen registries,Path directory) throws Exception {
        String[] tests={
            "{predicate_type:'minecraft:always_true'}",
            "{predicate_type:'minecraft:block_match',block:'minecraft:stone'}",
            "{predicate_type:'minecraft:blockstate_match',block_state:{id:'minecraft:oak_stairs',properties:{facing:'east'}}}",
            "{predicate_type:'minecraft:tag_match',tag:'minecraft:stairs'}",
            "{predicate_type:'minecraft:height_match',min_inclusive:-3,max_inclusive:5}",
            "{predicate_type:'minecraft:height_match',min_inclusive:12,max_inclusive:5}",
            "{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.37}",
            "{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:1.2}",
            "{predicate_type:'minecraft:random_blockstate_match',block_state:'minecraft:stone',probability:-0.1}",
            "{predicate_type:'minecraft:random_blockstate_match',block_state:'minecraft:stone',probability:0.61}",
            "{predicate_type:'minecraft:all_of',rules:[]}",
            "{predicate_type:'minecraft:any_of',rules:[]}",
            "{predicate_type:'minecraft:all_of',rules:[{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.25},{predicate_type:'minecraft:not',rule:{predicate_type:'minecraft:height_match',min_inclusive:0,max_inclusive:4}}]}",
            "{predicate_type:'minecraft:any_of',rules:[{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.25},{predicate_type:'minecraft:random_blockstate_match',block_state:'minecraft:stone',probability:0.75}]}"
        };
        String[] states={"stone","air","oak_stairs","oak_log","deepslate","stone_bricks"};
        int[][] positions={{0,0,0},{4,-4,9},{-1,5,3},{0,6,0},{0,12,0},{Integer.MIN_VALUE,32,Integer.MAX_VALUE}};
        var out=new JsonObject();var blockCases=new JsonArray();var ops=RegistryOps.create(JsonOps.INSTANCE,registries);
        for(String definition:tests)for(long seed:new long[]{0,-1,8675309}) {
            var value=JsonParser.parseString(definition);var test=RuleTest.CODEC.parse(ops,value).getOrThrow();var random=new LegacyRandomSource(seed);var item=new JsonObject();item.add("definition",value);item.addProperty("seed",seed);var results=new JsonArray();
            for(String stateName:states)for(int[] p:positions) {
                var state=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(stateName)).defaultBlockState();
                var result=new JsonObject();result.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));result.add("pos",ints(p));result.addProperty("result",test.test(state,new BlockPos(p[0],p[1],p[2]),random));result.addProperty("next",random.nextInt());results.add(result);
            }
            item.add("results",results);blockCases.add(item);
        }
        out.add("blocks",blockCases);var posCases=new JsonArray();
        for(String type:List.of("linear_pos","axis_aligned_linear_pos"))for(String axis:List.of("x","y","z"))for(long seed:new long[]{0,-1,8675309}) {
            var value=JsonParser.parseString("{predicate_type:'minecraft:"+type+"',axis:'"+axis+"',min_chance:0.1,max_chance:0.8,min_dist:2,max_dist:17}");
            var test=PosRuleTest.CODEC.parse(ops,value).getOrThrow();var random=new LegacyRandomSource(seed);var item=new JsonObject();item.add("definition",value);item.addProperty("seed",seed);var results=new JsonArray();
            for(int[] p:positions)for(int[] reference:positions) {
                var result=new JsonObject();result.add("pos",ints(p));result.add("reference",ints(reference));result.addProperty("result",test.test(BlockPos.ZERO,new BlockPos(p[0],p[1],p[2]),new BlockPos(reference[0],reference[1],reference[2]),random));result.addProperty("next",random.nextInt());results.add(result);
            }
            item.add("results",results);posCases.add(item);
        }
        out.add("positions",posCases);write(directory.resolve("rules.json.gz"),out);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var directory=Path.of(args[0]);Files.createDirectories(directory);var registries=load();beardifier(directory);placements(registries,directory);rules(registries,directory);
    }
}
