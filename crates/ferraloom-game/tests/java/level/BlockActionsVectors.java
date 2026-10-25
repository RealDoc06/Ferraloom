import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import com.mojang.serialization.JsonOps;
import com.google.gson.JsonParser;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.nbt.*;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.dedicated.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.*;
import net.minecraft.server.notifications.*;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.storage.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real gameMode/player/ServerLevel action sequences. Only chunk acquisition,
 * outbound transport and dedicated-server startup inputs are fixture services.
 * The protection branch calls DedicatedServer's real method over a real op list;
 * no mining method or result is replaced. */
public final class BlockActionsVectors extends DestroyBlockVectors {
    final List<String> actions = new ArrayList<>();
    MinecraftServer protectionServer;
    boolean capture;
    final GameTestServer actualServer;
    final Path scratch;
    ServerPlayer miner;
    int nextId;
    String name;
    BlockActionsVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage, Path scratch) {
        super(server,storage);actualServer=server;this.scratch=scratch;
    }
    @Override public MinecraftServer getServer() { return protectionServer==null?super.getServer():protectionServer; }
    @Override public boolean hasChunk(int x,int z) {return chunks.containsKey(new ChunkPos(x,z));}
    @Override public LevelData.RespawnData getRespawnData() {
        return new LevelData.RespawnData(GlobalPos.of(Level.OVERWORLD,POS),0,0);
    }
    static String pos(BlockPos pos) {return pos.getX()+","+pos.getY()+","+pos.getZ();}
    @Override public void levelEvent(Entity source,int event,BlockPos pos,int data) {
        if(capture) actions.add("L:"+event+":"+pos(pos)+":"+data);
        // Audience is tested separately for cracks. Other levelEvent transport
        // stays captured at this native call boundary, as in the mining oracle.
    }
    @Override public void gameEvent(Holder<GameEvent> event,Vec3 position,GameEvent.Context context) {
        if(capture) actions.add("G:"+BuiltInRegistries.GAME_EVENT.getKey(event.value())+":"+
            Block.getId(context.affectedState())+":"+(context.sourceEntity()==miner?"miner":"none"));
    }
    void packet(int recipient,Packet<?> packet) {
        if(!capture) return;
        if(packet instanceof ClientboundBlockDestructionPacket value)
            actions.add("C:"+recipient+":"+pos(value.getPos())+":"+value.getProgress());
        else if(packet instanceof ClientboundBlockUpdatePacket value)
            actions.add("U:"+recipient+":"+pos(value.getPos())+":"+Block.getId(value.getBlockState()));
        else if(packet instanceof ClientboundSystemChatPacket value) {
            var text=(net.minecraft.network.chat.contents.TranslatableContents)value.content().getContents();
            actions.add("S:"+recipient+":"+value.overlay()+":"+text.getKey()+":"+text.getArgs()[0]+":"+value.content().getStyle().getColor());
        }
    }
    ServerPlayer player(int recipient,GameType mode,Vec3 position) throws Exception {
        var profile=new GameProfile(new UUID(0,++nextId),"break"+nextId);
        var player=new ServerPlayer(actualServer,this,profile,ClientInformation.createDefault());
        new ServerGamePacketListenerImpl(actualServer,
            new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND),player,
            CommonListenerCookie.createInitial(profile,false)) {
            @Override public void send(Packet<?> value) {packet(recipient,value);}
            @Override public void send(Packet<?> value,io.netty.channel.ChannelFutureListener listener) {packet(recipient,value);}
        };
        var setter=ServerPlayerGameMode.class.getDeclaredMethod("setGameModeForPlayer",GameType.class,GameType.class);
        setter.setAccessible(true);setter.invoke(player.gameMode,mode,null);
        player.setPos(position);player.setOnGround(true);
        actualServer.getPlayerList().getPlayers().add(player);
        return player;
    }
    void setup(String name,GameType type,ItemStack hand,boolean protection,double border,Vec3 position) throws Exception {
        capture=false;protectionServer=null;chunks.clear();actions.clear();actualServer.getPlayerList().getPlayers().clear();
        this.name=name;
        getWorldBorder().setSize(border);
        miner=player(0,type,position);
        miner.getInventory().setItem(0,hand);
        player(1,GameType.SPECTATOR,new Vec3(8,64,8));
        for(int x:new int[]{8,9}) getChunkAt(new BlockPos(x,64,8)).setBlockState(new BlockPos(x,64,8),Blocks.STONE.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
        if(protection) {
            var server=mock(DedicatedServer.class);
            var list=mock(DedicatedPlayerList.class);
            var ops=new ServerOpList(scratch.resolve(name+"-ops.json").toFile(),mock(NotificationService.class));
            ops.add(new ServerOpListEntry(new NameAndId(new UUID(100,100),"operator"),LevelBasedPermissionSet.forLevel(PermissionLevel.OWNERS),false));
            var opsField=PlayerList.class.getDeclaredField("ops");opsField.setAccessible(true);opsField.set(list,ops);
            var serverField=PlayerList.class.getDeclaredField("server");serverField.setAccessible(true);serverField.set(list,server);
            doReturn(list).when(server).getPlayerList();doReturn(16).when(server).spawnProtectionRadius();
            doReturn(ops).when(list).getOps();doCallRealMethod().when(list).isOp(any());
            doCallRealMethod().when(server).isUnderSpawnProtection(any(),any(),any());
            protectionServer=server;
            if(!server.isUnderSpawnProtection(this,POS,miner)) throw new AssertionError("real dedicated protection");
        }
        String stack=hand.isEmpty()?"-":ItemStack.CODEC.encodeStart(RegistryOps.create(NbtOps.INSTANCE,registryAccess()),hand).getOrThrow().toString();
        System.out.println("BLOCK_ACTION\tsetup\t"+name+"\t"+type.getName()+"\t"+stack+"\t"+protection+"\t"+border+"\t"+position.x+","+position.y+","+position.z+"\t"+getMinY()+"\t"+getMaxY());
        actions.clear();capture=true;
    }
    String traceActions() {return actions.isEmpty()?"-":String.join(";",actions);}
    String state() throws Exception {
        var mode=miner.gameMode;
        return MiningVectors.field(mode,"isDestroyingBlock")+"/"+MiningVectors.field(mode,"destroyProgressStart")+"/"+
            pos((BlockPos)MiningVectors.field(mode,"destroyPos"))+"/"+MiningVectors.field(mode,"destroyDirection")+"/"+
            MiningVectors.field(mode,"gameTicks")+"/"+MiningVectors.field(mode,"hasDelayedDestroy")+"/"+
            pos((BlockPos)MiningVectors.field(mode,"delayedDestroyPos"))+"/"+MiningVectors.field(mode,"delayedTickStart")+"/"+
            MiningVectors.field(mode,"lastSentState");
    }
    void output(String input) throws Exception {
        System.out.println("BLOCK_ACTION\tstep\t"+name+"\t"+input+"\t"+state()+"\t"+
            Block.getId(getBlockState(POS))+","+Block.getId(getBlockState(POS.east()))+"\t"+traceActions());
        actions.clear();
    }
    void action(ServerboundPlayerActionPacket.Action action,BlockPos pos,Direction direction) throws Exception {
        miner.gameMode.handleBlockBreakAction(pos,action,direction,getMaxY(),7);
        output(action.name()+":"+pos(pos)+":"+direction.name());
    }
    void tick() throws Exception {miner.gameMode.tick();output("tick");}
    void field(String name,Object value) throws Exception {MiningVectors.field(miner.gameMode,name,value);output("field:"+name+":"+value);}
    void move(Vec3 pos) throws Exception {miner.setPos(pos);output("move:"+pos.x+","+pos.y+","+pos.z);}
    void replace(BlockPos pos,Block block) throws Exception {
        getChunkAt(pos).setBlockState(pos,block.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
        output("replace:"+pos(pos)+":"+BuiltInRegistries.BLOCK.getKey(block));
    }
    void sequences() throws Exception {
        var start=ServerboundPlayerActionPacket.Action.START_DESTROY_BLOCK;
        var stop=ServerboundPlayerActionPacket.Action.STOP_DESTROY_BLOCK;
        var abort=ServerboundPlayerActionPacket.Action.ABORT_DESTROY_BLOCK;
        var change=ServerboundPlayerActionPacket.Action.CHANGE_DESTROY_DIRECTION;
        var origin=new Vec3(8.5,64,8.5);
        setup("replacement",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        action(start,POS,Direction.NORTH);tick();action(start,POS.east(),Direction.EAST);action(abort,POS,Direction.UP);
        setup("early-stop",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        field("gameTicks",2);action(start,POS,Direction.NORTH);action(stop,POS,Direction.UP);tick();tick();action(abort,POS.east(),Direction.DOWN);tick();
        replace(POS,Blocks.AIR);tick();
        setup("active",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        action(start,POS,Direction.SOUTH);action(change,POS,Direction.WEST);tick();tick();tick();tick();replace(POS,Blocks.AIR);tick();
        setup("active-over-threshold",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        action(start,POS,Direction.UP);field("gameTicks",150);tick();
        setup("out-of-range",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        action(start,POS,Direction.UP);action(abort,new BlockPos(100,64,8),Direction.DOWN);
        action(start,new BlockPos(100,64,8),Direction.DOWN);action(change,new BlockPos(100,64,8),Direction.EAST);
        setup("upper-bound",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,new Vec3(8.5,getMaxY()+1,8.5));
        action(start,new BlockPos(8,getMaxY()+1,8),Direction.UP);action(abort,new BlockPos(8,getMaxY()+1,8),Direction.DOWN);
        setup("upper-inclusive",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,new Vec3(8.5,getMaxY(),8.5));
        action(start,new BlockPos(8,getMaxY(),8),Direction.UP);tick();
        setup("protection",GameType.SURVIVAL,ItemStack.EMPTY,true,100000,origin);action(start,POS,Direction.UP);
        setup("border",GameType.SURVIVAL,ItemStack.EMPTY,false,10,origin);action(start,POS,Direction.UP);
        setup("adventure-deny",GameType.ADVENTURE,new ItemStack(Items.STICK),false,100000,origin);action(start,POS,Direction.UP);
        for(String predicate:List.of("{\"blocks\":\"minecraft:stone\"}","{\"blocks\":\"minecraft:dirt\"}",
            "{\"blocks\":\"minecraft:stone\",\"components\":{\"minecraft:custom_name\":{\"text\":\"absent\"}}}",
            "{\"blocks\":\"minecraft:stone\",\"nbt\":{\"id\":\"minecraft:chest\"}}")) {
            var stack=new ItemStack(Items.STICK);
            stack.set(DataComponents.CAN_BREAK,AdventureModePredicate.CODEC.parse(RegistryOps.create(JsonOps.INSTANCE,registryAccess()),JsonParser.parseString(predicate)).getOrThrow());
            setup("adventure-"+(predicate.contains("components")?"components":predicate.contains("nbt")?"nbt":predicate.contains("dirt")?"wrong":"allow"),GameType.ADVENTURE,stack,false,100000,origin);
            action(start,POS,Direction.UP);
        }
        var chestStack=new ItemStack(Items.STICK);
        chestStack.set(DataComponents.CAN_BREAK,AdventureModePredicate.CODEC.parse(RegistryOps.create(JsonOps.INSTANCE,registryAccess()),
            JsonParser.parseString("{\"blocks\":\"minecraft:chest\",\"nbt\":{\"id\":\"minecraft:chest\"}}")).getOrThrow());
        setup("adventure-chest-nbt",GameType.ADVENTURE,chestStack,false,100000,origin);
        replace(POS,Blocks.CHEST);action(start,POS,Direction.UP);
        setup("spectator",GameType.SPECTATOR,ItemStack.EMPTY,false,100000,origin);action(start,POS,Direction.UP);
        setup("creative",GameType.CREATIVE,ItemStack.EMPTY,false,100000,origin);action(start,POS,Direction.UP);
        var refused=new ItemStack(Items.STICK);
        refused.set(DataComponents.TOOL,new net.minecraft.world.item.component.Tool(List.of(),1f,0,false));
        setup("creative-refusal",GameType.CREATIVE,refused,false,100000,origin);action(start,POS,Direction.UP);
        setup("unarmed-stop",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        action(start,POS,Direction.UP);field("gameTicks",104);action(stop,POS,Direction.UP);
        setup("delayed-complete",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,origin);
        field("gameTicks",150);action(start,POS,Direction.UP);action(stop,POS,Direction.UP);tick();
    }
    void listener() throws Exception {
        setup("listener",GameType.SURVIVAL,ItemStack.EMPTY,false,100000,new Vec3(8.5,64,8.5));
        var connection=miner.connection;
        var timer=ServerGamePacketListenerImpl.class.getDeclaredField("clientLoadedTimeoutTimer");timer.setAccessible(true);
        var ack=ServerGamePacketListenerImpl.class.getDeclaredField("ackBlockChangesUpTo");ack.setAccessible(true);
        int[] sequences={1,3,1,9,-1};
        for(int i=0;i<sequences.length;i++) {
            timer.set(connection,i==0?60:0);
            var action=i<2?ServerboundPlayerActionPacket.Action.START_DESTROY_BLOCK:i==2?
                ServerboundPlayerActionPacket.Action.CHANGE_DESTROY_DIRECTION:ServerboundPlayerActionPacket.Action.ABORT_DESTROY_BLOCK;
            var target=i==3?new BlockPos(100,64,8):POS;
            String failure="ok";
            try {connection.handlePlayerAction(new ServerboundPlayerActionPacket(action,target,Direction.UP,sequences[i]));}
            catch(IllegalArgumentException error) {failure=error.getClass().getSimpleName();}
            System.out.println("BLOCK_ACTION\tlistener\t"+connection.hasClientLoaded()+"\t"+action+"\t"+sequences[i]+"\t"+
                ack.get(connection)+"\t"+failure+"\t"+traceActions());actions.clear();
        }
    }
    void unloadedAdventure() throws Exception {
        setup("unloaded-adventure",GameType.ADVENTURE,ItemStack.EMPTY,false,100000,new Vec3(8.5,64,8.5));
        for(String predicate:List.of("-","{}","{\"blocks\":\"minecraft:stone\"}")) {
            var stack=new ItemStack(Items.STICK);
            if(!predicate.equals("-")) stack.set(DataComponents.CAN_BREAK,AdventureModePredicate.CODEC.parse(
                RegistryOps.create(JsonOps.INSTANCE,registryAccess()),JsonParser.parseString(predicate)).getOrThrow());
            miner.getInventory().setItem(0,stack);chunks.clear();
            String result;
            try {result=Boolean.toString(miner.blockActionRestricted(this,POS,GameType.ADVENTURE));}
            catch(NullPointerException error) {result=error.getClass().getSimpleName();}
            System.out.println("BLOCK_ACTION\tadventure-unloaded\t"+predicate+"\t"+result+"\t"+chunks.size());
        }
    }
    void audience() throws Exception {
        capture=false;protectionServer=null;actualServer.getPlayerList().getPlayers().clear();chunks.clear();
        var players=new ArrayList<ServerPlayer>();
        var positions=new double[]{0,31.999,32,-32};
        for(int i=0;i<positions.length;i++) players.add(player(i,i==1?GameType.SPECTATOR:GameType.SURVIVAL,new Vec3(8+positions[i],64,8)));
        miner=players.getFirst();capture=true;actions.clear();
        for(int progress:new int[]{-1,0,9,10,255,256}) {
            destroyBlockProgress(miner.getId(),POS,progress);
            System.out.println("BLOCK_ACTION\taudience\t"+progress+"\t"+traceActions());actions.clear();
        }
    }
    static void run() throws Exception {
        var path=Files.createTempDirectory("block-actions-");
        var storage=LevelStorageSource.createDefault(path).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()) {});
        var vectors=new BlockActionsVectors(server,storage,path);vectors.sequences();vectors.audience();vectors.listener();vectors.unloadedAdventure();
        System.out.flush();System.exit(0);
    }
}
class BlockActionsCapture {
    public static void main(String[] args) throws Exception {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        System.setOut(out);System.setErr(err);BlockActionsVectors.run();
    }
}
