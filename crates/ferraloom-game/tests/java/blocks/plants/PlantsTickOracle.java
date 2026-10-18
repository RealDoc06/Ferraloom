import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.named;

/** A test-only agent runs scenarios on an actual dedicated ServerLevel. The
 * launched server owns a disposable flat world; no Minecraft object is mocked. */
public final class PlantsTickOracle {
    static Path output;
    static final AtomicBoolean ran=new AtomicBoolean();
    public static void premain(String argument,Instrumentation instrumentation) {
        output=Path.of(argument);
        new AgentBuilder.Default().disableClassFormatChanges()
            .type(named("net.minecraft.server.level.ServerLevel"))
            .transform((builder,type,loader,module,domain)->builder.visit(Advice.to(OnTick.class).on(named("tick"))))
            .installOn(instrumentation);
    }
    public static class OnTick {
        @Advice.OnMethodEnter public static void before(@Advice.This Object level) { PlantsTickOracle.run(level); }
    }
    public static void run(Object input) {
        if(!ran.compareAndSet(false,true))return;
        var level=(net.minecraft.server.level.ServerLevel)input;
        try {
            var lines=new ArrayList<String>();
            var pos=new net.minecraft.core.BlockPos(8,100,8);
            var blocks=List.of(net.minecraft.world.level.block.Blocks.WHEAT,net.minecraft.world.level.block.Blocks.BEETROOTS,net.minecraft.world.level.block.Blocks.TORCHFLOWER_CROP,net.minecraft.world.level.block.Blocks.NETHER_WART);
            for(var block:blocks)for(var state:block.getStateDefinition().getPossibleStates())for(int moisture:new int[]{0,7})for(int pattern=0;pattern<5;pattern++)for(int seed=0;seed<32;seed++) {
                for(int x=-2;x<=2;x++)for(int y=-1;y<=1;y++)for(int z=-2;z<=2;z++)level.setBlock(pos.offset(x,y,z),net.minecraft.world.level.block.Blocks.AIR.defaultBlockState(),816);
                var soil=(block==net.minecraft.world.level.block.Blocks.NETHER_WART?net.minecraft.world.level.block.Blocks.SOUL_SAND:net.minecraft.world.level.block.Blocks.FARMLAND).defaultBlockState();
                if(soil.hasProperty(net.minecraft.world.level.block.state.properties.BlockStateProperties.MOISTURE))soil=soil.setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.MOISTURE,moisture);
                for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++)level.setBlock(pos.below().offset(x,0,z),soil,816);
                level.setBlock(pos,state,816);
                if((pattern&1)!=0)level.setBlock(pos.west(),block.defaultBlockState(),816);
                if((pattern&2)!=0)level.setBlock(pos.north(),block.defaultBlockState(),816);
                if(pattern==4)level.setBlock(pos.west().north(),block.defaultBlockState(),816);
                int light=level.getRawBrightness(pos,0);
                level.getRandom().setSeed(seed);
                state.randomTick(level,pos,level.getRandom());
                lines.add("random-crop\t"+net.minecraft.world.level.block.Block.getId(state)+"\t"+moisture+"\t"+pattern+"\t"+seed+"\t"+light+"\t"+net.minecraft.world.level.block.Block.getId(level.getBlockState(pos))+"\t"+level.getRandom().nextLong());
            }
            Files.write(output,lines);
        }catch(Throwable error){error.printStackTrace();try{Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString());}catch(Exception ignored){}}
        finally{level.getServer().halt(false);}
    }
}
