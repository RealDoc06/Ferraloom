import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.*;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.lighting.LevelLightEngine;

/** Calls the actual engine with and without skylight, including Java int wrap. */
public final class RawBrightnessVectors {
    public static void main(String[] args) {
        var out=System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var getter=(BlockGetter)Proxy.newProxyInstance(RawBrightnessVectors.class.getClassLoader(),new Class[]{BlockGetter.class},(proxy,method,values)->{
            if(method.getName().equals("getMinY"))return -64;
            if(method.getName().equals("getHeight"))return 384;
            if(method.isDefault())return InvocationHandler.invokeDefault(proxy,method,values);
            throw new UnsupportedOperationException(method.toString());
        });
        var chunks=new LightChunkGetter(){
            public LightChunk getChunkForLighting(int x,int z){return null;}
            public BlockGetter getLevel(){return getter;}
        };
        var pos=new BlockPos(8,8,8);
        for(boolean sky:new boolean[]{false,true}) {
            var engine=new LevelLightEngine(chunks,true,sky);
            for(int dampen:new int[]{Integer.MIN_VALUE,Integer.MIN_VALUE+15,-16,-15,-1,0,1,15,16,Integer.MAX_VALUE-15,Integer.MAX_VALUE})
                System.out.println(sky+"\t"+engine.getLayerListener(LightLayer.SKY).getLightValue(pos)+"\t"+engine.getLayerListener(LightLayer.BLOCK).getLightValue(pos)+"\t"+dampen+"\t"+engine.getRawBrightness(pos,dampen));
        }
    }
}
