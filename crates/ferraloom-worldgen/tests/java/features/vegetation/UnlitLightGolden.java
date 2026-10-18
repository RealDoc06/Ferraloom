import com.google.gson.*;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.lighting.LevelLightEngine;

/** Feature-stage light storage is not initialized, even after placing an emitter. */
public final class UnlitLightGolden {
    static void generate(net.minecraft.core.RegistryAccess.Frozen registries,Path path) throws Exception {
        var samples=new JsonArray();
        for(boolean sky:new boolean[]{true,false}) {
            var proto=new ProtoChunk(new ChunkPos(0,0),UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),null);
            proto.setPersistedStatus(net.minecraft.world.level.chunk.status.ChunkStatus.FEATURES);
            LightChunkGetter chunks=new LightChunkGetter() {
                public LightChunk getChunkForLighting(int x,int z) {return x==0 && z==0?proto:null;}
                public BlockGetter getLevel() {return proto;}
            };
            var engine=new LevelLightEngine(chunks,true,sky);
            for(boolean emitter:new boolean[]{false,true}) {
                if(emitter)proto.setBlockState(new BlockPos(0,70,0),Blocks.GLOWSTONE.defaultBlockState(),2);
                // Feature writes only mutate protochunk storage. INITIALIZE_LIGHT
                // has not enabled sections or registered any emission sources.
                for(int[] p:new int[][]{{0,70,0},{1,70,0},{0,10,0},{0,319,0}}) {
                    var pos=new BlockPos(p[0],p[1],p[2]);var item=new JsonObject();
                    item.addProperty("has_sky",sky);item.addProperty("emitter",emitter);item.add("pos",WorldgenGolden.point(p));
                    item.addProperty("block",engine.getLayerListener(LightLayer.BLOCK).getLightValue(pos));
                    item.addProperty("sky",engine.getLayerListener(LightLayer.SKY).getLightValue(pos));samples.add(item);
                }
            }
        }
        WorldgenGolden.write(path,samples);
    }
}
