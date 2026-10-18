import com.google.gson.*;
import com.mojang.brigadier.StringReader;
import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.SharedConstants;
import net.minecraft.commands.arguments.selector.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import com.mojang.authlib.GameProfile;
import net.minecraft.world.item.*;
import static org.mockito.Mockito.*;

/** Executes the native NBT selector predicate, including SelectedItem and storage metadata. */
public final class PlayerNbtVector {
    static final class ActualPlayer extends Player {
        ActualPlayer(Level level) {super(level,new GameProfile(new UUID(0,1),"SelectorPlayer"));}
        @Override public GameType gameMode() {return GameType.SURVIVAL;}
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p->p.apply());
        var level=mock(net.minecraft.server.level.ServerLevel.class);
        doReturn(RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)).when(level).registryAccess();
        var actual=new ActualPlayer(level);
        // ServerPlayer identity enables the native selector's SelectedItem branch;
        // saveWithoutId delegates to a real Player, covering real storage metadata.
        var player=mock(net.minecraft.server.level.ServerPlayer.class);
        doReturn(actual.getInventory()).when(player).getInventory();
        doReturn(level.registryAccess()).when(player).registryAccess();
        doAnswer(call->{actual.saveWithoutId(call.getArgument(0));return null;}).when(player).saveWithoutId(any());
        var field=EntitySelector.class.getDeclaredField("contextFreePredicates");field.setAccessible(true);
        var vectors=new JsonArray();
        for(int count:new int[]{0,1,2}) for(String pattern:List.of("{SelectedItem:{}}","{SelectedItem:{id:'minecraft:stone'}}","{SelectedItem:{count:2}}","{SelectedItem:{id:'minecraft:dirt'}}","{DataVersion:5023}")) for(boolean inverted:new boolean[]{false,true}) {
            player.getInventory().setItem(0,count==0?ItemStack.EMPTY:new ItemStack(Items.STONE,count));
            var selector=new EntitySelectorParser(new StringReader("@a[nbt="+(inverted?"!":"")+pattern+"]"),true).parse();
            var predicates=(List<Predicate<Entity>>)field.get(selector);
            var row=new JsonObject();row.addProperty("count",count);row.addProperty("pattern",pattern);row.addProperty("inverted",inverted);
            row.addProperty("matches",predicates.stream().allMatch(predicate->predicate.test(player)));vectors.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
