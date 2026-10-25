import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.*;
import net.minecraft.server.waypoints.ServerWaypointManager;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.dimension.BuiltinDimensionTypes;
import static org.mockito.Mockito.*;

/** Executes native refreshDirtyAttributes, including the ServerPlayer observer. */
public final class AttributeUpdateVectors {
    static final class Parent extends Player {
        Parent(Level level) { super(level,new GameProfile(new UUID(1,2),"attribute")); }
        @Override public GameType gameMode() { return GameType.SURVIVAL; }
    }
    static Field field(Class<?> owner,String name) throws Exception {
        var field=owner.getDeclaredField(name);field.setAccessible(true);return field;
    }
    static String bits(float value) { return Integer.toHexString(Float.floatToRawIntBits(value)); }
    static final class Receiver extends ServerPlayer {
        ArrayList<String> events;
        GameType mode;
        Receiver() { super(null,null,null,null); }
        @Override public GameType gameMode() { return mode; }
        @Override public void refreshDimensions() { events.add("scale"); }
        @Override protected void onAttributeUpdated(Holder<Attribute> attribute) {
            events.add(attribute.unwrapKey().orElseThrow().identifier().getPath());
            if(playerDirty(this)==0)throw new AssertionError("dirty set cleared before callback");
            super.onAttributeUpdated(attribute);
        }
    }
    static Receiver player(ServerLevel level,ArrayList<String> events) throws Exception {
        var unsafe=field(sun.misc.Unsafe.class,"theUnsafe");
        var allocator=(sun.misc.Unsafe)unsafe.get(null);
        var player=(Receiver)allocator.allocateInstance(Receiver.class);
        player.events=events;player.mode=GameType.SURVIVAL;
        var parent=new Parent(level);
        for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass()) {
            for(var field:owner.getDeclaredFields()) if(!Modifier.isStatic(field.getModifiers())) {
                field.setAccessible(true);field.set(player,field.get(parent));
            }
        }
        field(LivingEntity.class,"attributes").set(player,new AttributeMap(DefaultAttributes.getSupplier(EntityTypes.PLAYER)));
        return player;
    }
    static int playerDirty(Object player) { return ((ServerPlayer)player).getAttributes().getAttributesToUpdate().size(); }
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        var level=mock(ServerLevel.class);
        doReturn(AdvancementVectors.lookup).when(level).registryAccess();
        doReturn(Level.OVERWORLD).when(level).dimension();
        doReturn(new WorldBorder()).when(level).getWorldBorder();
        doReturn(AdvancementVectors.lookup.lookupOrThrow(net.minecraft.core.registries.Registries.DIMENSION_TYPE)
            .getOrThrow(BuiltinDimensionTypes.OVERWORLD).value()).when(level).dimensionType();
        var events=new ArrayList<String>();
        var manager=mock(ServerWaypointManager.class,call->{
            events.add(call.getMethod().getName());return null;
        });
        doReturn(manager).when(level).getWaypointManager();
        var refresh=LivingEntity.class.getDeclaredMethod("refreshDirtyAttributes");refresh.setAccessible(true);
        var rows=new ArrayList<String>();
        for(String scenario:List.of("health-down","health-up","absorption-down","absorption-up","scale",
            "receive-on","receive-off","transmit-on","transmit-off","other","all")) {
            var player=player(level,events);
            player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(40);player.setHealth(35);
            player.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(20);player.setAbsorptionAmount(15);
            player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(0);
            player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(0);
            player.getAttributes().getAttributesToUpdate().clear();
            switch(scenario) {
                case "health-down" -> player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(12);
                case "health-up" -> player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(50);
                case "absorption-down" -> player.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(6);
                case "absorption-up" -> player.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(30);
                case "scale" -> player.getAttribute(Attributes.SCALE).setBaseValue(2);
                case "receive-on" -> player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(128);
                case "receive-off" -> {player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(128);player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(0);}
                case "transmit-on" -> player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(128);
                case "transmit-off" -> {player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(128);player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(0);}
                case "other" -> player.getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.25);
                case "all" -> {
                    player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(12);
                    player.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(6);
                    player.getAttribute(Attributes.SCALE).setBaseValue(2);
                    player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(128);
                    player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(128);
                }
            }
            // Identity hashes vary between JVMs. Record the actual dirty-set
            // encounter order as an input, alongside the native dispatch trace.
            var order=new ArrayList<String>();
            for(var instance:player.getAttributes().getAttributesToUpdate())
                order.add(instance.getAttribute().unwrapKey().orElseThrow().identifier().getPath());
            events.clear();refresh.invoke(player);
            if(scenario.equals("all")) {
                var visited=events.stream().filter(order::contains).filter(value->!value.equals("scale")).toList();
                // SCALE records both the dispatcher and refreshDimensions.
                var expected=order.stream().filter(value->!value.equals("scale")).toList();
                if(!visited.equals(expected))throw new AssertionError(order+" != "+events);
                rows.add("refresh-order\t"+order.size()+"\ttrue\t"+playerDirty(player));
            } else {
                rows.add("refresh\t"+scenario+"\t"+String.join(",",order)+"\t"+String.join(",",events)+"\t"+
                    bits(player.getHealth())+"\t"+bits(player.getAbsorptionAmount())+"\t"+playerDirty(player));
            }
        }
        for(float amount:new float[]{-1,-0.0f,0,3.25f,6,9,Float.NaN,Float.POSITIVE_INFINITY,Float.NEGATIVE_INFINITY}) {
            var player=player(level,events);player.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(6);
            player.setAbsorptionAmount(amount);
            rows.add("absorption\t"+bits(amount)+"\t"+bits(player.getAbsorptionAmount()));
        }
        Files.write(Path.of(args[0]),rows);
        System.out.println(rows.size()+" native attribute-update observations");
    }
}
