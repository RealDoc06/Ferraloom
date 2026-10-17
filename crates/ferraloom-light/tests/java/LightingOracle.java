import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.lighting.*;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.shapes.Shapes;

/** Our constructed worlds, vanilla's real chunks, source maps and light engine. */
public final class LightingOracle implements LightChunkGetter, BlockGetter {
    final int minSection = -2;
    final int maxSection;
    final boolean sky;
    final LinkedHashMap<ChunkPos, ProtoChunk> chunks = new LinkedHashMap<>();
    LevelLightEngine engine;
    final DataOutputStream out;
    static PalettedContainerFactory factory;

    LightingOracle(Path path, int maxSection, boolean sky) throws Exception {
        this.maxSection = maxSection;
        this.sky = sky;
        this.out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(path)));
        out.writeInt(0x464c3236); out.writeInt(minSection); out.writeInt(maxSection); out.writeBoolean(sky);
        for (int z=-1; z<=1; z++) for(int x=-1; x<=1; x++) {
            var pos = new ChunkPos(x,z);
            chunks.put(pos, new ProtoChunk(pos, UpgradeData.EMPTY, this, factory, null));
        }
        engine = new LevelLightEngine(this,true,sky);
    }
    public int getMinY() { return minSection*16; }
    public int getHeight() { return (maxSection-minSection)*16; }
    public ProtoChunk getChunkForLighting(int x,int z) { return chunks.get(new ChunkPos(x,z)); }
    public BlockGetter getLevel() { return this; }
    public BlockState getBlockState(BlockPos p) {
        var c = getChunkForLighting(p.getX()>>4,p.getZ()>>4);
        return c == null ? Blocks.BEDROCK.defaultBlockState() : c.getBlockState(p);
    }
    public BlockEntity getBlockEntity(BlockPos p) { return null; }
    public FluidState getFluidState(BlockPos p) { return getBlockState(p).getFluidState(); }
    void set(int x,int y,int z,BlockState state,boolean check) throws Exception {
        out.writeByte(check ? 4 : 1); out.writeInt(x); out.writeInt(y); out.writeInt(z); out.writeShort(Block.getId(state));
        var p = new BlockPos(x,y,z);
        var chunk = getChunkForLighting(x>>4,z>>4);
        var section = chunk.getSection(chunk.getSectionIndex(y));
        boolean wasEmpty = section.hasOnlyAir();
        chunk.setBlockState(p,state);
        if(check) {
            chunk.getSkyLightSources().update(chunk,x&15,y,z&15);
            if(wasEmpty != section.hasOnlyAir()) engine.updateSectionStatus(SectionPos.of(p),section.hasOnlyAir());
            engine.checkBlock(p);
        }
    }
    void set(int x,int y,int z,Block block) throws Exception { set(x,y,z,block.defaultBlockState(),false); }
    void fill(int x0,int y0,int z0,int x1,int y1,int z1,Block block) throws Exception {
        for(int y=y0;y<=y1;y++) for(int z=z0;z<=z1;z++) for(int x=x0;x<=x1;x++) set(x,y,z,block);
    }
    void initialize(boolean lighted) throws Exception {
        out.writeByte(2); out.writeBoolean(lighted);
        for(var c:chunks.values()) {
            c.initializeLightSources();
            for(int y=minSection;y<maxSection;y++) if(!c.getSection(y-minSection).hasOnlyAir()) engine.updateSectionStatus(SectionPos.of(c.getPos(),y),false);
        }
        engine.runLightUpdates();
        for(var p:chunks.keySet()) { engine.setLightEnabled(p,lighted); engine.retainData(p,false); }
    }
    void light() throws Exception {
        out.writeByte(3);
        for(var p:chunks.keySet()) engine.propagateLightSources(p);
        engine.runLightUpdates();
    }
    void enabled(int x,int z,boolean enabled) throws Exception {
        out.writeByte(6);out.writeInt(x);out.writeInt(z);out.writeBoolean(enabled);
        engine.setLightEnabled(new ChunkPos(x,z),enabled);
    }
    void status(int x,int y,int z,boolean empty) throws Exception {
        out.writeByte(7);out.writeInt(x);out.writeInt(y);out.writeInt(z);out.writeBoolean(empty);
        engine.updateSectionStatus(SectionPos.of(x,y,z),empty);
    }
    void retain(int x,int z,boolean retain) throws Exception {
        out.writeByte(8);out.writeInt(x);out.writeInt(z);out.writeBoolean(retain);
        engine.retainData(new ChunkPos(x,z),retain);
    }
    void queued(int x,int y,int z,LightLayer layer,int value) throws Exception {
        out.writeByte(9);out.writeInt(x);out.writeInt(y);out.writeInt(z);out.writeBoolean(layer==LightLayer.SKY);out.writeByte(value);
        engine.queueSectionData(layer,SectionPos.of(x,y,z),value<0?null:new DataLayer(value));
    }
    void diskReload() throws Exception {
        out.writeByte(10);
        var old = engine;
        engine = new LevelLightEngine(this,true,sky);
        for(var p:chunks.keySet()) {
            engine.retainData(p,true);
            for(int y=minSection-1;y<=maxSection;y++) for(var layer:LightLayer.values()) {
                var data=old.getLayerListener(layer).getDataLayerData(SectionPos.of(p,y));
                if(data!=null) engine.queueSectionData(layer,SectionPos.of(p,y),data.copy());
            }
        }
        // Same pre-update and post-update split as initializeLight.
        for(var c:chunks.values()) for(int y=minSection;y<maxSection;y++) if(!c.getSection(y-minSection).hasOnlyAir()) engine.updateSectionStatus(SectionPos.of(c.getPos(),y),false);
        engine.runLightUpdates();
        for(var p:chunks.keySet()) { engine.setLightEnabled(p,true); engine.retainData(p,false); }
        engine.runLightUpdates();
    }
    void checkpoint() throws Exception {
        out.writeByte(5);
        engine.runLightUpdates();
        for(int z=-2;z<=2;z++) for(int x=-2;x<=2;x++) for(int y=minSection-1;y<=maxSection;y++) {
            var s=SectionPos.of(x,y,z);
            for(var layer:new LightLayer[]{LightLayer.BLOCK,LightLayer.SKY}) {
                var listener=engine.getLayerListener(layer);
                var data=listener.getDataLayerData(s);
                out.writeByte(engine.getDebugSectionType(layer,s).ordinal());
                out.writeBoolean(data!=null);
                if(data!=null) {
                    int homogeneous=255;
                    for(int v=0;v<=15;v++) if(data.isDefinitelyFilledWith(v)) homogeneous=v;
                    out.writeByte(homogeneous);
                    rle(out,data.copy().getData());
                }
                byte[] queried=new byte[2048];
                for(int i=0;i<4096;i++) {
                    var b=new BlockPos(x*16+(i&15),y*16+(i>>8),z*16+((i>>4)&15));
                    int value=listener.getLightValue(b);
                    queried[i>>1]|=(byte)(value<<((i&1)*4));
                }
                rle(out,queried);
            }
        }
    }
    static void rle(DataOutputStream out,byte[] bytes) throws Exception {
        int count=0;
        for(int i=0;i<bytes.length;) { int j=i+1;while(j<bytes.length&&bytes[j]==bytes[i])j++;count++;i=j; }
        out.writeInt(count);
        for(int i=0;i<bytes.length;) { int j=i+1;while(j<bytes.length&&bytes[j]==bytes[i])j++;out.writeShort(j-i);out.writeByte(bytes[i]);i=j; }
    }
    void close() throws Exception { out.writeByte(0); out.close(); }
    static void faces(Path output) throws Exception {
        var representatives=new LinkedHashMap<String,BlockState>();
        for(var state:Block.BLOCK_STATE_REGISTRY) {
            String key=state.useShapeForLightOcclusion()+":"+state.getOcclusionShape().toAabbs();
            representatives.putIfAbsent(key,state);
        }
        try(var out=new DataOutputStream(Files.newOutputStream(output.resolve("faces.bin")))) {
            var states=new ArrayList<>(representatives.values());
            out.writeInt(states.size());
            for(var s:states) out.writeShort(Block.getId(s));
            for(var a:states) for(var b:states) for(var d:Direction.values()) {
                out.writeBoolean(Shapes.faceShapeOccludes(LightEngine.getOcclusionShape(a,d),LightEngine.getOcclusionShape(b,d.getOpposite())));
                out.writeByte(LightEngine.getLightDampeningInto(a,b,d,3));
            }
        }
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var lookup=VanillaRegistries.createWorldLookup();
        var biomes=lookup.lookupOrThrow(net.minecraft.core.registries.Registries.BIOME);
        var biomeIds=new IdMapper<Holder<net.minecraft.world.level.biome.Biome>>();
        biomes.listElements().forEach(biomeIds::add);
        var blocks=Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
        var biomeStrategy=Strategy.createForBiomes(biomeIds);
        var air=Blocks.AIR.defaultBlockState();
        var plains=biomes.getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS);
        factory=new PalettedContainerFactory(blocks,air,PalettedContainer.codecRW(BlockState.CODEC,blocks,air),biomeStrategy,plains,PalettedContainer.codecRO(com.mojang.serialization.Codec.INT.xmap(i -> (Holder<net.minecraft.world.level.biome.Biome>) plains, b -> 0),biomeStrategy,plains));
        Path output=Path.of(args[0]);Files.createDirectories(output);
        faces(output);
        var w=new LightingOracle(output.resolve("open.bin"),4,true);
        w.initialize(false);w.light();w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("caves.bin"),4,true);
        w.fill(-16,-32,-16,31,3,31,Blocks.STONE);
        w.fill(-12,-20,-12,12,-10,12,Blocks.AIR);
        w.fill(-2,-10,-2,2,4,2,Blocks.AIR);
        w.initialize(false);w.light();w.checkpoint();w.diskReload();w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("room.bin"),4,true);
        w.fill(0,-4,0,12,8,12,Blocks.STONE);w.fill(1,-3,1,11,7,11,Blocks.AIR);
        w.set(6,1,6,Blocks.GLOWSTONE);w.set(9,1,6,Blocks.TORCH);
        w.initialize(false);w.light();w.checkpoint();
        w.set(6,1,6,Blocks.AIR.defaultBlockState(),true);w.checkpoint();
        w.set(6,1,6,Blocks.GLOWSTONE.defaultBlockState(),true);w.checkpoint();
        w.set(6,8,6,Blocks.AIR.defaultBlockState(),true);w.checkpoint();
        w.set(6,8,6,Blocks.STONE.defaultBlockState(),true);w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("directional.bin"),4,true);
        w.fill(-4,-4,-4,20,7,20,Blocks.STONE);w.fill(-3,-3,-3,19,6,19,Blocks.AIR);
        int n=0;
        for(var d:Direction.Plane.HORIZONTAL) for(var half:Half.values()) for(var stair:StairsShape.values()) {
            int x=(n%10)*2-2,z=(n/10)*4-2;n++;
            var s=Blocks.OAK_STAIRS.defaultBlockState().setValue(StairBlock.FACING,d).setValue(StairBlock.HALF,half).setValue(StairBlock.SHAPE,stair);
            w.set(x,0,z,s,false);w.set(x,0,z+1,Blocks.GLOWSTONE);
            w.set(x,1,z,Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,half==Half.TOP?SlabType.TOP:SlabType.BOTTOM),false);
        }
        w.initialize(false);w.light();w.checkpoint();
        w.set(-2,0,-2,Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP),true);w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("dampening.bin"),4,true);
        w.fill(0,-16,0,15,-16,15,Blocks.STONE);
        w.fill(2,-15,2,5,20,5,Blocks.WATER);w.fill(7,-15,2,10,20,5,Blocks.OAK_LEAVES);
        w.set(3,-10,3,Blocks.SEA_LANTERN);w.set(8,0,3,Blocks.GLOWSTONE);
        w.initialize(false);w.light();w.checkpoint();
        w.set(3,20,3,Blocks.AIR.defaultBlockState(),true);w.set(8,20,3,Blocks.AIR.defaultBlockState(),true);w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("overhang.bin"),4,true);
        w.fill(-8,16,-8,23,16,23,Blocks.STONE);w.set(15,-1,15,Blocks.GLOWSTONE);
        w.initialize(false);w.light();w.checkpoint();
        for(int y=0;y<=15;y++) w.set(0,y,0,Blocks.STONE.defaultBlockState(),true);
        w.checkpoint();w.set(0,16,0,Blocks.AIR.defaultBlockState(),true);w.checkpoint();
        w.set(0,32,0,Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.BOTTOM),true);w.checkpoint();
        w.set(0,32,0,Blocks.AIR.defaultBlockState(),true);w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("boundaries.bin"),4,true);
        w.set(-1,-1,-1,Blocks.GLOWSTONE);w.set(-16,15,15,Blocks.LAVA);w.set(16,16,16,Blocks.SEA_LANTERN);
        w.fill(-16,32,-16,31,32,31,Blocks.STONE);w.set(-1,32,-1,Blocks.AIR);
        w.initialize(false);w.light();w.checkpoint();
        w.set(-1,-1,-1,Blocks.AIR.defaultBlockState(),true);w.set(16,16,16,Blocks.AIR.defaultBlockState(),true);w.checkpoint();
        w.set(-1,-1,-1,Blocks.GLOWSTONE.defaultBlockState(),true);w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("gaps.bin"),9,true);
        w.fill(-16,112,-16,-1,112,-1,Blocks.STONE);w.set(0,-31,0,Blocks.STONE);
        w.initialize(false);w.light();w.checkpoint();
        w.set(-1,112,-1,Blocks.AIR.defaultBlockState(),true);w.checkpoint();
        w.set(0,48,0,Blocks.STONE.defaultBlockState(),true);w.checkpoint();
        w.set(0,48,0,Blocks.AIR.defaultBlockState(),true);w.checkpoint();w.close();

        w=new LightingOracle(output.resolve("lifecycle.bin"),4,false);
        w.set(0,0,0,Blocks.GLOWSTONE);w.initialize(false);w.light();w.checkpoint();
        w.retain(0,0,true);w.status(0,0,0,true);w.checkpoint();
        w.status(0,0,0,false);w.checkpoint();w.diskReload();w.checkpoint();
        w.enabled(0,0,false);w.set(0,0,0,Blocks.GLOWSTONE.defaultBlockState(),true);w.checkpoint();
        w.enabled(0,0,true);w.set(0,0,0,Blocks.GLOWSTONE.defaultBlockState(),true);w.checkpoint();
        w.queued(0,0,0,LightLayer.BLOCK,4);w.checkpoint();
        w.queued(0,0,0,LightLayer.BLOCK,-1);w.checkpoint();w.close();
    }
}
