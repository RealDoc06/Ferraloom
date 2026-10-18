import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

/** Calls real BlockState virtuals on a concrete in-memory Level. Omitted server
 * services throw in MutationFixture; no allocation bypass or mocked block method. */
public final class PlantsVectors {
    static final class PlantLevel extends MutationFixture {
        int light;
        PlantLevel(RegistryAccess registry) { super(registry); }
        @Override public int getRawBrightness(BlockPos p,int darkening) { return Math.max(0,light-darkening); }
        @Override public int getBrightness(LightLayer layer,BlockPos p) { return layer==LightLayer.SKY?light:0; }
    }
    public static void main(String[] args) throws Exception {
        var out=System.out;var err=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
        var world=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        List<Block> plants=List.of(Blocks.DANDELION,Blocks.WITHER_ROSE,Blocks.SHORT_GRASS,Blocks.FERN,Blocks.BUSH,
            Blocks.DEAD_BUSH,Blocks.SHORT_DRY_GRASS,Blocks.TALL_DRY_GRASS,Blocks.OAK_SAPLING,Blocks.AZALEA,
            Blocks.MANGROVE_PROPAGULE,Blocks.WHEAT,Blocks.CARROTS,Blocks.POTATOES,Blocks.BEETROOTS,Blocks.TORCHFLOWER_CROP,
            Blocks.PITCHER_CROP,Blocks.MELON_STEM,Blocks.PUMPKIN_STEM,Blocks.ATTACHED_MELON_STEM,Blocks.ATTACHED_PUMPKIN_STEM,
            Blocks.SWEET_BERRY_BUSH,Blocks.NETHER_WART,Blocks.BROWN_MUSHROOM,Blocks.RED_MUSHROOM,Blocks.CRIMSON_FUNGUS,
            Blocks.WARPED_FUNGUS,Blocks.CRIMSON_ROOTS,Blocks.WARPED_ROOTS,Blocks.NETHER_SPROUTS,Blocks.LILY_PAD,
            Blocks.CACTUS,Blocks.CACTUS_FLOWER,Blocks.SUGAR_CANE,Blocks.BAMBOO,Blocks.BAMBOO_SAPLING,Blocks.SEAGRASS,
            Blocks.TALL_SEAGRASS,Blocks.SEA_PICKLE,Blocks.TALL_GRASS,Blocks.LARGE_FERN,Blocks.SUNFLOWER,
            Blocks.HANGING_ROOTS,Blocks.SPORE_BLOSSOM,Blocks.BIG_DRIPLEAF,Blocks.BIG_DRIPLEAF_STEM,Blocks.SMALL_DRIPLEAF,
            Blocks.PINK_PETALS,Blocks.LEAF_LITTER,Blocks.FIREFLY_BUSH,Blocks.SNOW,Blocks.DEAD_TUBE_CORAL,
            Blocks.DEAD_TUBE_CORAL_FAN,Blocks.DEAD_TUBE_CORAL_WALL_FAN,Blocks.PALE_MOSS_CARPET,Blocks.GLOW_LICHEN,Blocks.VINE);
        List<Block> supports=List.of(Blocks.AIR,Blocks.STONE,Blocks.DIRT,Blocks.GRASS_BLOCK,Blocks.COARSE_DIRT,Blocks.ROOTED_DIRT,
            Blocks.PODZOL,Blocks.MYCELIUM,Blocks.FARMLAND,Blocks.SAND,Blocks.RED_SAND,Blocks.SOUL_SAND,Blocks.NETHERRACK,
            Blocks.CRIMSON_NYLIUM,Blocks.WARPED_NYLIUM,Blocks.MOSS_BLOCK,Blocks.PALE_MOSS_BLOCK,Blocks.CLAY,Blocks.MUD,
            Blocks.CACTUS,Blocks.SUGAR_CANE,Blocks.BAMBOO,Blocks.BAMBOO_SAPLING,Blocks.BIG_DRIPLEAF,Blocks.BIG_DRIPLEAF_STEM,
            Blocks.MANGROVE_LEAVES,Blocks.OAK_LEAVES,Blocks.WATER,Blocks.ICE,Blocks.PACKED_ICE,Blocks.BLUE_ICE,
            Blocks.SNOW,Blocks.SNOW_BLOCK,Blocks.POWDER_SNOW,Blocks.GLASS,Blocks.OAK_FENCE,Blocks.STONE_SLAB,
            Blocks.HONEY_BLOCK,Blocks.BARRIER,Blocks.PALE_MOSS_CARPET);
        var pos=new BlockPos(8,5,8);
        var level=new PlantLevel(registries);
        for(var block:plants) {
            var states=new ArrayList<BlockState>();states.add(block.defaultBlockState());
            if(block.defaultBlockState().hasProperty(BlockStateProperties.DOUBLE_BLOCK_HALF)) states.add(block.defaultBlockState().setValue(BlockStateProperties.DOUBLE_BLOCK_HALF,net.minecraft.world.level.block.state.properties.DoubleBlockHalf.UPPER));
            if(block==Blocks.MANGROVE_PROPAGULE)states.add(block.defaultBlockState().setValue(BlockStateProperties.HANGING,true));
            if(block==Blocks.PALE_MOSS_CARPET)states.add(block.defaultBlockState().setValue(BlockStateProperties.BOTTOM,false));
            if(block==Blocks.GLOW_LICHEN||block==Blocks.VINE) states.add(block.defaultBlockState().setValue(BlockStateProperties.NORTH,true));
            for(var state:states)for(var support:supports)for(int light:new int[]{0,7,8,15}) {
                level.chunks.clear();level.trace.clear();level.light=light;
                level.getChunkAt(pos).setBlockState(pos,state,816);
                level.getChunkAt(pos.below()).setBlockState(pos.below(),support.defaultBlockState(),816);
                boolean survived=state.canSurvive(level,pos);
                System.out.println("survive\t"+Block.getId(state)+"\t"+Block.getId(support.defaultBlockState())+"\t"+light+"\t"+survived);
                for(var direction:Direction.values()) {
                    var random=RandomSource.create(1234);
                    level.trace.clear();
                    var neighborPos=pos.relative(direction);
                    var neighbor=level.getBlockState(neighborPos);
                    var result=state.updateShape(level,level,pos,direction,neighborPos,neighbor,random);
                    System.out.println("update\t"+Block.getId(state)+"\t"+Block.getId(support.defaultBlockState())+"\t"+light+"\t"+direction.ordinal()+"\t"+Block.getId(result)+"\t"+random.nextLong()+"\t"+String.join(",",level.trace));
                }
            }
        }
        var shapeBlocks=new ArrayList<Block>(plants);shapeBlocks.add(Blocks.POWDER_SNOW);shapeBlocks.add(Blocks.OPEN_EYEBLOSSOM);
        for(var block:shapeBlocks)for(var state:block.getStateDefinition().getPossibleStates())for(var at:List.of(BlockPos.ZERO,pos,new BlockPos(-7,11,17))){
            for(boolean collision:new boolean[]{false,true}){
                var shape=collision?state.getCollisionShape(level,at,net.minecraft.world.phys.shapes.CollisionContext.empty()):state.getShape(level,at);
                var boxes=new ArrayList<String>();
                for(var box:shape.toAabbs()){
                    var coordinates=new ArrayList<String>();for(double d:new double[]{box.minX,box.minY,box.minZ,box.maxX,box.maxY,box.maxZ})coordinates.add(Long.toUnsignedString(Double.doubleToRawLongBits(d),16));
                    boxes.add(String.join(":",coordinates));
                }
                System.out.println("shape\t"+Block.getId(state)+"\t"+at.getX()+"\t"+at.getY()+"\t"+at.getZ()+"\t"+collision+"\t"+String.join(";",boxes));
            }
        }
        var growthSpeed=CropBlock.class.getDeclaredMethod("getGrowthSpeed",Block.class,BlockGetter.class,BlockPos.class);
        growthSpeed.setAccessible(true);
        for(var block:List.of(Blocks.WHEAT,Blocks.BEETROOTS,Blocks.TORCHFLOWER_CROP)) {
            var crop=(CropBlock)block;
            int states=block.getStateDefinition().getPossibleStates().size();
            for(int age=0;age<states;age++)for(int seed=0;seed<32;seed++) {
                level.chunks.clear();level.trace.clear();level.light=15;level.getRandom().setSeed(seed);
                var state=crop.getStateForAge(age);
                level.getChunkAt(pos).setBlockState(pos,state,816);
                level.getChunkAt(pos.below()).setBlockState(pos.below(),Blocks.FARMLAND.defaultBlockState(),816);
                crop.growCrops(level,pos,state);
                System.out.println("grow\t"+Block.getId(state)+"\t"+seed+"\t"+Block.getId(level.getBlockState(pos))+"\t"+level.getRandom().nextLong());
            }
            for(int moisture:new int[]{0,1,7})for(int pattern=0;pattern<16;pattern++) {
                level.chunks.clear();level.trace.clear();
                var soil=Blocks.FARMLAND.defaultBlockState().setValue(BlockStateProperties.MOISTURE,moisture);
                for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++)level.getChunkAt(pos).setBlockState(pos.below().offset(x,0,z),soil,816);
                if((pattern&1)!=0)level.getChunkAt(pos).setBlockState(pos.west(),block.defaultBlockState(),816);
                if((pattern&2)!=0)level.getChunkAt(pos).setBlockState(pos.north(),block.defaultBlockState(),816);
                if((pattern&4)!=0)level.getChunkAt(pos).setBlockState(pos.west().north(),block.defaultBlockState(),816);
                if((pattern&8)!=0)level.getChunkAt(pos).setBlockState(pos.east().south(),block.defaultBlockState(),816);
                float speed=(Float)growthSpeed.invoke(null,block,level,pos);
                System.out.println("speed\t"+Block.getId(block.defaultBlockState())+"\t"+moisture+"\t"+pattern+"\t"+Float.floatToIntBits(speed));
            }
        }
    }
}
