import java.util.*;
import com.mojang.serialization.JsonOps;
import com.google.gson.JsonParser;
import net.minecraft.ChatFormatting;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.chat.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;

/** Executes Entity/Player/ItemEntity virtual naming and real team formatting. */
public final class NameVectors {
  static void itemNames(Level level) {
    var ops=level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
    String[] stacks={
      "{}",
      "{\"id\":\"minecraft:compass\"}",
      "{\"id\":\"minecraft:compass\",\"components\":{\"minecraft:lodestone_tracker\":{}}}",
      "{\"id\":\"minecraft:recovery_compass\",\"components\":{\"minecraft:lodestone_tracker\":{}}}",
      "{\"id\":\"minecraft:shield\"}",
      "{\"id\":\"minecraft:shield\",\"components\":{\"minecraft:base_color\":\"red\"}}",
      "{\"id\":\"minecraft:potion\"}",
      "{\"id\":\"minecraft:potion\",\"components\":{\"minecraft:potion_contents\":{\"potion\":\"minecraft:long_swiftness\"}}}",
      "{\"id\":\"minecraft:splash_potion\",\"components\":{\"minecraft:potion_contents\":{\"potion\":\"minecraft:strong_healing\",\"custom_name\":\"brew\"}}}",
      "{\"id\":\"minecraft:lingering_potion\",\"components\":{\"minecraft:potion_contents\":{\"potion\":\"minecraft:water\"}}}",
      "{\"id\":\"minecraft:tipped_arrow\",\"components\":{\"minecraft:potion_contents\":{\"potion\":\"minecraft:strong_poison\"}}}",
      "{\"id\":\"minecraft:player_head\"}",
      "{\"id\":\"minecraft:player_head\",\"components\":{\"minecraft:profile\":{\"name\":\"Doc\"}}}",
      "{\"id\":\"minecraft:stone\",\"components\":{\"minecraft:item_name\":\"overridden\"}}",
      "{\"id\":\"minecraft:potion\",\"components\":{\"!minecraft:potion_contents\":{},\"minecraft:item_name\":\"overridden\"}}"
    };
    for(int i=0;i<stacks.length;i++) {
      var stack=i==0?ItemStack.EMPTY:ItemStack.CODEC.parse(ops,JsonParser.parseString(stacks[i])).getOrThrow();
      System.out.println("item-name\t"+i+"\t"+stacks[i]+"\t"+json(stack.getItemName()));
    }
  }
  static Component actionName() {
    return Component.literal("named").withStyle(s->s.withBold(true).withInsertion("custom").withClickEvent(new ClickEvent.RunCommand("/root")))
      .append(Component.literal("child").withStyle(s->s.withClickEvent(new ClickEvent.SuggestCommand("/child"))));
  }
  static String json(Component value) {return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,value).getOrThrow().toString();}
  public static void run(Level level) {
    itemNames(level);
    for(boolean team:List.of(false,true))for(String scenario:List.of("player","player-custom","mob","mob-custom","item","item-stack-custom","item-custom","item-zero")) {
      Entity entity;
      if(scenario.startsWith("player"))entity=new LivingVectors(level);
      else if(scenario.startsWith("item")) {
        var item=new ItemEntity(EntityTypes.ITEM,level);var stack=new ItemStack(Items.STONE,scenario.equals("item-zero")?0:3);
        if(scenario.equals("item-stack-custom"))stack.set(DataComponents.CUSTOM_NAME,actionName());
        item.setItem(stack);entity=item;
      } else entity=new Cow(EntityTypes.COW,level);
      entity.setId(1);entity.setUUID(new UUID(0,1));
      if(scenario.endsWith("-custom")&&!scenario.equals("item-stack-custom"))entity.setCustomName(actionName());
      var scoreboard=level.getScoreboard();
      if(team) {
        var own=scoreboard.addPlayerTeam("names");own.setPlayerPrefix(Component.literal("<").withStyle(ChatFormatting.BOLD));
        own.setPlayerSuffix(Component.literal(">").withStyle(s->s.withItalic(false)));own.setColor(Optional.of(net.minecraft.world.scores.TeamColor.RED));
        scoreboard.addPlayerToTeam(entity.getScoreboardName(),own);
      }
      System.out.println("entity-name\t"+scenario+"\t"+team+"\t"+entity.getScoreboardName()+"\t"+json(entity.getName())+"\t"+json(entity.getDisplayName()));
      if(team)scoreboard.removePlayerTeam(scoreboard.getPlayerTeam("names"));
    }
  }
}
