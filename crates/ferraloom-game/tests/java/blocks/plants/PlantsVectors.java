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
        @Override public void levelEvent(net.minecraft.world.entity.Entity source,int type,BlockPos pos,int data) { trace.add("event:"+type+":"+pos.getX()+":"+pos.getY()+":"+pos.getZ()+":"+data); }
        @Override public void gameEvent(Holder event,net.minecraft.world.phys.Vec3 at,net.minecraft.world.level.gameevent.GameEvent.Context context) { trace.add("game:"+BuiltInRegistries.GAME_EVENT.getKey((net.minecraft.world.level.gameevent.GameEvent)event.value())); }
        @Override public int getRawBrightness(BlockPos p,int darkening) { return Math.max(0,light-darkening); }
        @Override public int getBrightness(LightLayer layer,BlockPos p) { return layer==LightLayer.SKY?light:0; }
    }
    static final class PlantPlayer extends net.minecraft.world.entity.player.Player {
        PlantPlayer(Level level) {super(level,new com.mojang.authlib.GameProfile(new java.util.UUID(0,1),"plants"));getAbilities().instabuild=true;}
        @Override public GameType gameMode() {return GameType.CREATIVE;}
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
        // LevelReader's non-ServerLevel branch also applies to WorldGenRegion.
        for(var block:List.of(Blocks.OAK_SAPLING,Blocks.SPRUCE_SAPLING,Blocks.BIRCH_SAPLING,Blocks.JUNGLE_SAPLING,
            Blocks.ACACIA_SAPLING,Blocks.CHERRY_SAPLING,Blocks.DARK_OAK_SAPLING,Blocks.PALE_OAK_SAPLING,
            Blocks.POPLAR_SAPLING,Blocks.MANGROVE_PROPAGULE,Blocks.AZALEA,Blocks.FLOWERING_AZALEA,
            Blocks.BROWN_MUSHROOM,Blocks.RED_MUSHROOM))for(var state:block.getStateDefinition().getPossibleStates()) {
            level.getRandom().setSeed(42);
            boolean valid=((BonemealableBlock)block).isValidBonemealTarget(level,pos,state,BonemealSource.INTERACTION);
            System.out.println("non-server-target\t"+Block.getId(state)+"\t"+valid+"\t"+level.getRandom().nextLong());
        }
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
        var canStayAlive=SpreadingSnowyBlock.class.getDeclaredMethod("canStayAlive",BlockState.class,LevelReader.class,BlockPos.class);
        var canPropagate=SpreadingSnowyBlock.class.getDeclaredMethod("canPropagate",BlockState.class,LevelReader.class,BlockPos.class);
        canStayAlive.setAccessible(true);canPropagate.setAccessible(true);
        for(var block:List.of(Blocks.GRASS_BLOCK,Blocks.MYCELIUM))for(var state:block.getStateDefinition().getPossibleStates())for(var support:supports)for(var above:support.getStateDefinition().getPossibleStates()) {
            level.chunks.clear();level.getChunkAt(pos).setBlockState(pos,state,816);
            level.getChunkAt(pos).setBlockState(pos.above(),above,816);
            System.out.println("grass-support\t"+Block.getId(state)+"\t"+Block.getId(above)+"\t"+canStayAlive.invoke(null,state,level,pos)+"\t"+canPropagate.invoke(null,state,level,pos));
        }
        for(var block:List.of(Blocks.OPEN_EYEBLOSSOM,Blocks.CLOSED_EYEBLOSSOM))for(var origin:List.of(new BlockPos(8,5,8),new BlockPos(15,15,15),new BlockPos(16,16,16))) {
            level.chunks.clear();var state=block.defaultBlockState();
            for(int dx:new int[]{-3,0,3})for(int dy:new int[]{-2,0,2})for(int dz:new int[]{-3,0,3}) {
                var at=origin.offset(dx,dy,dz);level.getChunkAt(at).setBlockState(at,state,816);
            }
            var order=new ArrayList<String>();
            level.findBlocksIn(origin.offset(-3,-2,-3),origin.offset(3,2,3)).filterState(s->s==state)
                .forEach((at,s)->order.add((at.getX()-origin.getX())+":"+(at.getY()-origin.getY())+":"+(at.getZ()-origin.getZ())));
            System.out.println("eyeblossom-scan\t"+Block.getId(state)+"\t"+origin.getX()+"\t"+origin.getY()+"\t"+origin.getZ()+"\t"+String.join(",",order));
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
        for(var block:List.of(Blocks.HANGING_ROOTS,Blocks.TUBE_CORAL_BLOCK,Blocks.BRAIN_CORAL_BLOCK,Blocks.BUBBLE_CORAL_BLOCK,Blocks.FIRE_CORAL_BLOCK,Blocks.HORN_CORAL_BLOCK))
            for(int neighbor=0;neighbor<=6;neighbor++)for(var fluid:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,1),Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,8),Blocks.LAVA.defaultBlockState()))for(int seed=0;seed<8;seed++) {
            level.chunks.clear();level.getChunkAt(pos).setBlockState(pos,fluid,816);
            if(neighbor<6)level.getChunkAt(pos).setBlockState(pos.relative(Direction.values()[neighbor]),fluid,816);
            level.trace.clear();level.getRandom().setSeed(seed);
            var context=new net.minecraft.world.item.context.DirectionalPlaceContext(level,pos,Direction.NORTH,net.minecraft.world.item.ItemStack.EMPTY,Direction.UP);
            var result=block.getStateForPlacement(context);
            String scheduled=level.trace.stream().filter(t->t.startsWith("schedule:")).map(t->t.substring("schedule:".length())).collect(java.util.stream.Collectors.joining(","));
            System.out.println("special-place\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+seed+"\t"+neighbor+"\t"+Block.getId(fluid)+"\t"+(result==null?-1:Block.getId(result))+"\t"+level.getRandom().nextLong()+"\t"+scheduled);
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
        for(var block:List.of(Blocks.TALL_GRASS,Blocks.LARGE_FERN,Blocks.SUNFLOWER,Blocks.PITCHER_CROP,Blocks.SMALL_DRIPLEAF,Blocks.TALL_SEAGRASS,Blocks.PALE_MOSS_CARPET))
            for(var state:block.getStateDefinition().getPossibleStates())for(var above:List.of(Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,1),Blocks.STONE.defaultBlockState()))
            for(int mode=0;mode<3;mode++)for(int seed=0;seed<4;seed++) {
            if(state.hasProperty(BlockStateProperties.DOUBLE_BLOCK_HALF)&&state.getValue(BlockStateProperties.DOUBLE_BLOCK_HALF)==net.minecraft.world.level.block.state.properties.DoubleBlockHalf.UPPER)continue;
            if(block==Blocks.PALE_MOSS_CARPET&&!state.getValue(MossyCarpetBlock.BASE))continue;
            var support=(block==Blocks.SMALL_DRIPLEAF?Blocks.CLAY:block==Blocks.PITCHER_CROP?Blocks.FARMLAND:block==Blocks.TALL_SEAGRASS||block==Blocks.PALE_MOSS_CARPET?Blocks.STONE:Blocks.DIRT).defaultBlockState();
            level.chunks.clear();level.getChunkAt(pos.below()).setBlockState(pos.below(),support,816);
            level.getChunkAt(pos).setBlockState(pos,state,816);level.getChunkAt(pos.above()).setBlockState(pos.above(),above,816);
            if(mode>=1)for(var direction:Direction.Plane.HORIZONTAL) {
                if(mode==1&&direction!=Direction.NORTH)continue;
                for(int y=0;y<=1;y++)level.getChunkAt(pos.above(y).relative(direction)).setBlockState(pos.above(y).relative(direction),Blocks.STONE.defaultBlockState(),816);
            }
            level.getRandom().setSeed(seed);block.setPlacedBy(level,pos,state,null,net.minecraft.world.item.ItemStack.EMPTY);
            System.out.println("post-place\t"+Block.getId(state)+"\t"+Block.getId(support)+"\t"+Block.getId(above)+"\t"+mode+"\t"+seed+"\t"+Block.getId(level.getBlockState(pos))+"\t"+Block.getId(level.getBlockState(pos.above()))+"\t"+level.getRandom().nextLong());
        }
        var creativePlayer=new PlantPlayer(level);
        for(var block:List.of(Blocks.TALL_GRASS,Blocks.SUNFLOWER,Blocks.PITCHER_CROP,Blocks.SMALL_DRIPLEAF,Blocks.TALL_SEAGRASS))for(var state:block.getStateDefinition().getPossibleStates())for(int mode=0;mode<4;mode++)for(int seed=0;seed<4;seed++) {
            var below=switch(mode) {case 0->state.setValue(BlockStateProperties.DOUBLE_BLOCK_HALF,net.minecraft.world.level.block.state.properties.DoubleBlockHalf.LOWER);case 1->state.setValue(BlockStateProperties.DOUBLE_BLOCK_HALF,net.minecraft.world.level.block.state.properties.DoubleBlockHalf.UPPER);case 2->Blocks.DIRT.defaultBlockState();default->Blocks.LARGE_FERN.defaultBlockState();};
            level.chunks.clear();level.getChunkAt(pos.below(2)).setBlockState(pos.below(2),Blocks.CLAY.defaultBlockState(),816);
            level.getChunkAt(pos.below()).setBlockState(pos.below(),below,816);level.getChunkAt(pos).setBlockState(pos,state,816);
            level.trace.clear();level.getRandom().setSeed(seed);var result=block.playerWillDestroy(level,pos,state,creativePlayer);
            String events=level.trace.stream().filter(t->t.startsWith("event:")||t.startsWith("game:")).collect(java.util.stream.Collectors.joining(","));
            System.out.println("creative-destroy\t"+Block.getId(state)+"\t"+Block.getId(below)+"\t"+seed+"\t"+Block.getId(result)+"\t"+Block.getId(level.getBlockState(pos.below()))+"\t"+Block.getId(level.getBlockState(pos))+"\t"+level.getRandom().nextLong()+"\t"+events);
        }
        var leafNeighbors=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.OAK_LOG.defaultBlockState(),Blocks.MANGROVE_ROOTS.defaultBlockState(),
            Blocks.OAK_LEAVES.defaultBlockState().setValue(BlockStateProperties.DISTANCE,1),Blocks.OAK_LEAVES.defaultBlockState().setValue(BlockStateProperties.DISTANCE,6),
            Blocks.OAK_LEAVES.defaultBlockState(),Blocks.OAK_LEAVES.defaultBlockState().setValue(BlockStateProperties.PERSISTENT,true));
        for(var block:List.of(Blocks.OAK_LEAVES,Blocks.SPRUCE_LEAVES,Blocks.CHERRY_LEAVES,Blocks.MANGROVE_LEAVES))for(var state:block.getStateDefinition().getPossibleStates())
            for(var neighbor:leafNeighbors)for(var direction:Direction.values()) {
            level.chunks.clear();level.getChunkAt(pos).setBlockState(pos,state,816);
            var at=pos.relative(direction);level.getChunkAt(at).setBlockState(at,neighbor,816);
            level.trace.clear();var random=RandomSource.create(1234);
            var result=state.updateShape(level,level,pos,direction,at,neighbor,random);
            System.out.println("leaf-update\t"+Block.getId(state)+"\t"+Block.getId(neighbor)+"\t"+direction.ordinal()+"\t"+Block.getId(result)+"\t"+random.nextLong()+"\t"+String.join(",",level.trace));
        }
        for(var block:List.of(Blocks.CHORUS_PLANT,Blocks.CHORUS_FLOWER))for(var state:block.getStateDefinition().getPossibleStates())
            for(var below:List.of(Blocks.AIR,Blocks.STONE,Blocks.END_STONE,Blocks.CHORUS_PLANT))for(boolean above:new boolean[]{false,true})for(int mode=0;mode<6;mode++) {
            level.chunks.clear();level.trace.clear();
            level.getChunkAt(pos).setBlockState(pos,state,816);
            level.getChunkAt(pos.below()).setBlockState(pos.below(),below.defaultBlockState(),816);
            if(above)level.getChunkAt(pos.above()).setBlockState(pos.above(),Blocks.STONE.defaultBlockState(),816);
            if(mode>=1&&mode<=3) {
                level.getChunkAt(pos.north()).setBlockState(pos.north(),Blocks.CHORUS_PLANT.defaultBlockState(),816);
                if(mode==1)level.getChunkAt(pos.north().below()).setBlockState(pos.north().below(),Blocks.END_STONE.defaultBlockState(),816);
                if(mode==3)level.getChunkAt(pos.south()).setBlockState(pos.south(),Blocks.CHORUS_PLANT.defaultBlockState(),816);
            }
            if(mode==4)level.getChunkAt(pos.east()).setBlockState(pos.east(),Blocks.STONE.defaultBlockState(),816);
            if(mode==5)level.getChunkAt(pos.west()).setBlockState(pos.west(),Blocks.CHORUS_FLOWER.defaultBlockState(),816);
            boolean survives=state.canSurvive(level,pos);
            for(var direction:Direction.values()) {
                level.trace.clear();var random=RandomSource.create(1234);
                var at=pos.relative(direction);var neighbor=level.getBlockState(at);
                var result=state.updateShape(level,level,pos,direction,at,neighbor,random);
                System.out.println("chorus-support\t"+Block.getId(state)+"\t"+Block.getId(below.defaultBlockState())+"\t"+above+"\t"+mode+"\t"+survives+"\t"+direction.ordinal()+"\t"+Block.getId(result)+"\t"+random.nextLong()+"\t"+String.join(",",level.trace));
            }
        }
        var configField=MultifaceSpreader.class.getDeclaredField("config");configField.setAccessible(true);
        var vein=(SculkVeinBlock)Blocks.SCULK_VEIN;
        var lichen=(GlowLichenBlock)Blocks.GLOW_LICHEN;
        var allConfigs=List.of(lichen.getSpreader(),vein.getSpreader(),vein.getSameSpaceSpreader());
        var oldStates=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.SHORT_GRASS.defaultBlockState(),
            Blocks.FIRE.defaultBlockState(),Blocks.SOUL_FIRE.defaultBlockState(),Blocks.WATER.defaultBlockState(),
            Blocks.WATER.defaultBlockState().setValue(BlockStateProperties.LEVEL,1),Blocks.LAVA.defaultBlockState(),
            Blocks.SCULK_VEIN.defaultBlockState(),Blocks.GLOW_LICHEN.defaultBlockState());
        for(int kind=0;kind<allConfigs.size();kind++) {
            var spreader=allConfigs.get(kind);var config=(MultifaceSpreader.SpreadConfig)configField.get(spreader);
            var replace=config.getClass().getDeclaredMethod("stateCanBeReplaced",BlockGetter.class,BlockPos.class,BlockPos.class,Direction.class,BlockState.class);replace.setAccessible(true);
            for(var old:oldStates)for(var against:List.of(Blocks.STONE,Blocks.SCULK,Blocks.SCULK_CATALYST,Blocks.MOVING_PISTON,Blocks.GLASS,Blocks.OAK_LEAVES,Blocks.SOUL_SAND,Blocks.STONE_SLAB))
                for(int distance=0;distance<3;distance++)for(var face:Direction.values())for(boolean blocked:new boolean[]{false,true}) {
                level.chunks.clear();level.trace.clear();
                var target=distance==0?pos:distance==1?pos.east():pos.east().above();
                var cells=new LinkedHashMap<BlockPos,BlockState>();
                cells.put(target,old);cells.put(target.relative(face),against.defaultBlockState());
                if(blocked)cells.put(pos.relative(face.getOpposite()),Blocks.STONE.defaultBlockState());
                for(var cell:cells.entrySet())level.getChunkAt(cell.getKey()).setBlockState(cell.getKey(),cell.getValue(),816);
                var existing=level.getBlockState(target);
                boolean replaceable=(boolean)replace.invoke(config,level,pos,target,face,existing);
                boolean canSpread=config.canSpreadInto(level,pos,new MultifaceSpreader.SpreadPos(target,face));
                var encoded=new ArrayList<String>();for(var cell:cells.entrySet())encoded.add((cell.getKey().getX()-pos.getX())+":"+(cell.getKey().getY()-pos.getY())+":"+(cell.getKey().getZ()-pos.getZ())+":"+Block.getId(cell.getValue()));
                System.out.println("spreader-config\t"+kind+"\t"+(target.getX()-pos.getX())+"\t"+(target.getY()-pos.getY())+"\t"+face.ordinal()+"\t"+Block.getId(existing)+"\t"+replaceable+"\t"+canSpread+"\t"+String.join(",",encoded));
            }
            var sourceStates=List.of(Blocks.STONE.defaultBlockState(),Blocks.AIR.defaultBlockState(),Blocks.WATER.defaultBlockState(),
                Blocks.GLOW_LICHEN.defaultBlockState(),Blocks.GLOW_LICHEN.defaultBlockState().setValue(BlockStateProperties.NORTH,true),
                Blocks.SCULK_VEIN.defaultBlockState(),Blocks.SCULK_VEIN.defaultBlockState().setValue(BlockStateProperties.NORTH,true),
                Blocks.SCULK_VEIN.defaultBlockState().setValue(BlockStateProperties.NORTH,true).setValue(BlockStateProperties.SOUTH,true));
            for(var input:sourceStates)for(int mode=0;mode<5;mode++)for(var from:Direction.values())for(var toward:Direction.values()) {
                level.chunks.clear();level.trace.clear();
                var cells=new LinkedHashMap<BlockPos,BlockState>();
                if(mode<3)for(int x=-2;x<=2;x++)for(int z=-2;z<=2;z++)cells.put(pos.offset(x,-1,z),(mode==1?Blocks.SCULK:Blocks.STONE).defaultBlockState());
                if(mode==2)cells.put(pos.east().below(),Blocks.SCULK_CATALYST.defaultBlockState());
                if(mode==3)for(int x=-2;x<=2;x++)for(int z=-2;z<=2;z++)cells.put(pos.offset(x,1,z),Blocks.STONE.defaultBlockState());
                if(mode==4)for(var direction:Direction.Plane.HORIZONTAL)cells.put(pos.relative(direction,2),Blocks.STONE.defaultBlockState());
                cells.put(pos,input);
                for(var cell:cells.entrySet())level.getChunkAt(cell.getKey()).setBlockState(cell.getKey(),cell.getValue(),816);
                var result=spreader.getSpreadFromFaceTowardDirection(input,level,pos,from,toward,config::canSpreadInto);
                String next=result.map(spread->(spread.pos().getX()-pos.getX())+":"+(spread.pos().getY()-pos.getY())+":"+(spread.pos().getZ()-pos.getZ())+":"+spread.face().ordinal()).orElse("-");
                var encoded=new ArrayList<String>();for(var cell:cells.entrySet())encoded.add((cell.getKey().getX()-pos.getX())+":"+(cell.getKey().getY()-pos.getY())+":"+(cell.getKey().getZ()-pos.getZ())+":"+Block.getId(cell.getValue()));
                System.out.println("spreader-candidate\t"+kind+"\t"+Block.getId(input)+"\t"+mode+"\t"+from.ordinal()+"\t"+toward.ordinal()+"\t"+next+"\t"+String.join(",",encoded));
            }
            for(var input:List.of(Blocks.STONE.defaultBlockState(),Blocks.AIR.defaultBlockState(),Blocks.GLOW_LICHEN.defaultBlockState().setValue(BlockStateProperties.NORTH,true),Blocks.SCULK_VEIN.defaultBlockState(),Blocks.SCULK_VEIN.defaultBlockState().setValue(BlockStateProperties.NORTH,true)))for(var face:Direction.values()) {
                System.out.println("spreader-source\t"+kind+"\t"+Block.getId(input)+"\t"+face.ordinal()+"\t"+config.isOtherBlockValidAsSource(input)+"\t"+config.canSpreadFrom(input,face));
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
                System.out.println("shape\t"+Block.getId(state)+"\t"+at.getX()+"\t"+at.getY()+"\t"+at.getZ()+"\t"+collision+"\t"+String.join(";",boxes)+"\t"+state.isCollisionShapeFullBlock(level,at));
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
