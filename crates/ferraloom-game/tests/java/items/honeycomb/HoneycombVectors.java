import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.sounds.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.*;

/** Actual HoneycombItem table, useOn and SignApplicator callbacks on a concrete Level. */
public final class HoneycombVectors {
    static final BlockPos POS=BlockItemVectors.POS;
    static final class World extends BlockItemVectors.Fixture {
        ItemStack stack;
        World(RegistryAccess registries){super(registries);}
        String at(BlockPos pos){return pos.getX()+","+pos.getY()+","+pos.getZ();}
        @Override public void gameEvent(Holder event,Vec3 at,GameEvent.Context context){actions.add("event:"+((net.minecraft.resources.ResourceKey<?>)event.unwrapKey().orElseThrow()).identifier()+":"+at(BlockPos.containing(at))+":"+Block.getId(context.affectedState())+":"+stack.getCount());}
        @Override public void levelEvent(Entity source,int type,BlockPos pos,int data){actions.add("level:"+type+":"+at(pos)+":"+data+":"+stack.getCount());}
        @Override public void playSeededSound(Entity except,double x,double y,double z,Holder sound,SoundSource source,float volume,float pitch,long seed){actions.add("sound:"+((SoundEvent)sound.value()).location()+":"+x+","+y+","+z+":"+stack.getCount());}
    }
    static final class Context extends UseOnContext {
        Context(World level,ItemStack stack){super(level,null,InteractionHand.OFF_HAND,stack,new BlockHitResult(Vec3.atCenterOf(POS),Direction.UP,POS,false));}
    }
    public static void main(String[] args)throws Exception {
        var stdout=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(stdout);
        var world=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var pairs=new JsonArray();
        for(var entry:HoneycombItem.WAXABLES.get().entrySet()){
            var pair=new JsonObject();pair.addProperty("unwaxed",BuiltInRegistries.BLOCK.getId(entry.getKey()));pair.addProperty("waxed",BuiltInRegistries.BLOCK.getId(entry.getValue()));pairs.add(pair);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(pairs)+"\n");
        var lines=new ArrayList<String>();
        for(var state:Block.BLOCK_STATE_REGISTRY){
            var waxed=HoneycombItem.getWaxed(state);
            var unwaxed=Optional.ofNullable(HoneycombItem.WAX_OFF_BY_BLOCK.get().get(state.getBlock())).map(block->block.withPropertiesOf(state));
            lines.add("state\t"+Block.getId(state)+"\t"+waxed.map(Block::getId).orElse(-1)+"\t"+unwaxed.map(Block::getId).orElse(-1));
        }
        for(var block:BuiltInRegistries.BLOCK){
            if(!HoneycombItem.WAXABLES.get().containsKey(block) && !List.of(Blocks.STONE,BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("waxed_copper_block")),Blocks.OAK_SIGN).contains(block))continue;
            for(var state:block.getStateDefinition().getPossibleStates()){
                // Exhaustive property mapping above; behavior fixtures cover each
                // constructor family and its default state, plus all chest halves.
                if(state!=block.defaultBlockState() && !(block instanceof CopperChestBlock))continue;
                for(boolean separate:List.of(false,true)){
                    var level=new World(registries);level.put(POS.below(),Blocks.STONE.defaultBlockState());level.put(POS,state);
                    BlockPos neighbor=POS.east();
                    if(state.getBlock() instanceof ChestBlock && state.getValue(ChestBlock.TYPE)!=ChestType.SINGLE){neighbor=ChestBlock.getConnectedBlockPos(POS,state);level.put(neighbor.below(),Blocks.STONE.defaultBlockState());level.put(neighbor,state.setValue(ChestBlock.TYPE,state.getValue(ChestBlock.TYPE)==ChestType.LEFT?ChestType.RIGHT:ChestType.LEFT));}
                    var receiver=new ItemStack(Items.HONEYCOMB,3);var held=separate?new ItemStack(Items.DIRT,7):receiver;level.stack=held;level.actions.clear();level.getRandom().setSeed(1234);
                    var result=receiver.useOn(new Context(level,held));
                    lines.add("use\t"+Block.getId(state)+"\t"+separate+"\t"+(result instanceof InteractionResult.Success?"success":"pass")+"\t"+receiver.getCount()+"\t"+held.getCount()+"\t"+Block.getId(level.getBlockState(POS))+"\t"+level.at(neighbor)+"\t"+Block.getId(level.getBlockState(neighbor))+"\t"+String.join(";",level.actions)+"\t"+level.getRandom().nextLong());
                }
            }
        }
        var item=(HoneycombItem)Items.HONEYCOMB;
        for(var block:List.of(Blocks.OAK_SIGN,Blocks.OAK_WALL_SIGN,Blocks.OAK_HANGING_SIGN,Blocks.OAK_WALL_HANGING_SIGN))for(boolean waxed:List.of(false,true)){
            var level=new World(registries);level.put(POS,block.defaultBlockState());var sign=(SignBlockEntity)level.getBlockEntity(POS);sign.setWaxed(waxed);
            level.stack=new ItemStack(Items.HONEYCOMB,3);level.actions.clear();level.getRandom().setSeed(1234);
            var result=item.tryApplyToSign(level,sign,SignTextSlot.FRONT,level.stack,null);
            lines.add("sign\t"+Block.getId(block.defaultBlockState())+"\t"+waxed+"\t"+result+"\t"+sign.isWaxed()+"\t"+level.stack.getCount()+"\t"+String.join(";",level.actions)+"\t"+level.getRandom().nextLong());
        }
        Files.writeString(Path.of(args[1]),String.join("\n",lines)+"\n");System.out.println(pairs.size()+" native wax pairs; "+lines.size()+" honeycomb vectors");
    }
}
