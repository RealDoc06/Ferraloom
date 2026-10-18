import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.permissions.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.*;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntityTypes;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.phys.*;

/** Native furniture placement: six world cells, real signal/support queries and item components. */
public final class FurniturePlacementVectors {
    static final BlockPos POS = ContainerPlacementVectors.POS;
    static final class PlacementPlayer extends Player {
        PermissionSet permission = PermissionSet.NO_PERMISSIONS;
        PlacementPlayer(Level level) { super(level, new com.mojang.authlib.GameProfile(new UUID(1, 2), "metadata")); }
        @Override public GameType gameMode() { return GameType.SURVIVAL; }
        @Override public PermissionSet permissions() { return permission; }
    }
    static final class Context extends BlockPlaceContext {
        Context(Level level, Player player, ItemStack item, Direction face) {
            super(level, player, InteractionHand.MAIN_HAND, item, new BlockHitResult(Vec3.atCenterOf(POS), face, POS, false));
        }
    }
    static Block block(String name) { return BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(name)); }
    static boolean supported(Block block) {
        return block instanceof AbstractBannerBlock || block instanceof SignBlock || block instanceof AbstractSkullBlock
            || block instanceof ChiseledBookShelfBlock || block instanceof ShelfBlock || block instanceof DecoratedPotBlock
            || block instanceof LecternBlock || block instanceof BellBlock || block instanceof BeehiveBlock
            || block instanceof CampfireBlock || block instanceof CrafterBlock || block instanceof ConduitBlock
            || block instanceof CopperGolemStatueBlock || block instanceof CommandBlock || block instanceof JigsawBlock
            || block instanceof TestBlock || block instanceof VaultBlock;
    }
    static BlockState[] cells(BlockState state) { var result = new BlockState[6]; Arrays.fill(result, state); return result; }
    static ItemStack held(Block block, String component) {
        var held = new ItemStack(block.asItem());
        if (component.startsWith("mode:")) {
            held.set(DataComponents.BLOCK_STATE, new BlockItemStateProperties(Map.of("mode", component.substring(5))));
        } else if (component.startsWith("book:")) {
            var tag = new CompoundTag();
            switch (component) {
                case "book:compound", "book:wrong_type" -> tag.put("Book", new CompoundTag());
                case "book:int" -> tag.putInt("Book", 7);
                case "book:wrong_key" -> tag.put("book", new CompoundTag());
                case "book:absent" -> { }
                default -> throw new AssertionError(component);
            }
            held.set(DataComponents.BLOCK_ENTITY_DATA, TypedEntityData.of(component.equals("book:wrong_type") ? BlockEntityTypes.CHEST : BlockEntityTypes.LECTERN, tag));
        }
        return held;
    }
    static void capture(String group, Block block, float yaw, float pitch, boolean secondary, Direction face,
                        int directional, BlockState clicked, BlockState[] cells, int capability, String component,
                        ContainerPlacementVectors.PlacementLevel level, PlacementPlayer player) {
        level.blocks.clear(); level.blocks.put(POS, clicked);
        for (var side : Direction.values()) level.blocks.put(POS.relative(side), cells[side.get3DDataValue()]);
        player.setYRot(yaw); player.setXRot(pitch); player.setShiftKeyDown(secondary);
        player.getAbilities().instabuild = capability == 1 || capability == 2;
        player.permission = capability >= 2 ? LevelBasedPermissionSet.GAMEMASTER : LevelBasedPermissionSet.ALL;
        var held = held(block, component);
        BlockPlaceContext context = directional >= 0
            ? new DirectionalPlaceContext(level, POS, Direction.from3DDataValue(directional), held, face)
            : new Context(level, directional == -2 ? null : player, held, face);
        level.seed(263777);
        var result = block.getStateForPlacement(context);
        String neighbors = String.join(",", Arrays.stream(cells).map(s -> Integer.toString(Block.getId(s))).toList());
        System.out.println("furniture\t" + group + "\t" + BuiltInRegistries.BLOCK.getId(block)
            + "\t" + Integer.toUnsignedString(Float.floatToRawIntBits(player.getYRot()),16)
            + "\t" + Integer.toUnsignedString(Float.floatToRawIntBits(player.getXRot()),16)
            + "\t" + secondary + "\t" + face.get3DDataValue() + "\t" + directional
            + "\t" + Block.getId(clicked) + "\t" + neighbors + "\t" + capability + "\t" + component
            + "\t" + (result == null ? -1 : Block.getId(result)) + "\t" + level.nextRandom());
    }
    public static void main(String[] args) {
        var out = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
        var registries = NeighborVectors.load();
        var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(), registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(DataComponentInitializers.PendingComponents::apply);
        var level = new ContainerPlacementVectors.PlacementLevel(registries); var player = new PlacementPlayer(level);
        var blocks = BuiltInRegistries.BLOCK.stream().filter(FurniturePlacementVectors::supported).toList();
        var air = Blocks.AIR.defaultBlockState(); var stone = Blocks.STONE.defaultBlockState();
        var fluids = List.of(air, Blocks.WATER.defaultBlockState(), Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,3),
            Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,8), Blocks.LAVA.defaultBlockState());
        for (var block : blocks) {
            for (float yaw : new float[]{0,90,180,270}) for (float pitch : new float[]{-90,0,90})
                for (var face : Direction.values()) for (var clicked : fluids) for (boolean power : new boolean[]{false,true})
                    capture("orientation", block,yaw,pitch,false,face,-1,clicked,cells(power?Blocks.REDSTONE_BLOCK.defaultBlockState():stone),0,"none",level,player);
            for (var forward : Direction.values()) for (var face : Direction.values())
                capture("directional",block,0,0,false,face,forward.get3DDataValue(),air,cells(stone),0,"none",level,player);
            for (var face : Direction.values())
                capture("redirect",block,0,0,false,face,-1,stone,cells(stone),0,"none",level,player);
        }
        var walls = blocks.stream().filter(b -> b instanceof WallBannerBlock || b instanceof WallSignBlock || b instanceof WallHangingSignBlock || b instanceof WallSkullBlock).toList();
        for (var block : walls) for (int mask=0;mask<16;mask++) for (float yaw : new float[]{0,90,180,270})
            for (var face : Direction.values()) for (var clicked : fluids.subList(0,2)) {
                var neighbors=cells(air);
                for(int i=0;i<4;i++) if((mask&(1<<i))!=0) neighbors[ContainerPlacementVectors.SIDES[i].get3DDataValue()]=stone;
                capture("wall-mask",block,yaw,0,false,face,-1,clicked,neighbors,0,"none",level,player);
            }
        var palette = List.of(stone,Blocks.GLASS.defaultBlockState(),block("oak_leaves").defaultBlockState(),Blocks.SNOW.defaultBlockState(),
            Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS,8),Blocks.STONE_SLAB.defaultBlockState(),
            Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP),Blocks.CHEST.defaultBlockState(),
            Blocks.WATER.defaultBlockState(),Blocks.SHORT_GRASS.defaultBlockState(),Blocks.IRON_BARS.defaultBlockState(),block("oak_fence").defaultBlockState());
        for (var block : walls) for(var support:palette) for(float yaw:new float[]{0,90,180,270}) for(var face:Direction.values())
            capture("wall-support",block,yaw,0,false,face,-1,air,cells(support),0,"none",level,player);
        var ceilingSigns=blocks.stream().filter(b->b instanceof CeilingHangingSignBlock).toList();
        var tops=new ArrayList<BlockState>(palette);
        tops.addAll(block("oak_hanging_sign").getStateDefinition().getPossibleStates().stream().filter(s->!s.getValue(SignBlock.WATERLOGGED)&&!s.getValue(CeilingHangingSignBlock.ATTACHED)).toList());
        tops.addAll(block("oak_wall_hanging_sign").getStateDefinition().getPossibleStates().stream().filter(s->!s.getValue(SignBlock.WATERLOGGED)).toList());
        for(var block:ceilingSigns) for(var support:tops) for(float yaw:new float[]{0,22.5f,45,90,135,180,225,270,315})
            for(boolean secondary:new boolean[]{false,true}) for(var clicked:fluids.subList(0,2)) {
                var neighbors=cells(air);neighbors[Direction.UP.get3DDataValue()]=support;
                capture("ceiling-chain",block,yaw,0,secondary,Direction.DOWN,-1,clicked,neighbors,0,"none",level,player);
            }
        var wallSigns=blocks.stream().filter(b->b instanceof WallHangingSignBlock).toList();
        for(var block:wallSigns) for(var partner:wallSigns) for(var facing:ContainerPlacementVectors.SIDES)
            for(float yaw:new float[]{0,90,180,270}) for(var face:Direction.values()) {
                var neighbors=cells(air);neighbors[Direction.EAST.get3DDataValue()]=partner.defaultBlockState().setValue(WallHangingSignBlock.FACING,facing);
                neighbors[Direction.WEST.get3DDataValue()]=neighbors[Direction.EAST.get3DDataValue()];
                capture("wall-chain",block,yaw,0,false,face,-1,air,neighbors,0,"none",level,player);
            }
        for(int mask=0;mask<64;mask++) for(float yaw:new float[]{0,90,180,270}) for(var face:Direction.values()) {
            var neighbors=cells(air);for(var side:Direction.values())if((mask&(1<<side.get3DDataValue()))!=0)neighbors[side.get3DDataValue()]=stone;
            capture("bell-mask",Blocks.BELL,yaw,0,false,face,-1,air,neighbors,0,"none",level,player);
        }
        for(var support:palette) for(var face:Direction.values())
            capture("bell-support",Blocks.BELL,0,0,false,face,-1,air,cells(support),0,"none",level,player);
        for(var block:blocks) if(block instanceof CampfireBlock) for(var below:List.of(air,stone,Blocks.HAY_BLOCK.defaultBlockState()))
            for(var clicked:fluids) for(float yaw:new float[]{0,90,180,270}) {
                var neighbors=cells(air);neighbors[Direction.DOWN.get3DDataValue()]=below;
                capture("campfire-smoke",block,yaw,0,false,Direction.UP,-1,clicked,neighbors,0,"none",level,player);
            }
        for(var block:blocks) if(block instanceof SkullBlock || block instanceof WallSkullBlock || block instanceof ShelfBlock || block instanceof CrafterBlock)
            for(int mask=0;mask<64;mask++) {
                var neighbors=cells(stone);for(var side:Direction.values())if((mask&(1<<side.get3DDataValue()))!=0)neighbors[side.get3DDataValue()]=Blocks.REDSTONE_BLOCK.defaultBlockState();
                capture("power-mask",block,90,45,false,Direction.UP,-1,air,neighbors,0,"none",level,player);
            }
        var angles=new ArrayList<Float>(List.of(-720f,-360f,-180f,-0.0f,0f,180f,360f,720f,Float.MAX_VALUE,-Float.MAX_VALUE));
        for(int i=-16;i<=16;i++) {float boundary=i*22.5f+11.25f;angles.add(Math.nextDown(boundary));angles.add(boundary);angles.add(Math.nextUp(boundary));}
        for(var block:blocks) if(block instanceof BannerBlock || block instanceof StandingSignBlock || block instanceof SkullBlock)
            for(float yaw:angles) capture("rotation-boundary",block,yaw,0,false,Direction.UP,-1,air,cells(air),0,"none",level,player);
        for(var component:List.of("none","book:absent","book:compound","book:int","book:wrong_key","book:wrong_type"))
            for(int capability=0;capability<4;capability++) for(float yaw:new float[]{0,90,180,270}) for(int directional:new int[]{-2,-1,0,1,2,3,4,5})
                capture("lectern-components",Blocks.LECTERN,yaw,0,false,Direction.UP,directional,air,cells(air),capability,component,level,player);
        for(var mode:List.of("none","mode:start","mode:log","mode:fail","mode:accept","mode:invalid","mode:START","mode:"))
            for(float yaw:new float[]{0,90,180,270}) for(var face:Direction.values())
                capture("test-mode",Blocks.TEST_BLOCK,yaw,0,false,face,-1,air,cells(air),0,mode,level,player);
    }
}
