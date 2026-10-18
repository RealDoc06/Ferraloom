import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.material.*;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.*;
import net.minecraft.world.phys.shapes.*;

/** Real 26.3 BlockGetter#clip/traverseBlocks, shape virtuals and fluid memo.
 * Reads use a bounded required-chunk service. Fresh cache setup isolates native
 * first-query cases; the code being observed is never replaced. */
public final class ClipVectors implements BlockGetter {
    final Map<BlockPos,BlockState> states=new LinkedHashMap<>();
    final Set<ChunkPos> loaded=new HashSet<>();
    final List<String> reads=new ArrayList<>();
    ClipVectors(String chunks) {
        for(String key:chunks.split(";"))if(!key.isEmpty()) {
            var p=key.split(",");loaded.add(new ChunkPos(Integer.parseInt(p[0]),Integer.parseInt(p[1])));
        }
    }
    @Override public int getHeight(){return 384;}
    @Override public int getMinY(){return -64;}
    @Override public BlockEntity getBlockEntity(BlockPos pos){throw new UnsupportedOperationException();}
    @Override public BlockState getBlockState(BlockPos pos){
        reads.add(p(pos));
        if(pos.getY() < -64 || pos.getY() >=320)return Blocks.VOID_AIR.defaultBlockState();
        var cp=new ChunkPos(pos.getX()>>4,pos.getZ()>>4);if(!loaded.contains(cp))throw new IllegalStateException("missing:"+cp.x()+","+cp.z());
        return states.getOrDefault(pos,Blocks.AIR.defaultBlockState());
    }
    @Override public FluidState getFluidState(BlockPos pos){return getBlockState(pos).getFluidState();}
    static String p(BlockPos p){return p.getX()+","+p.getY()+","+p.getZ();}
    static String v(Vec3 v){return v.x+","+v.y+","+v.z;}
    static String hit(BlockHitResult h){return h.getType()+"|"+v(h.getLocation())+"|"+h.getDirection().get3DDataValue()+"|"+p(h.getBlockPos())+"|"+h.isInside()+"|"+h.isWorldBorderHit();}
    String states(){return states.entrySet().stream().map(e->p(e.getKey())+","+Block.getId(e.getValue())).collect(Collectors.joining(";"));}
    static void clearFluidShapes() throws Exception {
        var f=FlowingFluid.class.getDeclaredField("shapes");f.setAccessible(true);
        for(var fluid:List.of(Fluids.WATER,Fluids.FLOWING_WATER,Fluids.LAVA,Fluids.FLOWING_LAVA))((Map<?,?>)f.get(fluid)).clear();
    }
    void clip(String name,String chunks,Vec3 from,Vec3 to,ClipContext.Block block,ClipContext.Fluid fluid)throws Exception {
        clearFluidShapes();reads.clear();String result;
        try{result=hit(clip(new ClipContext(from,to,block,fluid,CollisionContext.empty())));}
        catch(IllegalStateException error){result=error.getMessage();}
        System.out.println("clip\t"+name+"\t"+chunks+"\t"+states()+"\t"+v(from)+"\t"+v(to)+"\t"+block+"\t"+fluid+"\t"+result+"\t"+String.join(";",reads));
    }
    static void traversal(String name,Vec3 from,Vec3 to,int stop){
        var visited=new ArrayList<String>();
        BlockGetter.traverseBlocks(from,to,0,(context,pos)->{visited.add(p(pos));return visited.size()==stop?true:null;},context->false);
        System.out.println("traverse\t"+name+"\t"+v(from)+"\t"+v(to)+"\t"+stop+"\t"+String.join(";",visited));
    }
    static void memo(boolean firstAbove)throws Exception {
        clearFluidShapes();var world=new ClipVectors("0,0");var pos=new BlockPos(8,64,8);
        var state=Fluids.WATER.defaultFluidState();
        var values=new ArrayList<String>();
        for(boolean above:new boolean[]{firstAbove,!firstAbove,firstAbove}) {
            world.states.clear();world.states.put(pos,Blocks.WATER.defaultBlockState());
            if(above)world.states.put(pos.above(),Blocks.WATER.defaultBlockState());
            world.reads.clear();values.add(state.getShape(world,pos).max(Direction.Axis.Y)+":"+world.reads.size());
        }
        System.out.println("memo\t"+firstAbove+"\t"+String.join(",",values));
    }
    static void interaction() {
        var world=new ClipVectors("0,0");var pos=new BlockPos(8,64,8);
        var block=Blocks.SCAFFOLDING;
        var shape=Shapes.box(.5,0,0,1,1,1);
        for(var ray:List.of(new Vec3[]{new Vec3(7,63.375,8.5),new Vec3(10,64.875,8.5)},new Vec3[]{new Vec3(7,64.25,8.5),new Vec3(10,64.25,8.5)})) {
            var base=shape.clip(ray[0],ray[1],pos);
            var result=world.clipWithInteractionOverride(ray[0],ray[1],pos,shape,block.defaultBlockState());
            System.out.println("interaction\t"+v(ray[0])+"\t"+v(ray[1])+"\t"+hit(base)+"\t"+hit(result));
        }
    }
    static void playerShapes()throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory("clip-player")).createAccess("world");
        var server=net.minecraft.gametest.framework.GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new net.minecraft.server.players.PlayerList(server,server.registries(),storage.createPlayerStorage(),new net.minecraft.server.notifications.EmptyNotificationService()) {});
        var level=new DestroyBlockVectors(server,storage);
        var player=new ServerPlayer(server,level,new com.mojang.authlib.GameProfile(new java.util.UUID(0,1),"Clip"),ClientInformation.createDefault());
        var pos=new BlockPos(8,64,8);var from=new Vec3(7,64.5,8.5);var to=new Vec3(10,64.5,8.5);
        for(var block:List.of(Blocks.STONE,Blocks.HAY_BLOCK,Blocks.END_PORTAL,Blocks.END_GATEWAY,Blocks.NETHER_PORTAL))for(int delay:new int[]{80,0}) {
            level.getGameRules().set(GameRules.PLAYERS_NETHER_PORTAL_DEFAULT_DELAY,delay,null);
            var context=new ClipContext(from,to,ClipContext.Block.FALLDAMAGE_RESETTING,ClipContext.Fluid.NONE,player);
            System.out.println("player-shape\t"+Block.getId(block.defaultBlockState())+"\t"+delay+"\t"+!context.getBlockShape(block.defaultBlockState(),level,pos).isEmpty());
        }
    }
    public static void main(String[] args)throws Exception {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);NeighborVectors.load();
        var states=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true),Blocks.DANDELION.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,5),Blocks.LAVA.defaultBlockState());
        var rays=List.of(new Vec3[]{new Vec3(7,64.25,8.5),new Vec3(10,64.25,8.5)},new Vec3[]{new Vec3(8.5,66,8.5),new Vec3(8.5,63,8.5)},new Vec3[]{new Vec3(8.5,64.25,8.5),new Vec3(10,64.25,8.5)},new Vec3[]{new Vec3(7,65,8.5),new Vec3(10,65,8.5)});
        int index=0;
        for(var state:states)for(var block:ClipContext.Block.values())for(var fluid:ClipContext.Fluid.values())for(var ray:rays) {
            var world=new ClipVectors("0,0");world.states.put(new BlockPos(8,64,8),state);
            world.clip("matrix-"+(index++),"0,0",ray[0],ray[1],block,fluid);
        }
        for(boolean hit:new boolean[]{false,true}) {
            var world=new ClipVectors("0,0");if(hit)world.states.put(new BlockPos(15,64,8),Blocks.STONE.defaultBlockState());
            world.clip("missing-"+hit,"0,0",new Vec3(14.5,64.5,8.5),new Vec3(17.5,64.5,8.5),ClipContext.Block.OUTLINE,ClipContext.Fluid.NONE);
        }
        var empty=new ClipVectors("");empty.clip("zero-no-read","",new Vec3(19,64,8),new Vec3(19,64,8),ClipContext.Block.OUTLINE,ClipContext.Fluid.ANY);
        empty.clip("outside-no-read","",new Vec3(19,321,8),new Vec3(22,321,8),ClipContext.Block.OUTLINE,ClipContext.Fluid.NONE);
        var negative=new ClipVectors("-1,0;0,0");negative.states.put(new BlockPos(-1,64,8),Blocks.STONE.defaultBlockState());negative.clip("negative-start","-1,0;0,0",new Vec3(0,64.5,8.5),new Vec3(-3,64.5,8.5),ClipContext.Block.OUTLINE,ClipContext.Fluid.NONE);
        for(var ray:List.of(new Vec3[]{new Vec3(0,0,0),new Vec3(2,2,2)},new Vec3[]{new Vec3(2,2,2),new Vec3(0,0,0)},new Vec3[]{new Vec3(-1,-1,-1),new Vec3(-3,-3,-3)},new Vec3[]{new Vec3(.5,0,.5),new Vec3(.5,0,.5)},new Vec3[]{new Vec3(15.5,15.5,15.5),new Vec3(16.5,16.5,16.5)},new Vec3[]{new Vec3(0,0,0),new Vec3(1e-10,0,0)}))for(int stop:new int[]{0,1,3})traversal("ties",ray[0],ray[1],stop);
        memo(false);memo(true);interaction();playerShapes();
        var result=new BlockHitResult(new Vec3(8.5,64.25,8.75),Direction.EAST,new BlockPos(8,64,8),true,true);
        System.out.println("with-position\t"+hit(result.withPosition(new BlockPos(-1,0,16))));
        System.out.println("with-position\t"+hit(BlockHitResult.miss(new Vec3(8.5,64.25,8.75),Direction.EAST,new BlockPos(8,64,8)).withPosition(new BlockPos(-1,0,16))));
        System.out.flush();System.exit(0);
    }
}
