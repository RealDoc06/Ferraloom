import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;

/** Pure bootstrapped FireBlock predicates and ConcretePowder constructor facts. */
public final class ExtractBuildingProperties {
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var root=new JsonObject();var burn=new JsonArray();var concrete=new JsonArray();var odds=new JsonArray();
        var canBurn=FireBlock.class.getDeclaredMethod("canBurn",BlockState.class);canBurn.setAccessible(true);
        var igniteOdds=FireBlock.class.getDeclaredMethod("getIgniteOdds",BlockState.class);igniteOdds.setAccessible(true);
        var burnOdds=FireBlock.class.getDeclaredMethod("getBurnOdds",BlockState.class);burnOdds.setAccessible(true);
        var target=ConcretePowderBlock.class.getDeclaredField("concrete");target.setAccessible(true);
        for(var state:Block.BLOCK_STATE_REGISTRY){
            if((boolean)canBurn.invoke(Blocks.FIRE,state))burn.add(Block.getId(state));
            int ignite=(int)igniteOdds.invoke(Blocks.FIRE,state),burnValue=(int)burnOdds.invoke(Blocks.FIRE,state);
            if(ignite<0||ignite>255||burnValue<0||burnValue>255)throw new IllegalStateException("fire odds exceed u8");
            if(ignite!=0||burnValue!=0){var row=new JsonArray();row.add(Block.getId(state));row.add(ignite);row.add(burnValue);odds.add(row);}
        }
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof ConcretePowderBlock){
            var pair=new JsonArray();pair.add(BuiltInRegistries.BLOCK.getId(block));pair.add(BuiltInRegistries.BLOCK.getId((Block)target.get(block)));concrete.add(pair);
        }
        root.addProperty("state_count",Block.BLOCK_STATE_REGISTRY.size());
        root.addProperty("block_count",BuiltInRegistries.BLOCK.size());
        root.add("can_burn",burn);root.add("concrete",concrete);root.add("fire_odds",odds);
        Files.writeString(Path.of(args[0]),new Gson().toJson(root)+"\n");
    }
}
