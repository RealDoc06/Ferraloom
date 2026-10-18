import com.google.gson.*;
import java.lang.reflect.Proxy;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.blending.*;
public final class CarvingFilterGolden {
    static void generate(RegistryAccess.Frozen registries,Path out) throws Exception {
        var cases=new JsonArray();
        var ctor=BlendingData.class.getDeclaredConstructor(int.class,int.class,Optional.class);ctor.setAccessible(true);
        var create=Blender.class.getDeclaredMethod("createAroundOldChunksCarvingMaskFilter",WorldGenLevel.class,ChunkAccess.class);create.setAccessible(true);
        var layouts=new int[][][]{{},{{0,0,0,16}},{{-1,0,-4,4},{1,1,0,8}},{{0,0,-4,8},{0,1,2,10},{1,0,-4,16}}};
        for(int[][] layout:layouts) {
            var chunks=new HashMap<ChunkPos,ProtoChunk>();
            for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++) {BlendingData data=null;for(int[] old:layout)if(x==old[0]&&z==old[1])data=ctor.newInstance(old[2],old[3],Optional.empty());var p=new ChunkPos(x,z);chunks.put(p,new ProtoChunk(p,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),data));}
            var region=(WorldGenLevel)Proxy.newProxyInstance(CarvingFilterGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(obj,method,args)-> {if(method.getName().equals("getChunk"))return chunks.get(new ChunkPos((int)args[0],(int)args[1]));throw new UnsupportedOperationException(method.toString());});
            var filter=(CarvingMask.Filter)create.invoke(null,region,chunks.get(new ChunkPos(0,0)));
            var item=new JsonObject();var old=new JsonArray();for(int[] entry:layout)old.add(FeaturesGolden.values(entry));item.add("old",old);var samples=new JsonArray();
            for(int x=0;x<16;x+=3)for(int z=0;z<16;z+=3)for(int y=-68;y<264;y+=7)samples.add(filter==null||filter.test(x,y,z));item.add("samples",samples);cases.add(item);
        }
        WorldgenGolden.write(out.resolve("carving_filter.json.gz"),cases);
    }
}
