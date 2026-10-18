import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.lighting.LevelLightEngine;
import net.minecraft.world.level.material.FluidState;

/** Calls real ProtoChunk#setBlockState and ChunkSkyLightSources, with a real
 * LevelLightEngine. Only callback observations and owned input chunks are ours. */
public final class ProtoLightVectors implements LightChunkGetter, BlockGetter {
    static final int MIN=-16,HEIGHT=48;
    final ProtoChunk chunk;
    final Engine engine;
    final List<String> calls=new ArrayList<>();
    BlockPos current;
    boolean record;
    final class Engine extends LevelLightEngine {
        Engine(boolean sky) {super(ProtoLightVectors.this,true,sky);}
        @Override public void updateSectionStatus(SectionPos pos,boolean empty) {
            if(record) calls.add("section:"+pos.x()+","+pos.y()+","+pos.z()+":"+empty+":"+sky());
            super.updateSectionStatus(pos,empty);
        }
        @Override public void checkBlock(BlockPos pos) {
            if(record) calls.add("check:"+pos.getX()+","+pos.getY()+","+pos.getZ()+":"+sky());
            super.checkBlock(pos);
        }
    }
    ProtoLightVectors(RegistryAccess.Frozen registry,int cx,int cz,boolean sky) {
        chunk=new ProtoChunk(new ChunkPos(cx,cz),UpgradeData.EMPTY,LevelHeightAccessor.create(MIN,HEIGHT),
            PalettedContainerFactory.create(registry),null);
        engine=new Engine(sky);
    }
    public int getMinY() {return MIN;}
    public int getHeight() {return HEIGHT;}
    public ProtoChunk getChunkForLighting(int x,int z) {return chunk.getPos().equals(new ChunkPos(x,z))?chunk:null;}
    public BlockGetter getLevel() {return this;}
    public BlockState getBlockState(BlockPos pos) {return chunk.getBlockState(pos);}
    public BlockEntity getBlockEntity(BlockPos pos) {return null;}
    public FluidState getFluidState(BlockPos pos) {return chunk.getFluidState(pos);}
    int sky() {return chunk.getSkyLightSources().getLowestSourceY(current.getX()&15,current.getZ()&15);}
    static JsonArray point(BlockPos p) {var out=new JsonArray();out.add(p.getX());out.add(p.getY());out.add(p.getZ());return out;}
    BlockPos pos(int x,int y,int z) {return new BlockPos(chunk.getPos().getMinBlockX()+x,y,chunk.getPos().getMinBlockZ()+z);}
    JsonObject write(BlockPos pos,BlockState state,boolean bulk) {
        current=pos;calls.clear();record=true;
        if(bulk) chunk.getSection(chunk.getSectionIndex(pos.getY())).setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,state);
        else chunk.setBlockState(pos,state,16);
        record=false;
        var out=new JsonObject();out.add("pos",point(pos));out.addProperty("state",Block.getId(state));out.addProperty("bulk",bulk);
        out.addProperty("stored",Block.getId(isOutsideBuildHeight(pos.getY())?Blocks.VOID_AIR.defaultBlockState():
            chunk.getSection(chunk.getSectionIndex(pos.getY())).getBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15)));
        out.addProperty("sky",sky());out.addProperty("highest",chunk.getSkyLightSources().getHighestLowestSourceY());
        var heights=new JsonArray();for(var type:Heightmap.Types.values()) heights.add(chunk.getHeight(type,pos.getX()&15,pos.getZ()&15)+1);
        out.add("heights",heights);var events=new JsonArray();calls.forEach(events::add);out.add("calls",events);
        return out;
    }
    JsonObject sequence(ChunkStatus status,boolean sky) {
        chunk.setBlockState(pos(15,-1,0),Blocks.STONE.defaultBlockState(),16);
        for(var type:Heightmap.Types.values()) chunk.getOrCreateHeightmapUnprimed(type);
        Heightmap.primeHeightmaps(chunk,EnumSet.allOf(Heightmap.Types.class));
        chunk.initializeLightSources();chunk.setLightEngine(engine);chunk.setPersistedStatus(status);
        var writes=new JsonArray();
        writes.add(write(pos(15,0,0),Blocks.AIR.defaultBlockState(),false));
        writes.add(write(pos(15,15,0),Blocks.GLOWSTONE.defaultBlockState(),false));
        writes.add(write(pos(15,15,0),Blocks.GLOWSTONE.defaultBlockState(),false));
        writes.add(write(pos(15,15,0),Blocks.STONE.defaultBlockState(),false));
        writes.add(write(pos(15,15,0),Blocks.DIRT.defaultBlockState(),false));
        writes.add(write(pos(15,15,0),Blocks.AIR.defaultBlockState(),false));
        writes.add(write(pos(15,16,0),Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP),false));
        writes.add(write(pos(15,16,0),Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.BOTTOM),false));
        writes.add(write(pos(15,16,0),Blocks.AIR.defaultBlockState(),false));
        writes.add(write(pos(15,HEIGHT+MIN,0),Blocks.STONE.defaultBlockState(),false));
        writes.add(write(pos(15,MIN-1,0),Blocks.STONE.defaultBlockState(),false));
        writes.add(write(pos(15,20,0),Blocks.STONE.defaultBlockState(),true));
        writes.add(write(pos(15,1,0),Blocks.TORCH.defaultBlockState(),false));
        writes.add(write(pos(15,20,0),Blocks.AIR.defaultBlockState(),true));
        writes.add(write(pos(15,-1,0),Blocks.AIR.defaultBlockState(),false));
        writes.add(write(pos(15,31,0),Blocks.CAVE_AIR.defaultBlockState(),true));
        writes.add(write(pos(15,31,0),Blocks.AIR.defaultBlockState(),false));
        var out=new JsonObject();out.addProperty("status",status.getName());out.addProperty("has_sky",sky);
        out.add("chunk",point(new BlockPos(chunk.getPos().x(),0,chunk.getPos().z())));out.add("writes",writes);return out;
    }
    JsonObject staleSource() {
        for(int x=0;x<16;x++)for(int z=0;z<16;z++)chunk.setBlockState(pos(x,-1,z),Blocks.STONE.defaultBlockState(),16);
        chunk.initializeLightSources();chunk.setLightEngine(engine);chunk.setPersistedStatus(ChunkStatus.INITIALIZE_LIGHT);
        engine.updateSectionStatus(SectionPos.of(chunk.getPos(),-1),false);engine.runLightUpdates();
        for(int x=0;x<16;x++)for(int z=0;z<16;z++)chunk.getSection(chunk.getSectionIndex(15)).setBlockState(x,15,z,Blocks.STONE.defaultBlockState());
        var checked=write(pos(8,1,8),Blocks.TORCH.defaultBlockState(),false);
        engine.setLightEnabled(chunk.getPos(),true);engine.propagateLightSources(chunk.getPos());engine.runLightUpdates();
        var out=new JsonObject();out.add("checked",checked);var light=new JsonArray();
        for(int y:new int[]{0,1,14,15,16}) {var p=pos(8,y,8);var row=point(p);row.add(engine.getLayerListener(LightLayer.SKY).getLightValue(p));row.add(engine.getLayerListener(LightLayer.BLOCK).getLightValue(p));light.add(row);}
        out.add("light",light);return out;
    }
    static RegistryAccess.Frozen registries() {
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft")
            .build(new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty())).fullResources();
        var resources=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(resources,builtins).forEach(Registry.PendingTags::apply);
        return RegistryDataLoader.load(resources,builtins.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value()).toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var registry=registries();var cases=new JsonArray();
        for(var status:List.of(ChunkStatus.FEATURES,ChunkStatus.INITIALIZE_LIGHT,ChunkStatus.LIGHT))
            for(boolean sky:new boolean[]{false,true})for(int[] p:new int[][]{{0,0},{5,-3},{-1,1}})
                cases.add(new ProtoLightVectors(registry,p[0],p[1],sky).sequence(status,sky));
        var out=new JsonObject();out.add("cases",cases);out.add("stale_source",new ProtoLightVectors(registry,0,0,true).staleSource());
        Files.writeString(Path.of(args[0]),new Gson().toJson(out));System.exit(0);
    }
}
