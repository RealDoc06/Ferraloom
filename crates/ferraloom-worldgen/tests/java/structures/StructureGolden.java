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
    static net.minecraft.server.packs.resources.ResourceManager RESOURCES;
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
        var manager=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));RESOURCES=manager;
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(manager,builtins).forEach(Registry.PendingTags::apply);
        var loaded=RegistryDataLoader.load(manager,builtins.registries().<HolderLookup.RegistryLookup<?>>map(e -> e.value()).toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
        return new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),loaded.registries())).freeze();
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
    static void templates(Path directory) throws Exception {
        var out=new JsonArray();
        var paletteField=StructureTemplate.class.getDeclaredField("palettes");paletteField.setAccessible(true);
        var entityField=StructureTemplate.class.getDeclaredField("entityInfoList");entityField.setAccessible(true);
        Path jar=Path.of(Block.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        try(var zip=new java.util.zip.ZipFile(jar.toFile())) {
            for(var entry:zip.stream().filter(e->e.getName().startsWith("data/minecraft/structure/")&&e.getName().endsWith(".nbt")).sorted(Comparator.comparing(e->e.getName())).toList()) {
                net.minecraft.nbt.CompoundTag tag;
                try(var in=zip.getInputStream(entry)) {tag=net.minecraft.nbt.NbtIo.readCompressed(in,net.minecraft.nbt.NbtAccounter.unlimitedHeap());}
                var template=new StructureTemplate();template.load(BuiltInRegistries.BLOCK,tag);
                var item=new JsonObject();item.addProperty("id",entry.getName().replace("data/minecraft/structure/","minecraft:").replace(".nbt",""));item.addProperty("version",tag.getIntOr("DataVersion",500));
                var size=template.getSize();item.add("size",ints(size.getX(),size.getY(),size.getZ()));
                item.addProperty("entities",((List<?>)entityField.get(template)).size());
                @SuppressWarnings("unchecked") var palettes=(List<StructureTemplate.Palette>)paletteField.get(template);var values=new JsonArray();
                for(var palette:palettes) {
                    var value=new JsonObject();var blocks=palette.blocks();value.addProperty("size",blocks.size());value.addProperty("nbt_count",blocks.stream().filter(b->b.nbt()!=null).count());
                    var samples=new JsonArray();var indices=new TreeSet<Integer>();for(int i=0;i<9;i++)if(!blocks.isEmpty())indices.add((blocks.size()-1)*i/8);
                    for(int i=1;i<blocks.size();i++)if((blocks.get(i-1).nbt()==null)!=(blocks.get(i).nbt()==null)){indices.add(i-1);indices.add(i);}
                    for(int i:indices) {var b=blocks.get(i);var sample=new JsonObject();sample.addProperty("index",i);sample.add("pos",ints(b.pos().getX(),b.pos().getY(),b.pos().getZ()));sample.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(b.state()));sample.addProperty("nbt",b.nbt()!=null);samples.add(sample);}
                    value.add("samples",samples);values.add(value);
                }
                item.add("palettes",values);out.add(item);
            }
        }
        write(directory.resolve("templates.json.gz"),out);
        var geometry=new JsonArray();
        for(var mirror:net.minecraft.world.level.block.Mirror.values())for(var rotation:net.minecraft.world.level.block.Rotation.values())for(int[] pivot:new int[][]{{0,0,0},{7,2,-11},{Integer.MAX_VALUE,0,Integer.MIN_VALUE}})for(int[] p:new int[][]{{0,0,0},{1,5,9},{-7,12,-15},{Integer.MAX_VALUE,-1,Integer.MIN_VALUE}}) {
            var item=new JsonObject();item.addProperty("mirror",mirror.ordinal());item.addProperty("rotation",rotation.ordinal());item.add("pivot",ints(pivot));item.add("pos",ints(p));
            var bp=new BlockPos(p[0],p[1],p[2]);var pv=new BlockPos(pivot[0],pivot[1],pivot[2]);var block=StructureTemplate.transform(bp,mirror,rotation,pv);item.add("block",ints(block.getX(),block.getY(),block.getZ()));
            var entity=StructureTemplate.transform(new net.minecraft.world.phys.Vec3(p[0]+0.25,p[1]+0.5,p[2]+0.75),mirror,rotation,pv);var bits=new JsonArray();for(double v:new double[]{entity.x,entity.y,entity.z})bits.add(Long.toString(Double.doubleToRawLongBits(v)));item.add("entity",bits);geometry.add(item);
        }
        write(directory.resolve("template_geometry.json.gz"),geometry);
    }
    static void processors(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var out=new JsonObject();var cases=new JsonArray();
        String[] definitions={
            "{processor_type:'minecraft:nop'}",
            "{processor_type:'minecraft:blackstone_replace'}",
            "{processor_type:'minecraft:block_age',mossiness:0.43}",
            "{processor_type:'minecraft:block_ignore',blocks:['minecraft:air','minecraft:stone']}",
            "{processor_type:'minecraft:block_rot',integrity:0.51}",
            "{processor_type:'minecraft:block_rot',integrity:0.1,rottable_blocks:['minecraft:stone','minecraft:oak_stairs']}",
            "{processor_type:'minecraft:gravity',heightmap:'WORLD_SURFACE_WG',offset:-1}",
            "{processor_type:'minecraft:jigsaw_replacement'}",
            "{processor_type:'minecraft:lava_submerged_block'}",
            "{processor_type:'minecraft:protected_blocks',value:['minecraft:lava']}",
            "{processor_type:'minecraft:rule',rules:[{input_predicate:{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.5},location_predicate:{predicate_type:'minecraft:always_true'},output_state:'minecraft:gold_block',block_entity_modifier:{type:'minecraft:append_loot',loot_table:'minecraft:chests/simple_dungeon'}}]}",
            "{processor_type:'minecraft:capped',limit:3,delegate:{processor_type:'minecraft:block_age',mossiness:0.5}}"
        };
        var ops=RegistryOps.create(JsonOps.INSTANCE,registries);var inputs=new ArrayList<net.minecraft.world.level.block.state.BlockState>();
        for(var block:List.of(net.minecraft.world.level.block.Blocks.STONE,net.minecraft.world.level.block.Blocks.STONE_BRICKS,net.minecraft.world.level.block.Blocks.CHISELED_STONE_BRICKS,net.minecraft.world.level.block.Blocks.OBSIDIAN,net.minecraft.world.level.block.Blocks.OAK_STAIRS,net.minecraft.world.level.block.Blocks.STONE_BRICK_STAIRS,net.minecraft.world.level.block.Blocks.MOSSY_COBBLESTONE_WALL,net.minecraft.world.level.block.Blocks.STONE_SLAB,net.minecraft.world.level.block.Blocks.MOSSY_COBBLESTONE_SLAB,net.minecraft.world.level.block.Blocks.IRON_BARS,net.minecraft.world.level.block.Blocks.AIR,net.minecraft.world.level.block.Blocks.JIGSAW))inputs.addAll(block.getStateDefinition().getPossibleStates());
        for(var definition:definitions)for(boolean lava:new boolean[]{false,true})for(boolean external:new boolean[]{false,true}) {
            var value=JsonParser.parseString(definition);var processor=StructureProcessorType.SINGLE_CODEC.parse(ops,value).getOrThrow();var settings=new StructurePlaceSettings();var supplied=new LegacyRandomSource(12345);if(external)settings.setRandom(supplied);
            var world=(net.minecraft.world.level.LevelReader)java.lang.reflect.Proxy.newProxyInstance(StructureGolden.class.getClassLoader(),new Class<?>[]{net.minecraft.world.level.LevelReader.class},(proxy,method,args)->switch(method.getName()) {
                case "getBlockState" -> (lava?net.minecraft.world.level.block.Blocks.LAVA:net.minecraft.world.level.block.Blocks.AIR).defaultBlockState();
                case "getHeight" -> 67;
                case "holderLookup" -> BuiltInRegistries.BLOCK;
                default -> throw new UnsupportedOperationException(method.toString());
            });
            var item=new JsonObject();item.add("definition",value);item.addProperty("lava",lava);item.addProperty("external",external);var samples=new JsonArray();
            int i=0;
            for(var state:inputs) {
                int[] p={i%31-15,52+i%27,i%19-9};i++;var tag=new net.minecraft.nbt.CompoundTag();tag.putString("final_state","minecraft:oak_stairs[facing=east,half=top]");tag.putInt("marker",17);
                var info=new StructureTemplate.StructureBlockInfo(new BlockPos(p[0],p[1],p[2]),state,tag);
                var result=processor.processBlock(world,new BlockPos(7,64,7),new BlockPos(3,65,4),new BlockPos(1,2,3),info,settings);
                var sample=new JsonObject();sample.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));sample.add("pos",ints(p));sample.addProperty("result",result==null?-1:Block.BLOCK_STATE_REGISTRY.getId(result.state()));
                if(result!=null) {sample.add("result_pos",ints(result.pos().getX(),result.pos().getY(),result.pos().getZ()));if(result.nbt()!=null)sample.addProperty("nbt",result.nbt().toString());}
                samples.add(sample);
            }
            item.add("samples",samples);item.addProperty("next",supplied.nextInt());cases.add(item);
        }
        out.add("blocks",cases);
        var modifiers=new JsonArray();
        for(String definition:List.of("{type:'minecraft:passthrough'}","{type:'minecraft:clear'}","{type:'minecraft:append_loot',loot_table:'minecraft:chests/simple_dungeon'}","{type:'minecraft:append_static',data:{nested:{a:7,b:'new'},numbers:[1,2,3],large:123456,short:600,decimal:0.25}}"))for(boolean existing:new boolean[]{false,true}) {
            var value=JsonParser.parseString(definition);var modifier=net.minecraft.world.level.levelgen.structure.templatesystem.rule.blockentity.RuleBlockEntityModifier.CODEC.parse(ops,value).getOrThrow();
            var tag=existing?net.minecraft.nbt.TagParser.parseCompoundFully("{nested:{a:3,c:9},old:1}"):null;var random=new LegacyRandomSource(99);var result=modifier.apply(random,tag);var item=new JsonObject();item.add("definition",value);item.addProperty("existing",existing);if(result!=null)item.addProperty("result",result.toString());item.addProperty("next",random.nextInt());modifiers.add(item);
        }
        out.add("modifiers",modifiers);write(directory.resolve("processors.json.gz"),out);
        // Exactly the classification used by template liquid preservation:
        // non-source LiquidBlockContainers all have WATERLOGGED, and vice versa.
        for(var state:Block.BLOCK_STATE_REGISTRY)if(!state.getFluidState().isSource()) {
            boolean container=state.getBlock() instanceof net.minecraft.world.level.block.LiquidBlockContainer;
            boolean property=state.hasProperty(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED);
            if(container!=property)throw new IllegalStateException("LiquidBlockContainer classification: "+state);
        }
    }
    static net.minecraft.nbt.ListTag nbtInts(int... values) {var list=new net.minecraft.nbt.ListTag();for(int v:values)list.add(net.minecraft.nbt.IntTag.valueOf(v));return list;}
    static void placedTemplates(Path directory) throws Exception {
        var tag=new net.minecraft.nbt.CompoundTag();tag.putInt("DataVersion",5023);tag.put("size",nbtInts(4,3,3));
        var palettes=new net.minecraft.nbt.ListTag();
        for(boolean variant:new boolean[]{false,true}) {
            var palette=new net.minecraft.nbt.ListTag();
            for(var state:List.of(net.minecraft.world.level.block.Blocks.STONE.defaultBlockState(),net.minecraft.world.level.block.Blocks.OAK_STAIRS.defaultBlockState(),net.minecraft.world.level.block.Blocks.STONE_BRICK_SLAB.defaultBlockState(),net.minecraft.world.level.block.Blocks.OAK_LOG.defaultBlockState(),net.minecraft.world.level.block.Blocks.AIR.defaultBlockState(),variant?net.minecraft.world.level.block.Blocks.MOSSY_COBBLESTONE.defaultBlockState():net.minecraft.world.level.block.Blocks.STONE_BRICKS.defaultBlockState()))palette.add(net.minecraft.nbt.NbtUtils.writeBlockState(state));
            palettes.add(palette);
        }
        tag.put("palettes",palettes);var blocks=new net.minecraft.nbt.ListTag();
        for(int x=0;x<4;x++)for(int y=0;y<3;y++)for(int z=0;z<3;z++) {var block=new net.minecraft.nbt.CompoundTag();block.put("pos",nbtInts(x,y,z));block.putInt("state",(x*3+y+z)%6);blocks.add(block);}
        tag.put("blocks",blocks);var template=new StructureTemplate();template.load(BuiltInRegistries.BLOCK,tag);var out=new JsonArray();
        for(var mirror:net.minecraft.world.level.block.Mirror.values())for(var rotation:net.minecraft.world.level.block.Rotation.values())for(boolean external:new boolean[]{false,true})for(boolean shared:new boolean[]{false,true}) {
            var settings=new StructurePlaceSettings().setMirror(mirror).setRotation(rotation).setRotationPivot(new BlockPos(1,0,-2)).setBoundingBox(new BoundingBox(2,63,2,13,69,13)).setKnownShape(true).setIgnoreEntities(true).setLiquidSettings(LiquidSettings.IGNORE_WATERLOGGING).addProcessor(new BlockIgnoreProcessor(List.of(net.minecraft.world.level.block.Blocks.AIR))).addProcessor(BlackstoneReplaceProcessor.INSTANCE).addProcessor(new BlockRotProcessor(0.8f));
            var supplied=new LegacyRandomSource(91);if(external)settings.setRandom(supplied);var placed=new TreeMap<BlockPos,net.minecraft.world.level.block.state.BlockState>(Comparator.comparingInt((BlockPos p)->p.getX()).thenComparingInt(p->p.getY()).thenComparingInt(p->p.getZ()));
            var world=(net.minecraft.world.level.WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(StructureGolden.class.getClassLoader(),new Class<?>[]{net.minecraft.world.level.WorldGenLevel.class},(proxy,method,args)->switch(method.getName()) {
                case "setBlock" -> {placed.put(((BlockPos)args[0]).immutable(),(net.minecraft.world.level.block.state.BlockState)args[1]);yield true;}
                default -> throw new UnsupportedOperationException(method.toString());
            });
            var random=new WorldgenRandom(new XoroshiroRandomSource(17));if(shared)settings.setRandom(random);boolean result=template.placeInWorld(world,new BlockPos(7,64,7),new BlockPos(3,65,4),settings,random,18);
            var item=new JsonObject();item.addProperty("mirror",mirror.ordinal());item.addProperty("rotation",rotation.ordinal());item.addProperty("external",external);item.addProperty("shared",shared);item.addProperty("result",result);item.addProperty("settings_next",supplied.nextInt());item.addProperty("next",random.nextInt());var values=new JsonArray();for(var e:placed.entrySet())values.add(ints(e.getKey().getX(),e.getKey().getY(),e.getKey().getZ(),Block.BLOCK_STATE_REGISTRY.getId(e.getValue())));item.add("blocks",values);out.add(item);
        }
        write(directory.resolve("placed_templates.json.gz"),out);
    }
    static String elementName(StructurePoolElement element) {
        if(element instanceof EmptyPoolElement)return "empty";
        if(element instanceof ListPoolElement list)return "["+list.getElements().stream().map(StructureGolden::elementName).collect(java.util.stream.Collectors.joining(","))+"]";
        if(element instanceof SinglePoolElement single)return (element instanceof LegacySinglePoolElement?"legacy:":"single:")+single.getTemplateLocation();
        return "feature";
    }
    static void poolsAndAliases(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var out=new JsonArray();var pools=registries.lookupOrThrow(Registries.TEMPLATE_POOL);
        for(var entry:pools.listElements().sorted(Comparator.comparing(h->h.key().identifier())).toList())for(long seed:new long[]{0,-1,8675309}) {
            var item=new JsonObject();item.addProperty("pool",entry.key().identifier().toString());item.addProperty("seed",seed);var random=new LegacyRandomSource(seed);var samples=new JsonArray();
            for(int i=0;i<32;i++)samples.add(elementName(entry.value().getRandomTemplate(random)));item.add("samples",samples);
            var shuffled=new JsonArray();for(var e:entry.value().getShuffledTemplates(random))shuffled.add(elementName(e));item.add("shuffled",shuffled);item.addProperty("next",random.nextInt());out.add(item);
        }
        write(directory.resolve("pools.json.gz"),out);
        var aliases=new JsonArray();var ops=RegistryOps.create(JsonOps.INSTANCE,registries);
        var bindings=JsonParser.parseString("[{type:'minecraft:direct',alias:'test:a',target:'test:b'},{type:'minecraft:random',alias:'test:c',targets:[{data:'test:d',weight:3},{data:'test:e',weight:0},{data:'test:f',weight:8}]},{type:'minecraft:random_group',groups:[{weight:1,data:[{type:'minecraft:direct',alias:'test:g',target:'test:h'},{type:'minecraft:random',alias:'test:i',targets:[{data:'test:j',weight:1},{data:'test:k',weight:1}]}]},{weight:7,data:[{type:'minecraft:direct',alias:'test:g',target:'test:l'}]}]}]");
        var decoded=net.minecraft.world.level.levelgen.structure.pools.alias.PoolAliasBinding.CODEC.listOf().parse(ops,bindings).getOrThrow();
        for(long seed:new long[]{0,-1,8675309,Long.MIN_VALUE,Long.MAX_VALUE})for(int[] p:new int[][]{{0,64,0},{-17,-63,31},{123456,93,-54321}}) {
            var lookup=net.minecraft.world.level.levelgen.structure.pools.alias.PoolAliasLookup.create(decoded,new BlockPos(p[0],p[1],p[2]),seed);var item=new JsonObject();item.addProperty("seed",seed);item.add("pos",ints(p));item.add("bindings",bindings);var values=new JsonObject();
            for(String name:List.of("a","b","c","g","i","missing")) {var key=ResourceKey.create(Registries.TEMPLATE_POOL,Identifier.parse("test:"+name));values.addProperty("test:"+name,lookup.lookup(key).identifier().toString());}item.add("values",values);aliases.add(item);
        }
        write(directory.resolve("aliases.json.gz"),aliases);
    }
    static JsonArray bounds(BoundingBox b) {return ints(b.minX(),b.minY(),b.minZ(),b.maxX(),b.maxY(),b.maxZ());}
    static void jigsaw(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var out=new JsonArray();var biomes=registries.lookupOrThrow(Registries.BIOME);var source=new FixedBiomeSource(biomes.getOrThrow(Biomes.PLAINS));
        var structures=registries.lookupOrThrow(Registries.STRUCTURE);var accessor=net.minecraft.world.level.LevelHeightAccessor.create(-64,384);
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        var flat=new FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var noise=new NoiseBasedChunkGenerator(source,settings);
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("structures-world")).createAccess("test")) {
            var templates=new StructureTemplateManager(RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            for(String terrain:List.of("flat","noise"))for(long seed:new long[]{0,-1,8675309})for(int[] chunk:new int[][]{{0,0},{-7,11}}) {
                var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings.value());var generator=terrain.equals("flat")?flat:noise;
                for(var entry:structures.listElements().sorted(Comparator.comparing(h->h.key().identifier())).toList())if(entry.value() instanceof net.minecraft.world.level.levelgen.structure.structures.JigsawStructure structure) {
                    var context=new Structure.GenerationContext(registries,generator,source,state.createClimateSampler(SamplerContext.builder().enableCaches().build()),state,templates,seed,new ChunkPos(chunk[0],chunk[1]),accessor,b->true);
                    var stub=structure.findGenerationPoint(context);var builtPieces=stub.map(s->s.getPiecesBuilder().build()).orElse(null);var item=new JsonObject();item.addProperty("structure",entry.key().identifier().toString());item.addProperty("terrain",terrain);item.addProperty("seed",seed);item.add("chunk",ints(chunk));var pieces=new JsonArray();
                    if(stub.isPresent()) {
                        var p=stub.get().position();item.add("position",ints(p.getX(),p.getY(),p.getZ()));
                        for(var piece:builtPieces.pieces()) {
                            var poolPiece=(PoolElementStructurePiece)piece;var value=new JsonObject();value.addProperty("element",elementName(poolPiece.getElement()));value.add("bounds",bounds(piece.getBoundingBox()));var pos=poolPiece.getPosition();value.add("position",ints(pos.getX(),pos.getY(),pos.getZ()));value.addProperty("rotation",poolPiece.getRotation().ordinal());value.addProperty("ground",poolPiece.getGroundLevelDelta());
                            var junctions=new JsonArray();for(var junction:poolPiece.getJunctions()) {var j=new JsonObject();j.add("source",ints(junction.getSourceX(),junction.getSourceGroundY(),junction.getSourceZ()));j.addProperty("delta_y",junction.getDeltaY());j.addProperty("projection",junction.getDestProjection().getName());junctions.add(j);}value.add("junctions",junctions);pieces.add(value);
                        }
                    }
                    item.add("pieces",pieces);
                    if(terrain.equals("flat") && seed==0 && chunk[0]==0 && chunk[1]==0 && stub.isPresent()) {
                        var serialization=new net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext(RESOURCES,registries,templates);
                        var start=new StructureStart(structure,new ChunkPos(chunk[0],chunk[1]),3,builtPieces);
                        var saved=start.createTag(serialization,new ChunkPos(17,-19));item.addProperty("terrain_adjustment",structure.terrainAdaptation().getSerializedName());item.addProperty("start_nbt",saved.toString());item.add("start_bounds",bounds(start.getBoundingBox()));
                        var loaded=StructureStart.loadStaticStart(serialization,saved,seed);item.addProperty("loaded_nbt",loaded.createTag(serialization,new ChunkPos(17,-19)).toString());item.add("loaded_bounds",bounds(loaded.getBoundingBox()));
                    }
                    out.add(item);
                }
            }
        }
        write(directory.resolve("jigsaw.json.gz"),out);
    }
    static void replacements(RegistryAccess.Frozen registries, Path directory) throws Exception {
        var out=new JsonArray();
        for(var text:List.of("stone","minecraft:stone junk","stone[ ]","stone []","stone{} trailing","stone{a:[1,2]}","stone{a:"," stone","#minecraft:logs","oak_stairs[facing=east]","oak_stairs[ facing = 'east' , half = top ]","oak_stairs[\"facing\"=\"west\"]","oak_stairs[facing=north,]","oak_stairs[facing=north, ]","oak_stairs[facing=north,facing=east]","oak_stairs[facing=up]","stone[missing=true]","oak_stairs[facing=north]extra","oak_stairs[facing=north]{a:1}x","STONE","stone!","stone/unknown","structure_void","oak_stairs[half=top,waterlogged=true]","oak_stairs[facing=north")) {
            var item=new JsonObject();item.addProperty("input",text);
            try { var result=net.minecraft.commands.arguments.blocks.BlockStateParser.parseForBlock(registries.lookupOrThrow(Registries.BLOCK),text,true);item.addProperty("state",Block.getId(result.blockState())); } catch(com.mojang.brigadier.exceptions.CommandSyntaxException e) {item.add("state",JsonNull.INSTANCE);}
            out.add(item);
        }
        write(directory.resolve("replacements.json.gz"),out);
    }
    static void decoration(RegistryAccess.Frozen registries, Path directory) throws Exception {
        var out=new JsonArray();var registry=registries.lookupOrThrow(Registries.STRUCTURE);
        for(long seed:new long[]{0,-1,8675309})for(var step:GenerationStep.Decoration.values()) {
            var entries=registry.listElements().filter(h->h.value().step()==step).toList();var item=new JsonObject();item.addProperty("seed",seed);item.addProperty("step",step.ordinal());var ids=new JsonArray();for(var e:entries)ids.add(e.key().identifier().toString());item.add("ids",ids);var next=new JsonArray();
            for(int prefix=0;prefix<=entries.size();prefix++) {var random=new WorldgenRandom(new net.minecraft.world.level.levelgen.XoroshiroRandomSource(17));long decorationSeed=random.setDecorationSeed(seed,-112,176);for(int index=0;index<prefix;index++)random.setFeatureSeed(decorationSeed,index,step.ordinal());next.add(random.nextInt());}
            item.add("next",next);out.add(item);
        }
        write(directory.resolve("decoration.json.gz"),out);
    }
    static void referenceSets(Path directory) throws Exception {
        var out=new JsonArray();
        for(boolean fromArray:new boolean[]{false,true})for(int size:new int[]{0,1,2,3,12,16,24,25,48,49,100,289}) {
            var input=new ArrayList<Long>();for(int i=0;i<size;i++) {long value=ChunkPos.pack((i%17)-8,(i/17)-8);input.add(value);if(i%7==0)input.add(value);if(i%11==0)input.add(0L);}
            var set=fromArray?new it.unimi.dsi.fastutil.longs.LongOpenHashSet(input.stream().mapToLong(Long::longValue).toArray()):new it.unimi.dsi.fastutil.longs.LongOpenHashSet();var added=new JsonArray();for(long value:input)added.add(set.add(value));var item=new JsonObject();item.addProperty("from_array",fromArray);var values=new JsonArray();input.forEach(values::add);item.add("input",values);item.add("added",added);var order=new JsonArray();for(long value:set.toLongArray())order.add(value);item.add("order",order);out.add(item);
        }
        write(directory.resolve("reference_sets.json.gz"),out);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var directory=Path.of(args[0]);Files.createDirectories(directory);var registries=load();beardifier(directory);placements(registries,directory);rules(registries,directory);templates(directory);processors(registries,directory);placedTemplates(directory);poolsAndAliases(registries,directory);jigsaw(registries,directory);replacements(registries,directory);decoration(registries,directory);StructureCheckGolden.capture(registries,directory);StructureStartCodecGolden.capture(registries,directory);referenceSets(directory);StructureCodecGolden.capture(registries,directory);StructureStatusGolden.capture(registries,directory);HandwrittenGolden.capture(registries,directory);ShipwreckHeightGolden.capture(registries,directory);JungleTempleGolden.capture(registries,directory);
        SwampHutGolden.capture(registries,directory);RegionRandomGolden.capture(registries,directory);DesertPyramidGolden.capture(registries,directory);RuinedPortalGolden.capture(registries,directory);MineshaftGolden.capture(registries,directory);NetherFortressGolden.capture(registries,directory);StrongholdGolden.capture(registries,directory);
        MansionGolden.grids(directory);
        MansionGolden.pieces(registries,directory);
        MansionFoundationGolden.capture(registries,directory);StrongholdBlocksGolden.capture(registries,directory);NetherFortressBlocksGolden.capture(registries,directory);MineshaftBlocksGolden.capture(registries,directory);
    }
}
