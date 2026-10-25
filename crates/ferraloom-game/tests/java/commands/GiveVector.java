import com.google.gson.*;
import com.mojang.authlib.GameProfile;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.item.ItemInput;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.GiveCommand;
import net.minecraft.server.level.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.*;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import static org.mockito.Mockito.*;

/** Invokes native GiveCommand with real inventories and the real drop constructor. */
public final class GiveVector {
    static final class ActualPlayer extends Player {
        final boolean creative;
        ActualPlayer(Level level,int index,boolean creative) {
            super(level,new GameProfile(new UUID(0,index+1),"Player"+index));this.creative=creative;
            getAbilities().instabuild=creative;getRandom().setSeed(42);setPos(0.5,64,0.5);
        }
        @Override public GameType gameMode(){return creative?GameType.CREATIVE:GameType.SURVIVAL;}
    }
    record Spec(String item,int count,int targets,String fill,boolean creative) {}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var registries=VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p->p.apply());
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var give=GiveCommand.class.getDeclaredMethod("giveItem",CommandSourceStack.class,ItemInput.class,Collection.class,int.class);give.setAccessible(true);
        var pickup=ItemEntity.class.getDeclaredField("pickupDelay");pickup.setAccessible(true);
        var target=ItemEntity.class.getDeclaredField("target");target.setAccessible(true);
        var specs=List.of(new Spec("stone",1,1,"empty",false),new Spec("stone",3,1,"empty",false),
            new Spec("stone",65,1,"empty",false),new Spec("stone",128,2,"empty",false),
            new Spec("stone",6400,1,"empty",false),new Spec("stone",6401,1,"empty",false),
            new Spec("stone",3,1,"full",false),new Spec("stone",3,1,"partial",false),
            new Spec("stone",65,1,"one-slot",false),new Spec("stone",3,1,"full",true),
            new Spec("diamond_sword",2,1,"empty",false),new Spec("diamond_sword",101,1,"empty",false),
            new Spec("air",3,1,"empty",false),new Spec("stone[minecraft:custom_name='Named']",3,1,"empty",false));
        var rows=new JsonArray();
        for(var spec:specs) {
            var row=new JsonObject();row.addProperty("item",spec.item);row.addProperty("count",spec.count);row.addProperty("targets",spec.targets);row.addProperty("fill",spec.fill);row.addProperty("creative",spec.creative);
            var calls=new JsonArray();var drops=new JsonArray();var sounds=new JsonArray();var feedback=new JsonArray();
            var source=mock(CommandSourceStack.class);var level=mock(ServerLevel.class);
            doReturn(new net.minecraft.server.ServerScoreboard(mock(net.minecraft.server.MinecraftServer.class))).when(level).getScoreboard();
            doReturn(RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)).when(level).registryAccess();
            doAnswer(call->{var item=(ItemEntity)call.getArgument(0);var value=new JsonObject();
                value.add("stack",ItemStack.CODEC.encodeStart(ops,item.getItem()).getOrThrow());value.addProperty("age",item.getAge());value.addProperty("delay",pickup.getInt(item));
                if(target.get(item)!=null)value.addProperty("target",target.get(item).toString());drops.add(value);calls.add("add");return true;
            }).when(level).addFreshEntity(any());
            doAnswer(call->{var value=new JsonObject();value.addProperty("pitch",Integer.toHexString(Float.floatToRawIntBits(call.getArgument(7))));sounds.add(value);calls.add("sound");return null;})
                .when(level).playSound(isNull(),anyDouble(),anyDouble(),anyDouble(),any(net.minecraft.sounds.SoundEvent.class),any(net.minecraft.sounds.SoundSource.class),anyFloat(),anyFloat());
            doAnswer(call->{var entry=new JsonObject();entry.add("message",ComponentSerialization.CODEC.encodeStart(ops,((Supplier<Component>)call.getArgument(0)).get()).getOrThrow());entry.addProperty("broadcast",(boolean)call.getArgument(1));feedback.add(entry);calls.add("feedback");return null;}).when(source).sendSuccess(any(),anyBoolean());
            var players=new ArrayList<ServerPlayer>();var actuals=new ArrayList<ActualPlayer>();
            for(int i=0;i<spec.targets;i++) {
                var actual=new ActualPlayer(level,i,spec.creative);actuals.add(actual);
                if(!spec.fill.equals("empty")) for(int slot=0;slot<36;slot++)actual.getInventory().setItem(slot,new ItemStack(Items.COBBLESTONE,64));
                if(spec.fill.equals("partial"))actual.getInventory().setItem(0,new ItemStack(Items.STONE,63));
                if(spec.fill.equals("one-slot"))actual.getInventory().setItem(0,ItemStack.EMPTY);
                var player=mock(ServerPlayer.class);player.containerMenu=mock(AbstractContainerMenu.class);
                doReturn(actual.getInventory()).when(player).getInventory();doReturn(level).when(player).level();doReturn(actual.getUUID()).when(player).getUUID();
                doReturn(actual.getRandom()).when(player).getRandom();doReturn(actual.getDisplayName()).when(player).getDisplayName();
                doReturn(actual.getX()).when(player).getX();doReturn(actual.getY()).when(player).getY();doReturn(actual.getZ()).when(player).getZ();
                doAnswer(call->{calls.add("create");return actual.createItemStackToDrop(call.getArgument(0),call.getArgument(1),call.getArgument(2));}).when(player).createItemStackToDrop(any(),anyBoolean(),anyBoolean());
                doAnswer(call->{calls.add("menu");return null;}).when(player.containerMenu).broadcastChanges();players.add(player);
            }
            var context=net.minecraft.commands.CommandBuildContext.simple(registries,net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);
            var input=net.minecraft.commands.arguments.item.ItemArgument.item(context).parse(new com.mojang.brigadier.StringReader(spec.item));
            try {row.addProperty("value",(int)give.invoke(null,source,input,players,spec.count));}
            catch(InvocationTargetException failure) {
                if(failure.getCause() instanceof com.mojang.brigadier.exceptions.CommandSyntaxException error)row.add("error",ComponentSerialization.CODEC.encodeStart(ops,(Component)error.getRawMessage()).getOrThrow());
                else throw failure;
            }
            var inventories=new JsonArray();var randoms=new JsonArray();
            for(var actual:actuals) {
                var contents=new JsonArray();for(int slot=0;slot<36;slot++)if(!actual.getInventory().getItem(slot).isEmpty()) {
                    var entry=new JsonArray();entry.add(slot);entry.add(ItemStack.CODEC.encodeStart(ops,actual.getInventory().getItem(slot)).getOrThrow());contents.add(entry);
                }
                inventories.add(contents);randoms.add(actual.getRandom().nextLong());
            }
            row.add("inventory",inventories);row.add("random",randoms);row.add("drops",drops);row.add("sounds",sounds);row.add("feedback",feedback);row.add("calls",calls);rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows)+"\n");
    }
}
