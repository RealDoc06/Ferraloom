import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.sounds.*;
import net.minecraft.stats.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Real ServerLevel/ServerPlayer destruction, loot and mining calls. Only the
 * transport, owned chunks and a removal callback are controlled by the fixture. */
public final class PlayerDestroyVectors extends DestroyBlockVectors {
    final GameTestServer server;
    int nextPlayer;
    Runnable removalCallback;
    PlayerDestroyVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) {
        super(server,storage); this.server=server;
    }
    @Override public ChunkAccess getChunk(int x,int z,ChunkStatus status,boolean load) {
        var pos=new ChunkPos(x,z);
        if(!load) return chunks.get(pos);
        return chunks.computeIfAbsent(pos,p->{
            var chunk=new LevelChunk(this,p) {
                @Override public FullChunkStatus getFullStatus() {return FullChunkStatus.BLOCK_TICKING;}
            };
            chunk.registerTickContainerInLevel(this);
            return chunk;
        });
    }
    @Override public boolean removeBlock(BlockPos pos,boolean moved) {
        if(removalCallback!=null && pos.equals(POS)) {
            var callback=removalCallback; removalCallback=null; callback.run();
        }
        return super.removeBlock(pos,moved);
    }
    @Override public void playSeededSound(Entity except,double x,double y,double z,
        Holder<SoundEvent> sound,SoundSource source,float volume,float pitch,long seed) {
        if(recording) trace.add("sound:"+BuiltInRegistries.SOUND_EVENT.getKey(sound.value())+":"+
            MiningVectors.bits(volume)+":"+MiningVectors.bits(pitch));
        super.playSeededSound(except,x,y,z,sound,source,volume,pitch,seed);
    }
    @Override public boolean addFreshEntity(Entity entity) {
        boolean result=super.addFreshEntity(entity);
        if(recording && entity instanceof ItemEntity item) {
            var name=item.getItem().get(DataComponents.CUSTOM_NAME);
            if(name!=null) trace.add("name:"+name.getString());
        }
        return result;
    }
    static <T> String statKey(Stat<T> stat) {return stat.getType().getRegistry().getKey(stat.getValue()).toString();}
    static void name(BlockEntity entity,String name) {
        var stack=new ItemStack(Items.CHEST);stack.set(DataComponents.CUSTOM_NAME,Component.literal(name));
        entity.applyComponentsFromItemStack(stack);
    }
    final class Miner extends ServerPlayer {
        Miner() {
            super(server,PlayerDestroyVectors.this,new GameProfile(new UUID(0,++nextPlayer),"destroy"+nextPlayer),
                ClientInformation.createDefault());
        }
        @Override public void awardStat(Stat<?> stat,int amount) {
            if(recording) trace.add("stat:"+statKey(stat)+":"+amount);
            super.awardStat(stat,amount);
        }
        @Override public void causeFoodExhaustion(float amount) {
            if(recording) trace.add("food:"+MiningVectors.bits(amount));
            super.causeFoodExhaustion(amount);
        }
    }
    Miner reset(long seed,boolean drops) throws Exception {
        recording=false; removalCallback=null;
        for(var chunk:chunks.values()) chunk.unregisterTickContainerFromLevel(this);
        chunks.clear(); trace.clear();
        getGameRules().set(GameRules.BLOCK_DROPS,drops,null);
        getWorldBorder().setSize(5.9999968E7);
        for(int x=-1;x<=1;x++) for(int z=-1;z<=1;z++) getChunk(x,z,ChunkStatus.FULL,true);
        var miner=new Miner();
        new net.minecraft.server.network.ServerGamePacketListenerImpl(server,
            new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND),miner,
            net.minecraft.server.network.CommonListenerCookie.createInitial(miner.getGameProfile(),false)) {
            @Override public void send(net.minecraft.network.protocol.Packet<?> packet) {}
        };
        miner.setPos(8.5,64,8.5); miner.setOnGround(true);
        miner.gameMode.changeGameModeForPlayer(GameType.SURVIVAL);
        random.setSeed(seed);
        var sound=Level.class.getDeclaredField("soundSeedGenerator");sound.setAccessible(true);
        ((RandomSource)sound.get(this)).setSeed(0);
        return miner;
    }
    void begin(long seed) {trace.clear();random.setSeed(seed);recording=true;}
    void output(String input,ServerPlayer miner) throws Exception {
        recording=false;
        var food=miner.getFoodData().getClass().getDeclaredField("exhaustionLevel");food.setAccessible(true);
        System.out.println("PLAYER_DESTROY\t"+input+"\t"+Block.getId(getBlockState(POS))+"\t"+
            random.nextInt()+"\t"+MiningVectors.bits(food.getFloat(miner.getFoodData()))+"\t"+
            (trace.isEmpty()?"-":String.join(";",trace)));
    }
    void base() throws Exception {
        for(long seed:new long[]{0,1,9123456}) for(boolean drops:List.of(false,true))
            for(var block:List.of(Blocks.OAK_PLANKS,Blocks.CHEST,Blocks.AIR)) {
                var miner=reset(seed,drops); var state=block.defaultBlockState();
                BlockEntity original=null;
                if(block==Blocks.CHEST) {
                    getChunkAt(POS).setBlockState(POS,state,Block.UPDATE_KNOWN_SHAPE);
                    original=getBlockEntity(POS);
                    name(original,"original");
                    getChunkAt(POS).setBlockState(POS,Blocks.BARREL.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
                    name(getBlockEntity(POS),"replacement");
                }
                begin(seed);
                Blocks.STONE.playerDestroy(this,miner,POS,state,original,ItemStack.EMPTY);
                output("base\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+drops+"\t"+seed,miner);
            }
    }
    static ItemStack tool(Item item,int damage) {
        var stack=new ItemStack(item);stack.set(DataComponents.TOOL,new Tool(List.of(),1,damage,true));return stack;
    }
    void mining() throws Exception {
        for(var item:List.of(Items.STICK,Items.SHEARS)) for(var block:List.of(Blocks.STONE,Blocks.SHORT_GRASS,Blocks.FIRE)) {
            var miner=reset(0,true);var stack=tool(item,0);miner.getInventory().setItem(0,stack);begin(0);
            stack.mineBlock(this,block.defaultBlockState(),POS,miner);
            output("mine\t"+BuiltInRegistries.ITEM.getKey(item)+"\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t0",miner);
        }
        var miner=reset(0,true);begin(0);new ItemStack(Items.STICK).mineBlock(this,Blocks.STONE.defaultBlockState(),POS,miner);
        output("mine\tminecraft:stick\tminecraft:stone\t-",miner);
    }
    void callback() throws Exception {
        for(boolean replace:List.of(false,true)) {
            var miner=reset(0,true);
            getChunkAt(POS).setBlockState(POS,Blocks.CHEST.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
            name(getBlockEntity(POS),"original");
            miner.getInventory().setItem(0,tool(Items.STICK,0));
            removalCallback=()->{
                trace.add("callback");miner.getInventory().setItem(0,tool(Items.BLAZE_ROD,0));
                if(replace) {
                    getChunkAt(POS).setBlockState(POS,Blocks.BARREL.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
                    name(getBlockEntity(POS),"replacement");
                }
            };
            begin(0); boolean changed=miner.gameMode.destroyBlock(POS);
            if(!changed) throw new AssertionError("native destroy result");
            output("callback\t"+replace,miner);
        }
    }
    void eggs() throws Exception {
        for(long seed:new long[]{0,1}) for(int eggs:new int[]{1,2,4}) for(boolean sand:List.of(false,true)) {
            var miner=reset(seed,true);
            getChunkAt(POS).setBlockState(POS.below(),(sand?Blocks.SAND:Blocks.STONE).defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
            var state=Blocks.TURTLE_EGG.defaultBlockState().setValue(BlockStateProperties.EGGS,eggs);
            getChunkAt(POS).setBlockState(POS,state,Block.UPDATE_KNOWN_SHAPE);
            begin(seed);miner.gameMode.destroyBlock(POS);
            output("egg\t"+eggs+"\t"+sand+"\t"+seed,miner);
        }
    }
    void dragon() throws Exception {
        for(long seed:new long[]{0,1,9123456}) for(String support:List.of("air","floor","border")) {
            var miner=reset(seed,true);
            if(!support.equals("air")) for(int x=-7;x<=23;x++) for(int z=-7;z<=23;z++)
                getChunkAt(new BlockPos(x,63,z)).setBlockState(new BlockPos(x,63,z),Blocks.STONE.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
            if(support.equals("border")) getWorldBorder().setSize(1);
            var state=Blocks.DRAGON_EGG.defaultBlockState();
            getChunkAt(POS).setBlockState(POS,state,Block.UPDATE_KNOWN_SHAPE);
            begin(seed);state.attack(this,POS,miner);output("dragon\t"+support+"\t"+seed,miner);
            var eggs=new ArrayList<String>();
            for(var chunk:chunks.values()) for(int x=0;x<16;x++) for(int z=0;z<16;z++) for(int y=57;y<=71;y++) {
                var pos=new BlockPos(chunk.getPos().getMinBlockX()+x,y,chunk.getPos().getMinBlockZ()+z);
                if(chunk.getBlockState(pos).is(Blocks.DRAGON_EGG)) eggs.add(pos.getX()+","+pos.getY()+","+pos.getZ());
            }
            Collections.sort(eggs);
            System.out.println("PLAYER_DESTROY\tdragon-position\t"+support+"\t"+seed+"\t"+String.join(";",eggs));
        }
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory("player-destroy-")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,
            ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()) {});
        var vectors=new PlayerDestroyVectors(server,storage);
        vectors.base();vectors.mining();vectors.callback();vectors.eggs();vectors.dragon();
        System.out.flush();System.exit(0);
    }
}
class PlayerDestroyCapture {
    public static void main(String[] args) throws Exception {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        System.setOut(out);System.setErr(err);PlayerDestroyVectors.run();
    }
}
