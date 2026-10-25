import java.lang.reflect.*;
import it.unimi.dsi.fastutil.longs.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.lighting.*;
import net.minecraft.world.level.material.FluidState;

/** Native getter vectors. Reflection sets only the immutable visible input map;
 * the real storage and LevelLightEngine getters execute without transformation. */
public final class PublishedReadVectors implements LightChunkGetter,BlockGetter {
    public int getMinY(){return -64;}
    public int getHeight(){return 384;}
    public LightChunk getChunkForLighting(int x,int z){return null;}
    public BlockGetter getLevel(){return this;}
    public BlockState getBlockState(BlockPos p){return Blocks.AIR.defaultBlockState();}
    public BlockEntity getBlockEntity(BlockPos p){return null;}
    public FluidState getFluidState(BlockPos p){return getBlockState(p).getFluidState();}
    static Field field(Object owner,String name)throws Exception {
        for(Class<?> c=owner.getClass();c!=null;c=c.getSuperclass()) try {
            var f=c.getDeclaredField(name);f.setAccessible(true);return f;
        } catch(NoSuchFieldException absent) { }
        throw new NoSuchFieldException(name);
    }
    static Object get(Object owner,String name)throws Exception{return field(owner,name).get(owner);}
    static DataLayer pattern(boolean sky) {
        var data=new DataLayer();
        for(int y=0;y<16;y++)for(int z=0;z<16;z++)for(int x=0;x<16;x++)
            data.set(x,y,z,(sky?x+2*y+3*z:2*x+y+z)&15);
        return data;
    }
    static Object storage(LevelLightEngine engine,boolean sky)throws Exception {
        return get(get(engine,sky?"skyEngine":"blockEngine"),"storage");
    }
    static void setup(LevelLightEngine engine,String name,boolean sky)throws Exception {
        if(name.equals("empty"))return;
        var storage=storage(engine,sky); var old=get(storage,"visibleSectionData");
        var layers=new Long2ObjectOpenHashMap<DataLayer>();
        var p=SectionPos.of(9,9,-1);
        if(name.equals("initialized")) {
            if(sky)layers.put(p.asLong(),new DataLayer());
        } else if(sky) {
            layers.put(p.asLong(),pattern(true));
            layers.put(SectionPos.of(9,11,-1).asLong(),pattern(true));
            layers.put(SectionPos.of(-1,0,-1).asLong(),pattern(true));
        } else {
            layers.put(p.asLong(),pattern(false));
            layers.put(SectionPos.of(-1,-2,-1).asLong(),pattern(false));
        }
        Object visible;
        if(sky) {
            var tops=new Long2IntOpenHashMap();
            tops.put(SectionPos.getZeroNode(p.asLong()),12);
            tops.put(SectionPos.getZeroNode(SectionPos.of(-1,0,-1).asLong()),1);
            var constructor=old.getClass().getDeclaredConstructor(Long2ObjectOpenHashMap.class,Long2IntOpenHashMap.class,int.class);
            constructor.setAccessible(true);visible=constructor.newInstance(layers,tops,-5);
        } else {
            var constructor=old.getClass().getDeclaredConstructor(Long2ObjectOpenHashMap.class);
            constructor.setAccessible(true);visible=constructor.newInstance(layers);
        }
        ((DataLayerStorageMap<?>)visible).disableCache();
        field(storage,"visibleSectionData").set(storage,visible);
        // Neither pending disk layers nor the mutable update map is a published read.
        ((DataLayerStorageMap<?>)get(storage,"updatingSectionData")).setLayer(p.asLong(),new DataLayer(15));
        engine.queueSectionData(sky?LightLayer.SKY:LightLayer.BLOCK,p,new DataLayer(14));
    }
    public static void main(String[]args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var world=new PublishedReadVectors();
        var points=new BlockPos[]{new BlockPos(153,158,-1),new BlockPos(153,174,-1),
            new BlockPos(153,177,-1),new BlockPos(153,190,-1),new BlockPos(153,191,-1),
            new BlockPos(153,192,-1),new BlockPos(153,-96,-1),new BlockPos(169,158,-1),
            new BlockPos(-1,-17,-1),new BlockPos(-1,15,-1),new BlockPos(-1,16,-1),
            new BlockPos(-17,158,-17)};
        for(String name:new String[]{"empty","initialized","pattern"}) {
            var engine=new LevelLightEngine(world,true,true);setup(engine,name,true);setup(engine,name,false);
            for(var p:points)Bootstrap.realStdoutPrintln("PUBLISHED_READ_VECTOR\t"+name+"\t"+p.getX()+"\t"+p.getY()+"\t"+p.getZ()+"\t"
                +engine.getLayerListener(LightLayer.SKY).getLightValue(p)+"\t"
                +engine.getLayerListener(LightLayer.BLOCK).getLightValue(p)+"\t"
                +engine.getRawBrightness(p,0)+"\t"+engine.getRawBrightness(p,5));
        }
    }
}
