import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.biome.Biomes;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;

/** Real Biome#shouldFreeze with replaced datapack water-tag membership. */
public final class FreezeGolden {
    static BlockState state(String name) {
        return switch(name) {
            case "water" -> Blocks.WATER.defaultBlockState();
            case "flowing_water" -> Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,1);
            case "lava" -> Blocks.LAVA.defaultBlockState();
            default -> throw new AssertionError(name);
        };
    }
    static void generate(RegistryAccess.Frozen registries, Path out) throws Exception {
        var bind=Holder.Reference.class.getDeclaredMethod("bindTags",Collection.class);bind.setAccessible(true);
        var previous=new LinkedHashMap<Holder.Reference<Fluid>,List<net.minecraft.tags.TagKey<Fluid>>>();
        for(var fluid:BuiltInRegistries.FLUID) {
            var holder=fluid.builtInRegistryHolder();previous.put(holder,holder.tags().toList());
        }
        var cases=new JsonArray();var p=new BlockPos(8,64,8);
        var biome=registries.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.SNOWY_PLAINS).value();
        try {
            for(var members:List.of(List.of("water","flowing_water"),List.of("lava"),List.<String>of())) {
                for(var entry:previous.entrySet()) {
                    var tags=new ArrayList<>(entry.getValue());tags.remove(FluidTags.WATER);
                    if(members.contains(BuiltInRegistries.FLUID.getKey(entry.getKey().value()).getPath()))tags.add(FluidTags.WATER);
                    bind.invoke(entry.getKey(),tags);
                }
                for(String center:List.of("water","flowing_water","lava"))for(String neighbor:List.of("water","flowing_water","lava"))for(boolean check:List.of(false,true)) {
                    var level=(LevelReader)Proxy.newProxyInstance(FreezeGolden.class.getClassLoader(),new Class<?>[]{LevelReader.class},(object,method,args)->switch(method.getName()) {
                        case "getBlockState" -> state(p.equals(args[0])?center:neighbor);
                        case "getFluidState" -> state(p.equals(args[0])?center:neighbor).getFluidState();
                        case "getSeaLevel" -> 63;
                        case "getBrightness" -> 0;
                        case "getMinY" -> -64;
                        case "getHeight" -> 384;
                        default -> {
                            if(method.isDefault())yield InvocationHandler.invokeDefault(object,method,args);
                            throw new AssertionError(method.toString());
                        }
                    });
                    var item=new JsonObject();item.add("water_tag",new Gson().toJsonTree(members));item.addProperty("center",center);item.addProperty("neighbor",neighbor);item.addProperty("check_neighbors",check);
                    item.addProperty("freeze",biome.shouldFreeze(level,p,check));cases.add(item);
                }
            }
        } finally {
            for(var entry:previous.entrySet())bind.invoke(entry.getKey(),entry.getValue());
        }
        WorldgenGolden.write(out.resolve("freeze.json.gz"),cases);
    }
}
