import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.named;
import static net.minecraft.world.level.block.Blocks.*;
import static net.minecraft.world.level.block.state.properties.BlockStateProperties.*;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.BonemealableBlock;
import net.minecraft.world.level.block.BonemealSource;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;

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
            if (Boolean.getBoolean("ferraloom.plants.extended")) extended(level,pos,lines);
            Files.write(output,lines);
            Files.writeString(output.resolveSibling("oracle-done"),"done\n");
        }catch(Throwable error){error.printStackTrace();try{Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString());}catch(Exception ignored){}}
        finally{level.getServer().halt(false);}
    }

    static void clear(ServerLevel level,BlockPos pos) {
        for(int x=-3;x<=3;x++)for(int y=-20;y<=20;y++)for(int z=-3;z<=3;z++)level.setBlock(pos.offset(x,y,z),AIR.defaultBlockState(),816);
    }
    static String snapshot(ServerLevel level,BlockPos pos) {
        var cells=new ArrayList<String>();
        for(int z=-3;z<=3;z++)for(int y=-20;y<=20;y++)for(int x=-3;x<=3;x++) {
            var state=level.getBlockState(pos.offset(x,y,z));
            if(!state.isAir())cells.add(x+":"+y+":"+z+":"+Block.getId(state));
        }
        return String.join(",",cells);
    }
    static void extended(ServerLevel level,BlockPos pos,List<String> lines) {
        // Real bonemeal dispatch, including both halves/heads/bodies and blocked
        // growth. Snapshot every relevant cell before and after the callback.
        var blocks=List.of(PITCHER_CROP,SEA_PICKLE,MELON_STEM,PUMPKIN_STEM,KELP,KELP_PLANT,
            TWISTING_VINES,TWISTING_VINES_PLANT,WEEPING_VINES,WEEPING_VINES_PLANT,
            BIG_DRIPLEAF,BIG_DRIPLEAF_STEM,SMALL_DRIPLEAF,MANGROVE_LEAVES,NETHERRACK,ROOTED_DIRT);
        for(var block:blocks)for(var state:block.getStateDefinition().getPossibleStates())for(int mode=0;mode<3;mode++)for(int seed=0;seed<8;seed++) {
            clear(level,pos);
            for(int x=-3;x<=3;x++)for(int z=-3;z<=3;z++)level.setBlock(pos.below().offset(x,0,z),FARMLAND.defaultBlockState().setValue(MOISTURE,7),816);
            if(block==MANGROVE_LEAVES||block==ROOTED_DIRT) {
                level.setBlock(pos.below(),AIR.defaultBlockState(),816);
            } else if(block==NETHERRACK) {
                if(mode!=2)level.setBlock(pos.west(),WARPED_NYLIUM.defaultBlockState(),816);
                if(mode!=0)level.setBlock(pos.east(),CRIMSON_NYLIUM.defaultBlockState(),816);
            } else if(block==SEA_PICKLE) {
                for(int x=-2;x<=2;x++)for(int z=-2;z<=2;z++)for(int y=-2;y<=0;y++)level.setBlock(pos.offset(x,y,z),(y==-2?TUBE_CORAL_BLOCK:WATER).defaultBlockState(),816);
            } else if(block==KELP||block==KELP_PLANT) {
                level.setBlock(pos.below(),SAND.defaultBlockState(),816);
                for(int y=0;y<=12;y++)level.setBlock(pos.above(y),WATER.defaultBlockState(),816);
                if(block==KELP_PLANT)level.setBlock(pos.above(),KELP.defaultBlockState().setValue(AGE_25,24),816);
            } else if(block==WEEPING_VINES||block==WEEPING_VINES_PLANT) {
                level.setBlock(pos.below(),AIR.defaultBlockState(),816);level.setBlock(pos.above(),NETHERRACK.defaultBlockState(),816);
                if(block==WEEPING_VINES_PLANT)level.setBlock(pos.below(),WEEPING_VINES.defaultBlockState().setValue(AGE_25,24),816);
            } else if(block==TWISTING_VINES_PLANT) {
                level.setBlock(pos.above(),TWISTING_VINES.defaultBlockState().setValue(AGE_25,24),816);
            } else if(block==PITCHER_CROP && state.getValue(DOUBLE_BLOCK_HALF)==DoubleBlockHalf.UPPER) {
                level.setBlock(pos.below(2),FARMLAND.defaultBlockState(),816);
                level.setBlock(pos.below(),state.setValue(DOUBLE_BLOCK_HALF,DoubleBlockHalf.LOWER),816);
            } else if(block==SMALL_DRIPLEAF) {
                if(state.getValue(DOUBLE_BLOCK_HALF)==DoubleBlockHalf.LOWER)level.setBlock(pos.above(),state.setValue(DOUBLE_BLOCK_HALF,DoubleBlockHalf.UPPER),816);
                else {level.setBlock(pos.below(2),CLAY.defaultBlockState(),816);level.setBlock(pos.below(),state.setValue(DOUBLE_BLOCK_HALF,DoubleBlockHalf.LOWER),816);}
            } else if(block==BIG_DRIPLEAF_STEM) {
                level.setBlock(pos.above(),BIG_DRIPLEAF.defaultBlockState(),816);
            }
            level.setBlock(pos,state,816);
            if(mode==1)level.setBlock(pos.above(2),STONE.defaultBlockState(),816);
            if(mode==2)level.setBlock(pos.below(block==SMALL_DRIPLEAF&&state.getValue(DOUBLE_BLOCK_HALF)==DoubleBlockHalf.UPPER?2:1),STONE.defaultBlockState(),816);
            String before=snapshot(level,pos);
            var behavior=(BonemealableBlock)block;
            level.getRandom().setSeed(seed);
            boolean valid=behavior.isValidBonemealTarget(level,pos,state,BonemealSource.INTERACTION);
            boolean success=valid&&behavior.isBonemealSuccess(level,level.getRandom(),pos,state,BonemealSource.INTERACTION);
            if(success)behavior.performBonemeal(level,level.getRandom(),pos,state,BonemealSource.INTERACTION);
            lines.add("bonemeal\t"+Block.getId(state)+"\t"+seed+"\t"+valid+"\t"+success+"\t"+before+"\t"+snapshot(level,pos)+"\t"+level.getRandom().nextLong());
        }
    }
}
