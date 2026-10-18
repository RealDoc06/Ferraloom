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
        var components=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),world.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(components).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
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
            Blocks.DEAD_TUBE_CORAL_FAN,Blocks.DEAD_TUBE_CORAL_WALL_FAN,Blocks.PALE_MOSS_CARPET,Blocks.GLOW_LICHEN,Blocks.VINE,Blocks.PALE_HANGING_MOSS,Blocks.MOSS_CARPET,Blocks.OPEN_EYEBLOSSOM,Blocks.CLOSED_EYEBLOSSOM);
        List<Block> supports=List.of(Blocks.AIR,Blocks.STONE,Blocks.DIRT,Blocks.GRASS_BLOCK,Blocks.COARSE_DIRT,Blocks.ROOTED_DIRT,
            Blocks.PODZOL,Blocks.MYCELIUM,Blocks.FARMLAND,Blocks.SAND,Blocks.RED_SAND,Blocks.SOUL_SAND,Blocks.NETHERRACK,
            Blocks.CRIMSON_NYLIUM,Blocks.WARPED_NYLIUM,Blocks.MOSS_BLOCK,Blocks.PALE_MOSS_BLOCK,Blocks.CLAY,Blocks.MUD,
            Blocks.CACTUS,Blocks.SUGAR_CANE,Blocks.BAMBOO,Blocks.BAMBOO_SAPLING,Blocks.BIG_DRIPLEAF,Blocks.BIG_DRIPLEAF_STEM,
            Blocks.MANGROVE_LEAVES,Blocks.OAK_LEAVES,Blocks.WATER,Blocks.ICE,Blocks.PACKED_ICE,Blocks.BLUE_ICE,
            Blocks.SNOW,Blocks.SNOW_BLOCK,Blocks.POWDER_SNOW,Blocks.GLASS,Blocks.OAK_FENCE,Blocks.STONE_SLAB,
            Blocks.HONEY_BLOCK,Blocks.BARRIER,Blocks.PALE_MOSS_CARPET);
        var pos=new BlockPos(8,5,8);
        var level=new PlantLevel(registries);
        var treeClass=net.minecraft.world.level.block.grower.TreeGrower.class;
        var selectTree=treeClass.getDeclaredMethod("getConfiguredFeature",RandomSource.class,boolean.class);selectTree.setAccessible(true);
        var selectMega=treeClass.getDeclaredMethod("getConfiguredMegaFeature",RandomSource.class);selectMega.setAccessible(true);
        for(var entry:List.of(Map.entry(Blocks.OAK_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.OAK),Map.entry(Blocks.SPRUCE_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.SPRUCE),
            Map.entry(Blocks.MANGROVE_PROPAGULE,net.minecraft.world.level.block.grower.TreeGrower.MANGROVE),Map.entry(Blocks.AZALEA,net.minecraft.world.level.block.grower.TreeGrower.AZALEA),
            Map.entry(Blocks.BIRCH_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.BIRCH),Map.entry(Blocks.JUNGLE_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.JUNGLE),
            Map.entry(Blocks.ACACIA_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.ACACIA),Map.entry(Blocks.CHERRY_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.CHERRY),
            Map.entry(Blocks.DARK_OAK_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.DARK_OAK),Map.entry(Blocks.PALE_OAK_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.PALE_OAK),
            Map.entry(Blocks.POPLAR_SAPLING,net.minecraft.world.level.block.grower.TreeGrower.POPLAR))) {
            for(boolean flowers:new boolean[]{false,true})for(boolean megaFirst:new boolean[]{false,true})for(int n=0;n<64;n++) {
                long seed=n*0x9e3779b97f4a7c15L;var random=RandomSource.create(seed);
                net.minecraft.resources.ResourceKey<?> tree,mega;
                if(megaFirst){mega=(net.minecraft.resources.ResourceKey<?>)selectMega.invoke(entry.getValue(),random);tree=(net.minecraft.resources.ResourceKey<?>)selectTree.invoke(entry.getValue(),random,flowers);}
                else{tree=(net.minecraft.resources.ResourceKey<?>)selectTree.invoke(entry.getValue(),random,flowers);mega=(net.minecraft.resources.ResourceKey<?>)selectMega.invoke(entry.getValue(),random);}
                System.out.println("tree-select\t"+BuiltInRegistries.BLOCK.getId(entry.getKey())+"\t"+seed+"\t"+flowers+"\t"+megaFirst+"\t"+(tree==null?"-":tree.identifier())+"\t"+(mega==null?"-":mega.identifier())+"\t"+random.nextLong());
            }
        }
        for(var block:List.of(Blocks.WHEAT,Blocks.CARROTS,Blocks.POTATOES,Blocks.BEETROOTS,Blocks.TORCHFLOWER_CROP,
            Blocks.MELON_STEM,Blocks.ATTACHED_MELON_STEM,Blocks.PUMPKIN_STEM,Blocks.ATTACHED_PUMPKIN_STEM,
            Blocks.SWEET_BERRY_BUSH,Blocks.NETHER_WART,Blocks.BAMBOO_SAPLING,Blocks.CAVE_VINES,Blocks.CAVE_VINES_PLANT,
            Blocks.BIG_DRIPLEAF_STEM,Blocks.TALL_SEAGRASS,Blocks.KELP_PLANT,Blocks.TWISTING_VINES_PLANT,Blocks.WEEPING_VINES_PLANT,Blocks.FROSTED_ICE)) {
            for(var state:block.getStateDefinition().getPossibleStates())for(boolean includeData:new boolean[]{false,true}) {
                var stack=state.getCloneItemStack(level,pos,includeData);
                System.out.println("clone\t"+Block.getId(state)+"\t"+BuiltInRegistries.ITEM.getId(stack.getItem())+"\t"+stack.getCount()+"\t"+includeData);
            }
        }
        for(var state:Blocks.FROSTED_ICE.getStateDefinition().getPossibleStates())for(int seed=0;seed<64;seed++) {
            level.trace.clear();level.getRandom().setSeed(seed);
            ((FrostedIceBlock)Blocks.FROSTED_ICE).onPlace(state,level,pos,Blocks.AIR.defaultBlockState(),false);
            System.out.println("frosted-place\t"+Block.getId(state)+"\t"+seed+"\t"+level.trace.getFirst().substring("schedule:".length())+"\t"+level.getRandom().nextLong());
        }
        for(var block:List.of(Blocks.NETHERRACK,Blocks.ROOTED_DIRT,Blocks.WHEAT,Blocks.SEAGRASS,Blocks.GLOW_LICHEN,
            Blocks.PALE_MOSS_CARPET,Blocks.BAMBOO,Blocks.CAVE_VINES,Blocks.BIG_DRIPLEAF,Blocks.SEA_PICKLE,Blocks.MANGROVE_LEAVES)) {
            var bonemeal=(BonemealableBlock)block;
            System.out.println("bonemeal-kind\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+String.valueOf((Object)bonemeal.getType())+"\t"+(bonemeal.getParticlePos(pos).getY()-pos.getY()));
        }
        var canBeNylium=NyliumBlock.class.getDeclaredMethod("canBeNylium",BlockState.class,LevelReader.class,BlockPos.class);
        canBeNylium.setAccessible(true);
        for(var block:List.of(Blocks.CRIMSON_NYLIUM,Blocks.WARPED_NYLIUM))for(var support:supports)for(var above:support.getStateDefinition().getPossibleStates()) {
            level.chunks.clear();level.getChunkAt(pos).setBlockState(pos,block.defaultBlockState(),816);
            level.getChunkAt(pos).setBlockState(pos.above(),above,816);
            System.out.println("nylium-alive\t"+Block.getId(block.defaultBlockState())+"\t"+Block.getId(above)+"\t"+canBeNylium.invoke(null,block.defaultBlockState(),level,pos));
        }
        var clickedVines=new ArrayList<BlockState>(Blocks.VINE.getStateDefinition().getPossibleStates());clickedVines.add(Blocks.AIR.defaultBlockState());
        for(var clicked:clickedVines)for(var direction:Direction.values())for(int mode=0;mode<4;mode++) {
            level.chunks.clear();level.getChunkAt(pos).setBlockState(pos,clicked,816);
            if(mode==1)level.getChunkAt(pos.north()).setBlockState(pos.north(),Blocks.STONE.defaultBlockState(),816);
            if(mode==2)level.getChunkAt(pos.above()).setBlockState(pos.above(),Blocks.STONE.defaultBlockState(),816);
            if(mode==3)level.getChunkAt(pos.above()).setBlockState(pos.above(),Blocks.VINE.defaultBlockState().setValue(BlockStateProperties.NORTH,true),816);
            var context=new net.minecraft.world.item.context.DirectionalPlaceContext(level,pos,direction,net.minecraft.world.item.ItemStack.EMPTY,Direction.UP);
            var placed=Blocks.VINE.getStateForPlacement(context);
            System.out.println("vine-place\t"+Block.getId(clicked)+"\t"+direction.ordinal()+"\t"+mode+"\t"+(placed==null?-1:Block.getId(placed)));
        }
        for(var block:List.of(Blocks.OAK_LEAVES,Blocks.MANGROVE_LEAVES,Blocks.MANGROVE_PROPAGULE,Blocks.TALL_GRASS,Blocks.TALL_SEAGRASS,
            Blocks.PITCHER_CROP,Blocks.BAMBOO,Blocks.KELP,Blocks.KELP_PLANT,Blocks.CAVE_VINES,Blocks.TWISTING_VINES,Blocks.WEEPING_VINES,
            Blocks.SEAGRASS,Blocks.SEA_PICKLE,Blocks.TUBE_CORAL,Blocks.TUBE_CORAL_FAN,Blocks.TUBE_CORAL_WALL_FAN,Blocks.DEAD_TUBE_CORAL_WALL_FAN,
            Blocks.PALE_MOSS_CARPET,Blocks.PINK_PETALS,Blocks.LEAF_LITTER,Blocks.SNOW,Blocks.PODZOL,Blocks.GRASS_BLOCK,Blocks.MYCELIUM,
            Blocks.FARMLAND,Blocks.BIG_DRIPLEAF,Blocks.SMALL_DRIPLEAF,Blocks.GLOW_LICHEN)) {
            for(var below:List.of(Blocks.AIR,Blocks.DIRT,Blocks.STONE,Blocks.FARMLAND,Blocks.CLAY,Blocks.BAMBOO,Blocks.BAMBOO_SAPLING,Blocks.BIG_DRIPLEAF,Blocks.BIG_DRIPLEAF_STEM,Blocks.OAK_LOG))
            for(var clicked:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,8),Blocks.LAVA.defaultBlockState(),block.defaultBlockState()))
            for(var above:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.STONE.defaultBlockState(),block.defaultBlockState()))
            for(var direction:Direction.values())for(long seed:new long[]{0,1234}) {
                level.chunks.clear();level.getChunkAt(pos).setBlockState(pos,clicked,816);
                level.getChunkAt(pos).setBlockState(pos.below(),below.defaultBlockState(),816);
                level.getChunkAt(pos).setBlockState(pos.above(),above,816);
                // Walls for lateral attachment; retaining an empty east side
                // distinguishes looking order from a hard-coded facing.
                level.getChunkAt(pos).setBlockState(pos.north(),Blocks.STONE.defaultBlockState(),816);
                level.getChunkAt(pos).setBlockState(pos.south(),Blocks.STONE.defaultBlockState(),816);
                level.getChunkAt(pos).setBlockState(pos.west(),Blocks.STONE.defaultBlockState(),816);
                level.getRandom().setSeed(seed);
                var context=new net.minecraft.world.item.context.DirectionalPlaceContext(level,pos,direction,net.minecraft.world.item.ItemStack.EMPTY,Direction.UP);
                var result=block.getStateForPlacement(context);
                System.out.println("plant-place\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+seed+"\t"+Block.getId(below.defaultBlockState())+"\t"+Block.getId(clicked)+"\t"+Block.getId(above)+"\t"+direction.ordinal()+"\t"+(result==null?-1:Block.getId(result))+"\t"+level.getRandom().nextLong());
            }
        }
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
        var growBamboo=BambooStalkBlock.class.getDeclaredMethod("growBamboo",BlockState.class,Level.class,BlockPos.class,RandomSource.class,int.class);
        growBamboo.setAccessible(true);
        for(int age=0;age<=1;age++)for(var leaves:net.minecraft.world.level.block.state.properties.BambooLeaves.values())for(boolean twoBelow:new boolean[]{false,true})for(int height:new int[]{1,10,11,15})for(int seed=0;seed<8;seed++){
            level.chunks.clear();level.trace.clear();level.getRandom().setSeed(seed);
            var state=Blocks.BAMBOO.defaultBlockState().setValue(BlockStateProperties.AGE_1,age);
            var below=Blocks.BAMBOO.defaultBlockState().setValue(BlockStateProperties.BAMBOO_LEAVES,leaves);
            var two=twoBelow?Blocks.BAMBOO.defaultBlockState():Blocks.AIR.defaultBlockState();
            level.getChunkAt(pos).setBlockState(pos,state,816);level.getChunkAt(pos).setBlockState(pos.below(),below,816);level.getChunkAt(pos).setBlockState(pos.below(2),two,816);
            growBamboo.invoke(Blocks.BAMBOO,state,level,pos,level.getRandom(),height);
            var output=new ArrayList<String>();for(int dy=-2;dy<=1;dy++)output.add(""+Block.getId(level.getBlockState(pos.above(dy))));
            System.out.println("bamboo-grow\t"+Block.getId(state)+"\t"+Block.getId(below)+"\t"+Block.getId(two)+"\t"+height+"\t"+seed+"\t"+String.join(",",output)+"\t"+level.getRandom().nextLong());
        }
    }
}
