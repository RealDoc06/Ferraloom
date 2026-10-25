import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.FallingBlockEntity;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.ticks.*;

/** Actual FallingBlock ticks and AnvilBlock/Fallable methods; the world's
 * addFreshEntity sink captures the same receiver before native falling sets it. */
public final class AnvilVectors extends RedstoneIdentityVectors {
    final LevelTicks<Block> ticks = new LevelTicks<>(pos -> true);
    final LevelTicks<Fluid> fluidTicks = new LevelTicks<>(pos -> true);
    final List<FallingBlockEntity> added = new ArrayList<>();
    long time;
    AnvilVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) { super(server, storage); }
    @Override public long getGameTime() { return time; }
    @Override public LevelTicks<Block> getBlockTicks() { return ticks; }
    @Override public LevelTicks<Fluid> getFluidTicks() { return fluidTicks; }
    @Override public boolean addFreshEntity(Entity entity) { if(entity instanceof FallingBlockEntity falling)added.add(falling); return true; }
    void prepare(BlockState state, BlockState below) {
        for(int x=-1;x<=1;x++) for(int z=-1;z<=1;z++) { ticks.removeContainer(new ChunkPos(x,z)); fluidTicks.removeContainer(new ChunkPos(x,z)); }
        super.reset(); added.clear(); time=0;
        for(int x=-1;x<=1;x++) for(int z=-1;z<=1;z++) {
            var chunk=new ChunkPos(x,z); ticks.addContainer(chunk,new LevelChunkTicks<>());
            fluidTicks.addContainer(chunk,new LevelChunkTicks<>());
            getChunkAt(new BlockPos(x*16,64,z*16)).setLoaded(true);
        }
        getChunkAt(POS).setBlockState(POS.below(),below,816);
        getChunkAt(POS).setBlockState(POS,state,816);
        events.clear();
    }
    static void out(String... values) { System.out.println("BUILDING\t"+String.join("\t",values)); }
    void capture() throws Exception {
        var hurt=FallingBlockEntity.class.getDeclaredField("hurtEntities"); hurt.setAccessible(true);
        var amount=FallingBlockEntity.class.getDeclaredField("fallDamagePerDistance"); amount.setAccessible(true);
        var max=FallingBlockEntity.class.getDeclaredField("fallDamageMax"); max.setAccessible(true);
        for(var block:List.of(Blocks.ANVIL,Blocks.CHIPPED_ANVIL,Blocks.DAMAGED_ANVIL)) for(var state:block.getStateDefinition().getPossibleStates()) {
            var next=AnvilBlock.damage(state);
            out("damage",""+Block.getId(state),""+(next==null?-1:Block.getId(next)));
            for(var below:List.of(Blocks.AIR,Blocks.STONE,Blocks.WATER,Blocks.FIRE,Blocks.COBWEB,Blocks.STONE_SLAB)) for(boolean moved:new boolean[]{false,true}) {
                prepare(state,below.defaultBlockState());
                state.onPlace(this,POS,Blocks.AIR.defaultBlockState(),moved);
                for(int tick=0;tick<=3;tick++) {
                    time=tick;
                    ticks.tick(tick,65536,(pos,type)->{var current=getBlockState(pos); if(current.is(type))current.tick(this,pos,random);});
                    var receiver=added.isEmpty()?null:added.getFirst();
                    out("tick",""+Block.getId(state),""+Block.getId(below.defaultBlockState()),""+moved,""+tick,
                        ""+Block.getId(getBlockState(POS)),""+ticks.hasScheduledTick(POS,block),""+added.size(),
                        receiver==null?"-":hurt.getBoolean(receiver)+","+Float.floatToRawIntBits(amount.getFloat(receiver))+","+max.getInt(receiver));
                }
            }
            for(boolean silent:new boolean[]{false,true}) for(boolean broken:new boolean[]{false,true}) {
                prepare(state,Blocks.STONE.defaultBlockState());
                var entity=new FallingBlockEntity(EntityTypes.FALLING_BLOCK,this); entity.setSilent(silent);
                if(broken)((Fallable)block).onBrokenAfterFall(this,POS,entity);
                else ((Fallable)block).onLand(this,POS,state,Blocks.AIR.defaultBlockState(),entity);
                out("event",""+Block.getId(state),""+silent,""+broken,events.toString());
            }
        }
        for(var block:List.of(Blocks.AIR,Blocks.STONE,Blocks.SAND,Blocks.ANVIL,Blocks.CHIPPED_ANVIL,Blocks.DAMAGED_ANVIL)) {
            prepare(block.defaultBlockState(),Blocks.STONE.defaultBlockState());
            var entity=new FallingBlockEntity(EntityTypes.FALLING_BLOCK,this);
            var source=block instanceof Fallable fallable?fallable.getFallDamageSource(entity):entity.damageSources().fallingBlock(entity);
            out("source",""+BuiltInRegistries.BLOCK.getId(block),source.typeHolder().unwrapKey().orElseThrow().identifier().toString(),""+(source.getDirectEntity()==entity),""+(source.getEntity()==entity));
            if(block instanceof Fallable fallable && !(block instanceof AnvilBlock)) {
                fallable.onLand(this,POS,block.defaultBlockState(),Blocks.AIR.defaultBlockState(),entity);
                fallable.onBrokenAfterFall(this,POS,entity);
                out("base-events",""+BuiltInRegistries.BLOCK.getId(block),events.toString());
            }
        }
        for(var block:List.of(Blocks.AIR,Blocks.STONE)) out("damage",""+Block.getId(block.defaultBlockState()),""+(AnvilBlock.damage(block.defaultBlockState())==null?-1:0));
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"anvil-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        new AnvilVectors(server,storage).capture();
        System.exit(0);
    }
}

final class AnvilCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors);
        AnvilVectors.run();
    }
}
