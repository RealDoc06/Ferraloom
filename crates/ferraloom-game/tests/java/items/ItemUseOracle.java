import java.lang.instrument.Instrumentation;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import net.bytebuddy.agent.builder.AgentBuilder;
import net.bytebuddy.asm.Advice;
import static net.bytebuddy.matcher.ElementMatchers.named;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.*;

/** Runs real item-use methods on a disposable dedicated ServerLevel. */
public final class ItemUseOracle {
    public static Path output;
    public static final AtomicBoolean ran = new AtomicBoolean();
    public static final List<String> actions = new ArrayList<>();
    public static boolean recording;
    public static void premain(String argument, Instrumentation instrumentation) {
        output = Path.of(argument);
        new AgentBuilder.Default().disableClassFormatChanges()
            .with(AgentBuilder.RedefinitionStrategy.RETRANSFORMATION)
            .type(named("net.minecraft.server.level.ServerLevel"))
            .transform((builder,type,loader,module,domain) -> builder
                .visit(Advice.to(OnTick.class).on(named("tick")))
                .visit(Advice.to(OnEvent.class).on(named("levelEvent")))
                .visit(Advice.to(OnVibration.class).on(named("gameEvent"))))
            .installOn(instrumentation);
    }
    public static class OnTick { @Advice.OnMethodEnter public static void before(@Advice.This Object level) { ItemUseOracle.run(level); } }
    public static class OnEvent { @Advice.OnMethodEnter public static void before(@Advice.Argument(1) int event, @Advice.Argument(3) int data) { ItemUseOracle.recordEvent(event,data); } }
    public static class OnVibration { @Advice.OnMethodEnter public static void before(@Advice.Argument(0) Object event, @Advice.Argument(2) Object context) { ItemUseOracle.recordVibration(event,context); } }
    public static void recordEvent(int event, int data) { if (recording) actions.add("level-event:" + event + ":" + data); }
    public static void recordVibration(Object holder, Object input) {
        if (!recording) return;
        var event = (Holder<?>) holder;
        var context = (GameEvent.Context) input;
        actions.add("event:" + event.unwrapKey().orElseThrow().identifier() + ":" + (context.sourceEntity() != null));
    }
    static void clear(ServerLevel level, BlockPos pos) {
        for (int x=-8;x<=8;x++) for (int z=-8;z<=8;z++) for (int y=-2;y<=12;y++) level.setBlock(pos.offset(x,y,z),Blocks.AIR.defaultBlockState(),816);
    }
    static String snapshot(ServerLevel level, BlockPos pos) {
        var cells = new ArrayList<String>();
        for (int z=-8;z<=8;z++) for (int y=-2;y<=12;y++) for (int x=-8;x<=8;x++) {
            var state=level.getBlockState(pos.offset(x,y,z));
            if (!state.isAir()) cells.add(x+":"+y+":"+z+":"+Block.getId(state));
        }
        return String.join(",",cells);
    }
    public static void run(Object input) {
        if (!ran.compareAndSet(false,true)) return;
        var level = (ServerLevel) input;
        try {
            var lines = new ArrayList<String>();
            var pos = new BlockPos(8,100,8);
            for (var block : List.of(Blocks.WHEAT,Blocks.BEETROOTS,Blocks.OAK_SAPLING)) for (var state : block.getStateDefinition().getPossibleStates()) for (int seed=0;seed<16;seed++) {
                clear(level,pos); level.setBlock(pos.below(),(block==Blocks.OAK_SAPLING?Blocks.DIRT:Blocks.FARMLAND).defaultBlockState(),816); level.setBlock(pos,state,816);
                var stack = new ItemStack(Items.BONE_MEAL,4);
                String before=snapshot(level,pos); level.getRandom().setSeed(seed);
                boolean result=BoneMealItem.growCrop(stack,level,pos);
                lines.add("crop\t"+Block.getId(state)+"\t"+seed+"\t"+result+"\t"+stack.getCount()+"\t"+before+"\t"+snapshot(level,pos)+"\t"+level.getRandom().nextLong());
            }
            for (String name : List.of("plains","warm_ocean")) {
                var biome=level.registryAccess().lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.BIOME,net.minecraft.resources.Identifier.withDefaultNamespace(name)));
                for (int cx=-1;cx<=1;cx++) for (int cz=-1;cz<=1;cz++) level.getChunk(cx,cz).fillBiomesFromNoise((x,y,z)->biome);
                for (int mode=0;mode<4;mode++) for (var face : Direction.values()) for (int seed=0;seed<8;seed++) {
                    clear(level,pos);
                    for (int x=-8;x<=8;x++) for (int z=-8;z<=8;z++) {
                        level.setBlock(pos.offset(x,-1,z),Blocks.STONE.defaultBlockState(),816);
                        if (mode!=1) for (int y=0;y<3;y++) level.setBlock(pos.offset(x,y,z),Blocks.WATER.defaultBlockState(),816);
                        if (mode==3 && (x+z)%3==0) level.setBlock(pos.offset(x,0,z),Blocks.SEAGRASS.defaultBlockState(),816);
                    }
                    if (mode==2) level.setBlock(pos,Blocks.WATER.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.LEVEL,8),816);
                    var stack=new ItemStack(Items.BONE_MEAL,4);
                    String before=snapshot(level,pos); level.getRandom().setSeed(seed);
                    boolean result=BoneMealItem.growWaterPlant(stack,level,pos,face);
                    lines.add("water\t"+name+"\t"+mode+"\t"+face.ordinal()+"\t"+seed+"\t"+result+"\t"+stack.getCount()+"\t"+before+"\t"+snapshot(level,pos)+"\t"+level.getRandom().nextLong());
                }
            }
            var player=new ServerPlayer(level.getServer(),level,new com.mojang.authlib.GameProfile(new java.util.UUID(0,1),"bone-meal"),ClientInformation.createDefault());
            player.snapTo(8.5,100,8.5,0,0);
            for (int count : new int[]{1,2}) {
                clear(level,pos);level.setBlock(pos.below(),Blocks.FARMLAND.defaultBlockState(),816);level.setBlock(pos,Blocks.WHEAT.defaultBlockState(),816);
                var stack=new ItemStack(Items.BONE_MEAL,count);player.setItemInHand(InteractionHand.MAIN_HAND,stack);
                actions.clear();level.getRandom().setSeed(1234);recording=true;
                var result=stack.useOn(new UseOnContext(player,InteractionHand.MAIN_HAND,new BlockHitResult(Vec3.atCenterOf(pos),Direction.UP,pos,false)));
                recording=false;
                lines.add("use\t"+count+"\t"+stack.getCount()+"\t"+((InteractionResult.Success)result).swingSource()+"\t"+Block.getId(level.getBlockState(pos))+"\t"+String.join(";",actions)+"\t"+level.getRandom().nextLong());
            }
            for (boolean creative : new boolean[]{false,true}) {
                clear(level,pos); level.setBlock(pos,Blocks.STONE.defaultBlockState(),816);
                player.getAbilities().instabuild=creative;
                var stack=new ItemStack(Items.POWDER_SNOW_BUCKET);player.setItemInHand(InteractionHand.MAIN_HAND,stack);
                level.getRandom().setSeed(1234);
                var result=player.gameMode.useItemOn(player,level,stack,InteractionHand.MAIN_HAND,new BlockHitResult(Vec3.atCenterOf(pos),Direction.UP,pos,false));
                var hand=player.getMainHandItem();
                var transformed=((InteractionResult.Success)result).heldItemTransformedTo();
                lines.add("solid-bucket\t"+creative+"\t"+net.minecraft.core.registries.BuiltInRegistries.ITEM.getId(stack.getItem())+"\t"+stack.getCount()+"\t"+net.minecraft.core.registries.BuiltInRegistries.ITEM.getId(hand.getItem())+"\t"+hand.getCount()+"\t"+(transformed==stack)+"\t"+(transformed==hand)+"\t"+Block.getId(level.getBlockState(pos.above()))+"\t"+level.getRandom().nextLong());
            }
            Files.write(output,lines);Files.writeString(output.resolveSibling("oracle-done"),"done\n");
        } catch(Throwable error) {
            error.printStackTrace();try{Files.writeString(output.resolveSibling("oracle-error.txt"),error.toString());}catch(Exception ignored){}
        } finally { level.getServer().halt(false); }
    }
}
