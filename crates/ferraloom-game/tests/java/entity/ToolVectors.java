import java.util.*;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.tags.BlockTags;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;

/** Real selected-tool drop predicates, component patches and ordered TOOL rules. */
public final class ToolVectors {
  record Case(String name,ItemStack stack) {}
  static HolderSet<Block> direct(Block... blocks) {return HolderSet.direct(Arrays.stream(blocks).map(Block::builtInRegistryHolder).toList());}
  static Tool.Rule rule(HolderSet<Block> blocks,Boolean correct) {return new Tool.Rule(blocks,Optional.of(2f),Optional.ofNullable(correct));}
  static ItemStack custom(Item item,Tool.Rule... rules) {
    var stack=new ItemStack(item);stack.set(DataComponents.TOOL,new Tool(List.of(rules),1f,1,true));return stack;
  }
  public static void run(Level level) {
    var pick=BuiltInRegistries.BLOCK.getOrThrow(BlockTags.MINEABLE_WITH_PICKAXE);
    var empty=BuiltInRegistries.BLOCK.getOrThrow(BlockTags.INCORRECT_FOR_NETHERITE_TOOL);
    if(empty.size()!=0)throw new AssertionError("controlled empty tag");
    var removed=new ItemStack(Items.WOODEN_PICKAXE);removed.remove(DataComponents.TOOL);
    var zero=new ItemStack(Items.IRON_PICKAXE);zero.setCount(0);
    var cases=List.of(
      new Case("empty",ItemStack.EMPTY),new Case("stick",new ItemStack(Items.STICK)),
      new Case("wooden",new ItemStack(Items.WOODEN_PICKAXE)),new Case("iron",new ItemStack(Items.IRON_PICKAXE)),
      new Case("shears",new ItemStack(Items.SHEARS)),new Case("removed",removed),new Case("zero",zero),
      new Case("speed_and_deny",custom(Items.STICK,rule(direct(Blocks.STONE),null),rule(direct(Blocks.DIAMOND_ORE),false),rule(pick,true))),
      new Case("deny_first",custom(Items.STICK,rule(direct(Blocks.STONE),false),rule(direct(Blocks.STONE),true))),
      new Case("allow_first",custom(Items.STICK,rule(direct(Blocks.STONE),true),rule(direct(Blocks.STONE),false))),
      new Case("direct_empty",custom(Items.STICK,rule(direct(),true))),
      new Case("miss_then_allow",custom(Items.STICK,rule(direct(Blocks.DIRT),false),rule(direct(Blocks.STONE),true))),
      new Case("speed_only_tag",custom(Items.STICK,rule(pick,null),rule(direct(Blocks.STONE),true))),
      new Case("air_payload",custom(Items.AIR,rule(direct(Blocks.STONE),true))),
      new Case("named_empty",custom(Items.STICK,rule(empty,true))),
      new Case("netherite",new ItemStack(Items.NETHERITE_PICKAXE))
    );
    var states=List.of(Blocks.STONE,Blocks.DIRT,Blocks.DIAMOND_ORE,Blocks.OAK_LOG,Blocks.COBWEB,Blocks.OAK_LEAVES,Blocks.WOOL.pick(DyeColor.WHITE),Blocks.IRON_BLOCK);
    var keys=new TreeSet<Identifier>();
    for(var c:cases) {var tool=c.stack.get(DataComponents.TOOL);if(tool!=null)for(var rule:tool.rules())rule.blocks().unwrapKey().ifPresent(key->keys.add(key.location()));}
    // Retain present empty tags and only the controlled blocks used by this oracle.
    for(var key:keys) {
      var tag=BuiltInRegistries.BLOCK.getOrThrow(net.minecraft.tags.TagKey.create(Registries.BLOCK,key));
      var members=states.stream().filter(block->block.defaultBlockState().is(tag)).map(block->BuiltInRegistries.BLOCK.getKey(block).toString()).toList();
      System.out.println("tool-tag\t"+key+"\t"+(members.isEmpty()?"-":String.join(",",members)));
    }
    var player=new LivingVectors(level);
    for(var c:cases) {
      player.setItemInHand(InteractionHand.MAIN_HAND,c.stack);
      for(var block:states) {
        var state=block.defaultBlockState();
        System.out.println("tool-drops\t"+c.name+"\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+c.stack.isCorrectToolForDrops(state)+","+player.hasCorrectToolForDrops(state)+","+state.requiresCorrectToolForDrops());
      }
    }
    var registry=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var json=JsonParser.parseString("{\"rules\":[{\"blocks\":\"#entity_probe:missing\",\"correct_for_drops\":true}]}");
    System.out.println("tool-missing-tag\t"+Tool.CODEC.parse(RegistryOps.create(JsonOps.INSTANCE,registry),json).isError());
  }
}
