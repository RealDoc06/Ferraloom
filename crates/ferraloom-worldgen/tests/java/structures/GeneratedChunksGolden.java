import com.google.gson.*;
import com.mojang.serialization.Lifecycle;
import com.mojang.serialization.JsonOps;
import java.net.Proxy;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
import net.minecraft.*;
import net.minecraft.commands.Commands;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.level.progress.*;
import net.minecraft.server.notifications.*;
import net.minecraft.server.packs.repository.*;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.*;
import net.minecraft.util.Util;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.util.debugchart.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.*;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.storage.*;

/** Gated real-world FEATURES captures, with no network listener or level ticks.
 * The temporary world uses the normal preset and default vanilla datapack. */
public final class GeneratedChunksGolden extends MinecraftServer {
    private final LocalSampleLogger sampleLogger=new LocalSampleLogger(4);
    GeneratedChunksGolden(LevelStorageSource.LevelStorageAccess storage,PackRepository packs,WorldStem stem)throws Exception {
        super(Thread.currentThread(),storage,packs,stem,Optional.empty(),Proxy.NO_PROXY,DataFixers.getDataFixer(),noServices(),LoggingLevelLoadListener.forDedicatedServer(),false,new NotificationManager());
        setPlayerList(new PlayerList(this,registries(),playerDataStorage,new EmptyNotificationService()){});
        createLevels();
    }
    static Services noServices()throws Exception {var f=GameTestServer.class.getDeclaredField("NO_SERVICES");f.setAccessible(true);return (Services)f.get(null);}
    static WorldStem load(PackRepository packs,long seed)throws Exception {
        packs.reload();var config=new WorldLoader.InitConfig(new WorldLoader.PackConfig(packs,WorldDataConfiguration.DEFAULT,false,true),Commands.CommandSelection.DEDICATED,LevelBasedPermissionSet.OWNER);
        return Util.<WorldStem>blockUntilDone(executor->WorldLoader.load(config,context->{
            var dimensions=context.datapackWorldRegistries().lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.NORMAL).value().createWorldDimensions();
            var complete=dimensions.bake(new MappedRegistry<LevelStem>(Registries.LEVEL_STEM,Lifecycle.stable()).freeze());
            var settings=new LevelSettings("Structure parity fixture",GameType.SURVIVAL,LevelSettings.DifficultySettings.DEFAULT,false,context.dataConfiguration());
            var data=new PrimaryLevelData(settings,complete.specialWorldProperty(),complete.lifecycle());
            // Skip spawn-area preparation; it is unrelated to these requested chunks.
            data.setInitialized(true);
            return new WorldLoader.DataLoadOutput<>(new LevelDataAndDimensions.WorldDataAndGenSettings(data,new WorldGenSettings(new WorldOptions(seed,true,false),dimensions)),complete.dimensionsRegistryAccess());
        },WorldStem::new,Util.backgroundExecutor(),executor)).get();
    }
    void captureStarts(long seed,Path output)throws Exception {
        var dimensions=new JsonArray();
        for(var key:List.of(Level.OVERWORLD,Level.NETHER,Level.END)) {
            var level=getLevel(key);var generator=level.getChunkSource().getGenerator();var state=level.getChunkSource().getGeneratorState();var registry=registryAccess().lookupOrThrow(Registries.STRUCTURE);
            var context=new StructurePieceSerializationContext(getResourceManager(),registryAccess(),getStructureTemplateManager());
            var dimension=new JsonObject();dimension.addProperty("dimension",key.identifier().toString());dimension.addProperty("seed",seed);dimension.addProperty("min_y",level.getMinY());dimension.addProperty("height",level.getHeight());dimension.addProperty("radius",16);
            dimension.add("generator",ChunkGenerator.CODEC.encodeStart(RegistryOps.create(JsonOps.INSTANCE,registryAccess()),generator).getOrThrow());var origin=state.getDimensionOrigin();dimension.add("origin",StructureGolden.ints(origin.x(),origin.z()));
            var sets=new JsonArray();var rings=new JsonArray();for(var holder:state.possibleStructureSets()) {
                sets.add(holder.unwrapKey().orElseThrow().identifier().toString());
                if(holder.value().placement() instanceof net.minecraft.world.level.levelgen.structure.placement.ConcentricRingsStructurePlacement ring) {
                    var positions=state.getRingPositionsFor(ring);if(positions!=null) {var value=new JsonObject();value.addProperty("set",holder.unwrapKey().orElseThrow().identifier().toString());var list=new JsonArray();for(var pos:positions)list.add(StructureGolden.ints(pos.x(),pos.z()));value.add("positions",list);rings.add(value);}
                }
            }
            dimension.add("sets",sets);dimension.add("rings",rings);var chunks=new JsonArray();System.out.println("CAPTURE structure starts "+key.identifier()+" seed="+seed+" area=33x33");
            for(int x=-16;x<=16;x++)for(int z=-16;z<=16;z++) {
                var chunk=level.getChunk(x,z,ChunkStatus.STRUCTURE_STARTS);var item=new JsonObject();item.add("chunk",StructureGolden.ints(x,z));var starts=new JsonArray();
                var entries=new ArrayList<>(chunk.getAllStarts().entrySet());entries.sort(Comparator.comparing(e->registry.getKey(e.getKey()).toString()));
                for(var entry:entries) {var value=new JsonObject();value.addProperty("structure",registry.getKey(entry.getKey()).toString());value.addProperty("nbt",entry.getValue().createTag(context,new ChunkPos(x,z)).toString());starts.add(value);}
                item.add("starts",starts);chunks.add(item);
            }
            dimension.add("chunks",chunks);dimensions.add(dimension);
            StructureGolden.write(output.resolve("structure_starts_"+seed+".json.gz"),dimensions);
        }
    }
    void capture(long seed,Path output)throws Exception {
        var cases=new JsonArray();
        for(String name:List.of("village_plains","stronghold","mineshaft","fortress","monument","mansion","ancient_city","trial_chambers","bastion_remnant","end_city")) {
            var dimension=name.equals("fortress")||name.equals("bastion_remnant")?Level.NETHER:name.equals("end_city")?Level.END:Level.OVERWORLD;
            var level=getLevel(dimension);var generator=level.getChunkSource().getGenerator();var registry=registryAccess().lookupOrThrow(Registries.STRUCTURE);var holder=registry.getOrThrow(ResourceKey.create(Registries.STRUCTURE,Identifier.withDefaultNamespace(name)));
            System.out.println("CAPTURE locating "+name+" seed="+seed);
            var located=generator.findNearestMapStructure(level,HolderSet.direct(List.of(holder)),BlockPos.ZERO,100,false);if(located==null)throw new IllegalStateException("No located "+name);
            var origin=ChunkPos.containing(located.getFirst());var starts=level.getChunk(origin.x(),origin.z(),ChunkStatus.STRUCTURE_STARTS);var start=level.structureManager().getStartForStructure(holder.value(),starts);if(start==null||!start.isValid())throw new AssertionError("missing located start");
            var context=new StructurePieceSerializationContext(level.getServer().getResourceManager(),registryAccess(),getStructureTemplateManager());
            var before=start.createTag(context,origin);var first=start.getPieces().getFirst().getBoundingBox().getCenter();var target=ChunkPos.containing(first);
            System.out.println("CAPTURE generating "+name+" origin="+origin+" chunk="+target);
            var chunk=level.getChunk(target.x(),target.z(),ChunkStatus.FEATURES);if(!(chunk instanceof ProtoChunk proto))throw new AssertionError("FEATURES was upgraded before capture");
            var item=new JsonObject();item.addProperty("structure","minecraft:"+name);item.addProperty("seed",seed);item.addProperty("dimension",dimension.identifier().toString());item.add("origin_chunk",StructureGolden.ints(origin.x(),origin.z()));item.add("chunk",StructureGolden.ints(target.x(),target.z()));item.addProperty("min_y",chunk.getMinY());item.addProperty("height",chunk.getHeight());item.addProperty("start_before",before.toString());
            var blocks=new JsonArray();var p=new BlockPos.MutableBlockPos();for(int y=chunk.getMinY();y<=chunk.getMaxY();y++)for(int z=0;z<16;z++)for(int x=0;x<16;x++)blocks.add(Block.BLOCK_STATE_REGISTRY.getId(chunk.getBlockState(p.set(target.getMinBlockX()+x,y,target.getMinBlockZ()+z))));item.add("blocks",blocks);
            var blockEntities=new JsonArray();var positions=new ArrayList<>(chunk.getBlockEntitiesPos());positions.sort(Comparator.comparingInt((BlockPos candidate)->candidate.getY()).thenComparingInt(candidate->candidate.getX()).thenComparingInt(candidate->candidate.getZ()));for(var pos:positions){var tag=chunk.getBlockEntityNbtForSaving(pos,registryAccess());if(tag!=null)blockEntities.add(tag.toString());}item.add("block_entities",blockEntities);
            var entities=new JsonArray();for(var tag:proto.getEntities())entities.add(tag.toString());item.add("entities",entities);
            var beardOrder=new JsonArray();for(var structure:chunk.getAllReferences().keySet())beardOrder.add(registry.getKey(structure).toString());item.add("structure_order",beardOrder);
            var allStarts=new JsonArray();for(var entry:chunk.getAllStarts().entrySet())allStarts.add(entry.getValue().createTag(context,target).toString());item.add("starts",allStarts);cases.add(item);
            StructureGolden.write(output.resolve("generated_structures_"+seed+".json.gz"),cases);
        }
    }
    @Override protected boolean initServer(){return true;}
    @Override public LevelBasedPermissionSet operatorUserPermissions(){return LevelBasedPermissionSet.ALL;}
    @Override public PermissionSet getFunctionCompilationPermissions(){return LevelBasedPermissionSet.OWNER;}
    @Override public boolean shouldRconBroadcast(){return false;}
    @Override protected SampleLogger getTickTimeLogger(){return sampleLogger;}
    @Override public boolean isTickTimeLoggingEnabled(){return false;}
    @Override public SystemReport fillServerSystemReport(SystemReport report){report.setDetail("Type","Structure fixture server");return report;}
    @Override public boolean isDedicatedServer(){return false;}
    @Override public int getMaxPlayers(){return 0;}
    @Override public int getRateLimitPacketsPerSecond(){return 0;}
    @Override public int getCommandSpamThresholdSeconds(){return 0;}
    @Override public int getChatSpamThresholdSeconds(){return 0;}
    @Override public boolean useNativeTransport(){return false;}
    @Override public boolean isPublished(){return false;}
    @Override public boolean shouldInformAdmins(){return false;}
    @Override public boolean isSingleplayerOwner(NameAndId id){return false;}
    @Override protected void waitUntilNextTick(){runAllTasks();}
    public static void main(String[]args)throws Exception {
        SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();var output=Path.of(args[0]).toAbsolutePath();Files.createDirectories(output);
        for(long seed:new long[]{0,8675309}) {
            Path temporary=Files.createTempDirectory("ferraloom-structure-world-");
            try {
                var source=LevelStorageSource.createDefault(temporary);var storage=source.createAccess("fixture");var packs=ServerPacksSource.createPackRepository(storage);var stem=load(packs,seed);
                try(var server=new GeneratedChunksGolden(storage,packs,stem)){server.captureStarts(seed,output);server.capture(seed,output);}
            } finally {try(var paths=Files.walk(temporary)){for(var path:paths.sorted(Comparator.reverseOrder()).toList())Files.deleteIfExists(path);}}
        }
        Util.shutdownExecutors();
    }
}
