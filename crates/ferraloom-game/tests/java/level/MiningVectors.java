import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import com.google.gson.JsonParser;
import com.mojang.authlib.GameProfile;
import com.mojang.serialization.JsonOps;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.tags.*;
import net.minecraft.world.effect.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Actual Player/BlockState mining calls and ServerPlayerGameMode#tick. Reflection
 * controls only the native tick inputs and invokes Entity's protected fluid scan.
 * The existing real ServerLevel fixture supplies owned chunks, not mining logic. */
public final class MiningVectors {
    static final BlockPos POS = DestroyBlockVectors.POS;
    static final Method FLUID_UPDATE;
    static {
        try {
            FLUID_UPDATE=Entity.class.getDeclaredMethod("updateFluidInteraction");
            FLUID_UPDATE.setAccessible(true);
        } catch (ReflectiveOperationException error) { throw new ExceptionInInitializerError(error); }
    }
    final GameTestServer server;
    final DestroyBlockVectors level;
    int nextPlayer;
    MiningVectors(GameTestServer server, DestroyBlockVectors level) {this.server=server;this.level=level;}
    static String bits(float value) {return Integer.toUnsignedString(Float.floatToRawIntBits(value),16);}
    static String bits(double value) {return Long.toUnsignedString(Double.doubleToRawLongBits(value),16);}
    static String attributes(ServerPlayer player) {
        // Attribute-key traversal is JVM identity-hash order, not a parity order.
        var packed=new ArrayList<>(player.getAttributes().pack());
        packed.sort(Comparator.comparing(value->BuiltInRegistries.ATTRIBUTE.getKey(value.attribute().value()).toString()));
        return AttributeInstance.Packed.CODEC.listOf().encodeStart(
            RegistryOps.create(NbtOps.INSTANCE,player.registryAccess()),packed).getOrThrow().toString();
    }
    ServerPlayer player(boolean water, boolean ground) throws Exception {
        level.chunks.clear();
        level.getChunkAt(POS);
        if(water) for(int y=64;y<=67;y++)
            level.getChunkAt(POS).setBlockState(new BlockPos(8,y,8),Blocks.WATER.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
        var profile=new GameProfile(new UUID(0,++nextPlayer),"mining"+nextPlayer);
        var player=new ServerPlayer(server,level,profile,ClientInformation.createDefault());
        new net.minecraft.server.network.ServerGamePacketListenerImpl(server,
            new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND),player,
            net.minecraft.server.network.CommonListenerCookie.createInitial(profile,false)) {
            @Override public void send(net.minecraft.network.protocol.Packet<?> packet) {}
        };
        player.setPos(8.5,64,8.5);
        player.setOnGround(ground);
        FLUID_UPDATE.invoke(player);
        if(player.isEyeInFluid(FluidTags.WATER)!=water) throw new AssertionError("actual eye fluid");
        return player;
    }
    void scenario(String name, Block block, ItemStack stack, double efficiency, double breakSpeed,
        Double submerged, int haste, int conduit, int fatigue, boolean water, boolean ground) throws Exception {
        var player=player(water,ground);
        player.getInventory().setItem(0,stack);
        player.getAttribute(Attributes.MINING_EFFICIENCY).setBaseValue(efficiency);
        player.getAttribute(Attributes.BLOCK_BREAK_SPEED).setBaseValue(breakSpeed);
        if(submerged!=null) player.getAttribute(Attributes.SUBMERGED_MINING_SPEED).setBaseValue(submerged);
        for(var entry:List.of(Map.entry(MobEffects.HASTE,haste),Map.entry(MobEffects.CONDUIT_POWER,conduit),Map.entry(MobEffects.MINING_FATIGUE,fatigue)))
            if(entry.getValue()>=0) player.addEffect(new MobEffectInstance(entry.getKey(),200,entry.getValue(),false,false,false));
        var state=block.defaultBlockState();
        String input=stack.isEmpty()?"-":ItemStack.CODEC.encodeStart(
            RegistryOps.create(NbtOps.INSTANCE,player.registryAccess()),stack).getOrThrow().toString();
        System.out.println("MINING_VECTOR\tmining\t"+name+"\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+input+"\t"+
            bits(efficiency)+"\t"+bits(breakSpeed)+"\t"+(submerged==null?"-":bits(submerged))+"\t"+
            haste+"\t"+conduit+"\t"+fatigue+"\t"+water+"\t"+ground+"\t"+
            bits(stack.getDestroySpeed(state))+"\t"+bits(player.getDestroySpeed(state))+"\t"+
            bits(state.getDestroyProgress(player,level,POS))+"\t"+player.hasCorrectToolForDrops(state));
    }
    static Tool.Rule rule(HolderSet<Block> blocks, Float speed, Boolean correct) {
        return new Tool.Rule(blocks,Optional.ofNullable(speed),Optional.ofNullable(correct));
    }
    static ItemStack custom(float fallback, Tool.Rule... rules) {
        var stack=new ItemStack(Items.STICK);
        stack.set(DataComponents.TOOL,new Tool(List.of(rules),fallback,1,true));
        return stack;
    }
    void numeric() throws Exception {
        for(var block:List.of(Blocks.STONE,Blocks.DIRT,Blocks.OBSIDIAN,Blocks.BEDROCK,Blocks.SHORT_GRASS,Blocks.COBWEB))
            for(var item:List.of(Items.AIR,Items.WOODEN_PICKAXE,Items.DIAMOND_PICKAXE,Items.SHEARS))
                scenario("base",block,new ItemStack(item),0,1,null,-1,-1,-1,false,true);
        for(float speed:new float[]{Math.nextDown(1f),1f,Math.nextUp(1f),8f,Float.MAX_VALUE})
            for(boolean ground:List.of(false,true)) for(boolean water:List.of(false,true))
                scenario("rounding",Blocks.STONE,custom(speed),2.0000001192092896,1.1000000596046448,
                    0.30000001192092896,2,4,1,water,ground);
        for(int fatigue=0;fatigue<256;fatigue++)
            scenario("fatigue",Blocks.STONE,new ItemStack(Items.DIAMOND_PICKAXE),0,1,null,-1,-1,fatigue,false,true);
        for(int[] boosts:new int[][]{{-1,-1},{0,-1},{-1,0},{3,1},{1,3},{255,-1}})
            scenario("boost",Blocks.DIRT,new ItemStack(Items.DIAMOND_SHOVEL),1.00000001,0.3333333333333333,
                null,boosts[0],boosts[1],-1,true,false);
        var stone=HolderSet.direct(Blocks.STONE.builtInRegistryHolder());
        var empty=BuiltInRegistries.BLOCK.getOrThrow(BlockTags.INCORRECT_FOR_NETHERITE_TOOL);
        scenario("rules-order",Blocks.STONE,custom(1.25f,rule(stone,null,false),rule(stone,7f,true),rule(stone,15f,true)),
            0,1,null,-1,-1,-1,false,true);
        scenario("named-empty",Blocks.STONE,custom(1.25f,rule(empty,42f,true)),0,1,null,-1,-1,-1,false,true);
        var missing=JsonParser.parseString("{\"rules\":[{\"blocks\":\"#mining_probe:missing\",\"speed\":2.0}]}");
        System.out.println("MINING_VECTOR\tmissing-tag\t"+Tool.CODEC.parse(
            RegistryOps.create(JsonOps.INSTANCE,server.registryAccess()),missing).isError());
        System.out.println("MINING_VECTOR\thardness-null\t"+bits(Blocks.BEDROCK.defaultBlockState().getDestroyProgress(null,level,POS)));
    }
    static void field(ServerPlayerGameMode mode, String name, Object value) throws Exception {
        var field=ServerPlayerGameMode.class.getDeclaredField(name);field.setAccessible(true);field.set(mode,value);
    }
    static Object field(ServerPlayerGameMode mode, String name) throws Exception {
        var field=ServerPlayerGameMode.class.getDeclaredField(name);field.setAccessible(true);return field.get(mode);
    }
    void ticks() throws Exception {
        for(boolean delayed:List.of(false,true)) for(int ticks:new int[]{0,20,100,Integer.MAX_VALUE}) {
            var player=player(false,true);
            level.getChunkAt(POS).setBlockState(POS,Blocks.STONE.defaultBlockState(),Block.UPDATE_KNOWN_SHAPE);
            var mode=player.gameMode;
            field(mode,"gameTicks",ticks);field(mode,"destroyProgressStart",0);
            field(mode,"isDestroyingBlock",true);field(mode,"hasDelayedDestroy",delayed);
            field(mode,"destroyPos",POS);field(mode,"delayedDestroyPos",POS);field(mode,"delayedTickStart",2);
            mode.tick();
            System.out.println("MINING_VECTOR\ttick\t"+delayed+"\t"+ticks+"\t0\t2\t"+
                field(mode,"gameTicks")+"\t"+field(mode,"lastSentState")+"\t"+field(mode,"hasDelayedDestroy"));
        }
        var increment=ServerPlayerGameMode.class.getDeclaredMethod("incrementDestroyProgress",BlockState.class,BlockPos.class,int.class);
        increment.setAccessible(true);
        var player=player(false,true);var state=Blocks.STONE.defaultBlockState();
        float perTick=state.getDestroyProgress(player,level,POS);
        for(int ticks:new int[]{-2,-1,0,1,103,104,149,150,Integer.MAX_VALUE,Integer.MIN_VALUE}) {
            float total=(float)increment.invoke(player.gameMode,state,POS,ticks);
            System.out.println("MINING_VECTOR\tprogress\t"+bits(perTick)+"\t"+ticks+"\t"+bits(total)+"\t"+
                field(player.gameMode,"lastSentState"));
        }
    }
    void lazyAttribute() throws Exception {
        var player=player(true,true);
        player.getInventory().setItem(0,new ItemStack(Items.DIAMOND_PICKAXE));
        String before=attributes(player);
        float speed=player.getDestroySpeed(Blocks.STONE.defaultBlockState());
        String after=attributes(player);
        System.out.println("MINING_VECTOR\tlazy-attribute\t"+before+"\t"+after+"\t"+bits(speed));
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory("mining-vectors-")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,
            ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()) {});
        var vectors=new MiningVectors(server,new DestroyBlockVectors(server,storage));
        vectors.numeric();vectors.ticks();vectors.lazyAttribute();
        System.out.flush();System.exit(0);
    }
}
class MiningCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        System.setOut(output);System.setErr(errors);MiningVectors.run();
    }
}
