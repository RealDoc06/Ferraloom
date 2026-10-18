import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

/** Calls the real Biome predicates through the concrete inputs of LevelReader. */
public final class WeatherVectors {
    static BlockState state(String kind){return switch(kind){
        case "water" -> Blocks.WATER.defaultBlockState();
        case "flowing" -> Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,3);
        case "snow" -> Blocks.SNOW.defaultBlockState();
        case "stone" -> Blocks.STONE.defaultBlockState();
        default -> Blocks.AIR.defaultBlockState();
    };}
    static LevelReader reader(BlockPos center,BlockState state,int light,boolean surrounded){
        return (LevelReader)Proxy.newProxyInstance(WeatherVectors.class.getClassLoader(),new Class[]{LevelReader.class},(proxy,method,args)->{
            var name=method.getName();
            if(name.equals("getMinY"))return -16;
            if(name.equals("getHeight"))return 32;
            if(name.equals("getSeaLevel"))return 63;
            if(name.equals("getBrightness"))return light;
            if(name.equals("getBlockState")||name.equals("getFluidState")){
                var p=(BlockPos)args[0];
                var value=p.equals(center)?state:p.equals(center.below())?Blocks.STONE.defaultBlockState():
                    surrounded&&p.getY()==center.getY()&&Math.abs(p.getX()-center.getX())+Math.abs(p.getZ()-center.getZ())==1?Blocks.WATER.defaultBlockState():Blocks.AIR.defaultBlockState();
                return name.equals("getFluidState")?value.getFluidState():value;
            }
            if(method.isDefault())return InvocationHandler.invokeDefault(proxy,method,args);
            throw new UnsupportedOperationException(method.toString());
        });
    }
    public static void main(String[] args)throws Exception{
        var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
        var registry=NeighborVectors.load().lookupOrThrow(Registries.BIOME);
        for(var biomeName:List.of("plains","snowy_plains","desert","frozen_ocean")){
            var biome=registry.getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.BIOME,Identifier.withDefaultNamespace(biomeName))).value();
            for(int light:new int[]{0,10})for(var kind:List.of("air","water","flowing","snow","stone"))for(int y:new int[]{-17,0,16})for(boolean surrounded:new boolean[]{false,true}){
                var p=new BlockPos(8,y,8);var level=reader(p,state(kind),light,surrounded);
                System.out.println(biomeName+":"+light+":"+kind+":"+y+":"+surrounded+"\t"+biome.shouldFreeze(level,p,false)+","+biome.shouldFreeze(level,p,true)+","+biome.shouldSnow(level,p)+","+biome.getPrecipitationAt(p,63).getSerializedName());
            }
        }
    }
}
