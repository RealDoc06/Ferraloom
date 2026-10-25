import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.Vec3;

/** Real ServerLevel loot, Level#destroyBlock and LevelChunk removal callbacks.
 * Only outbound effects and chunk acquisition are captured; no allocation bypass. */
public class DestroyBlockVectors extends ServerLevel {
    static final BlockPos POS = new BlockPos(8, 64, 8);
    final Map<ChunkPos, LevelChunk> chunks = new HashMap<>();
    final List<String> trace = new ArrayList<>();
    boolean recording;

    DestroyBlockVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) {
        super(server, Runnable::run, storage, server.getWorldData().overworldData(), Level.OVERWORLD,
            server.registryAccess().lookupOrThrow(Registries.LEVEL_STEM).getValue(LevelStem.OVERWORLD),
            false, 0, List.of(), false);
    }
    @Override public ChunkAccess getChunk(int x, int z, ChunkStatus status, boolean load) {
        var pos = new ChunkPos(x,z);
        return load ? chunks.computeIfAbsent(pos, p -> new LevelChunk(this,p) {
            @Override public FullChunkStatus getFullStatus() { return FullChunkStatus.BLOCK_TICKING; }
        }) : chunks.get(pos);
    }
    @Override public void sendBlockUpdated(BlockPos pos, BlockState old, BlockState state, int flags) {}
    @Override public void updatePOIOnBlockStateChange(BlockPos pos, BlockState old, BlockState state) {}
    @Override public void levelEvent(Entity source, int type, BlockPos pos, int data) {
        if(recording) trace.add("event:"+type+":"+(source==null?"none":"breaker")+":"+data);
        super.levelEvent(source,type,pos,data);
    }
    @Override public void gameEvent(Holder<GameEvent> event, Vec3 pos, GameEvent.Context context) {
        if(recording) trace.add("game:"+BuiltInRegistries.GAME_EVENT.getKey(event.value())+":"+
            (context.sourceEntity()==null?"none":"breaker")+":"+Block.getId(context.affectedState()));
    }
    @Override public boolean setBlock(BlockPos pos, BlockState state, int flags, int limit) {
        if(recording && pos.equals(POS)) trace.add("set:"+Block.getId(state)+":"+flags+":"+limit);
        return super.setBlock(pos,state,flags,limit);
    }
    // Ordinary popResource uses the item's independent nondeterministic RNG;
    // container scatter has an explicit level-RNG velocity and is checked bitwise.
    @Override public boolean addFreshEntity(Entity entity) {
        if(!(entity instanceof ItemEntity item)) throw new UnsupportedOperationException("unexpected entity");
        var velocity=item.getDeltaMovement();
        if(recording) trace.add("item:"+BuiltInRegistries.ITEM.getKey(item.getItem().getItem())+":"+
            item.getItem().getCount()+":"+bits(item.getX())+","+bits(item.getY())+","+bits(item.getZ())+":"+
            (item.getItem().is(Items.DIAMOND)?bits(velocity.x)+","+bits(velocity.y)+","+bits(velocity.z):"private-rng"));
        return true;
    }
    static String bits(double value) { return Long.toUnsignedString(Double.doubleToRawLongBits(value),16); }
    void scenario(String name, BlockState state, boolean drops, boolean enabled, long seed, boolean contents) {
        recording=false; chunks.clear(); trace.clear();
        getGameRules().set(GameRules.BLOCK_DROPS,enabled,null);
        if(state.is(Blocks.FIRE)) getChunkAt(POS).setBlockState(POS.below(),Blocks.STONE.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
        getChunkAt(POS).setBlockState(POS,state,Block.UPDATE_KNOWN_SHAPE);
        if(contents) ((ChestBlockEntity)getBlockEntity(POS)).setItem(0,new ItemStack(Items.DIAMOND,3));
        trace.clear(); random.setSeed(seed); recording=true;
        boolean result=destroyBlock(POS,drops,null,7);
        recording=false;
        System.out.println("VECTOR\t"+name+":"+drops+":"+enabled+":"+seed+"\t"+result+"|"+
            Block.getId(getBlockState(POS))+"|"+random.nextInt()+"|"+String.join(";",trace));
    }
    void audiences(GameTestServer server) {
        var received=new ArrayList<String>();
        var players=new ArrayList<ServerPlayer>();
        int index=0;
        for(double offset:new double[]{0,63.999,64.0,-64.0}) {
            int id=index++;
            var profile=new com.mojang.authlib.GameProfile(new java.util.UUID(0,id+1),"player"+id);
            var player=new ServerPlayer(server,this,profile,ClientInformation.createDefault());
            player.setPos(POS.getX()+offset,POS.getY(),POS.getZ());
            new net.minecraft.server.network.ServerGamePacketListenerImpl(server,
                new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND),player,
                net.minecraft.server.network.CommonListenerCookie.createInitial(profile,false)) {
                @Override public void send(net.minecraft.network.protocol.Packet<?> packet) {
                    if(packet instanceof net.minecraft.network.protocol.game.ClientboundLevelEventPacket event)
                        received.add(id+":"+event.getType());
                }
            };
            server.getPlayerList().getPlayers().add(player); players.add(player);
        }
        for(String operation:List.of("destroy","player-particles","straw-particles","fire-particles")) {
            recording=false; chunks.clear(); trace.clear(); received.clear();
            var state=Blocks.STONE.defaultBlockState();
            getChunkAt(POS).setBlockState(POS,state,Block.UPDATE_KNOWN_SHAPE);
            switch(operation) {
                case "destroy" -> destroyBlock(POS,false,players.getFirst(),7);
                case "player-particles" -> state.getBlock().spawnDestroyByEntityParticles(this,players.getFirst(),POS,state);
                case "straw-particles" -> Blocks.STRAW_BED.spawnDestroyParticles(this,POS,Blocks.STRAW_BED.defaultBlockState());
                case "fire-particles" -> Blocks.FIRE.spawnDestroyParticles(this,POS,Blocks.FIRE.defaultBlockState());
            }
            System.out.println("VECTOR\taudience:"+operation+"\t"+String.join(",",received));
        }
        server.getPlayerList().getPlayers().clear();
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory("destroy-block")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,
            ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()) {});
        var level=new DestroyBlockVectors(server,storage);
        for(long seed:new long[]{0,1,9123456}) for(boolean drops:new boolean[]{false,true}) for(boolean enabled:new boolean[]{false,true}) {
            level.scenario("planks",Blocks.OAK_PLANKS.defaultBlockState(),drops,enabled,seed,false);
            level.scenario("chest",Blocks.CHEST.defaultBlockState(),drops,enabled,seed,true);
        }
        for(var block:List.of(Blocks.AIR,Blocks.STONE,Blocks.WATER,Blocks.LAVA,Blocks.FIRE))
            level.scenario(BuiltInRegistries.BLOCK.getKey(block).getPath(),block.defaultBlockState(),true,true,0,false);
        level.scenario("chain",Blocks.IRON_CHAIN.defaultBlockState().setValue(
            net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED,true),true,true,0,false);
        level.audiences(server);
        System.out.flush(); System.exit(0);
    }
}

class DestroyBlockCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors);
        DestroyBlockVectors.run();
    }
}
