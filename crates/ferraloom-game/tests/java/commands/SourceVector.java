import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.arguments.EntityAnchorArgument.Anchor;
import net.minecraft.network.chat.Component;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.permissions.*;
import net.minecraft.util.TaskChainer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Captures owned-source transformations without evaluating commands or world mutations. */
public final class SourceVector {
    static final List<String> calls=new ArrayList<>();
    static final CommandResultCallback first=(success,value)->calls.add("first:"+success+":"+value);
    static final CommandResultCallback second=(success,value)->calls.add("second:"+success+":"+value);
    static ServerLevel overworld,nether;
    static CommandSource normal,other,always;
    static Entity a,b;
    static CommandSigningContext signing;
    static TaskChainer chainer;
    static CommandSourceStack base;
    static JsonObject snapshot(String operation,CommandSourceStack source) throws Exception {
        var out=new JsonObject();out.addProperty("operation",operation);
        var position=new JsonArray();position.add(source.getPosition().x);position.add(source.getPosition().y);position.add(source.getPosition().z);out.add("position",position);
        var rotation=new JsonArray();rotation.add(Float.floatToIntBits(source.getRotation().x));rotation.add(Float.floatToIntBits(source.getRotation().y));out.add("rotation",rotation);
        var field=CommandSourceStack.class.getDeclaredField("source");field.setAccessible(true);var sink=field.get(source);
        out.addProperty("source",sink==normal?"normal":sink==always?"always":"other");
        out.addProperty("entity",source.getEntity()==a?"a":source.getEntity()==b?"b":"none");
        out.addProperty("level",source.getLevel()==nether?"nether":"overworld");
        out.addProperty("name",source.getDisplayName().getString());out.addProperty("text_name",source.getTextName());
        out.addProperty("silent",source.isSilent());out.addProperty("eyes",source.getAnchor()==Anchor.EYES);
        out.addProperty("signing",source.getSigningContext()==signing);out.addProperty("chainer",source.getChatMessageChainer()==chainer);
        var permissions=new JsonArray();for(var level:PermissionLevel.values())permissions.add(source.permissions().hasPermission(new Permission.HasCommandLevel(level)));out.add("permissions",permissions);
        calls.clear();source.callback().onResult(true,17);var events=new JsonArray();calls.forEach(events::add);out.add("callbacks",events);
        return out;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        normal=mock(CommandSource.class);other=mock(CommandSource.class);always=mock(CommandSource.class);when(always.alwaysAccepts()).thenReturn(true);
        overworld=mock(ServerLevel.class);nether=mock(ServerLevel.class);
        var ordinary=mock(DimensionType.class);var scaled=mock(DimensionType.class);when(ordinary.coordinateScale()).thenReturn(1.);when(scaled.coordinateScale()).thenReturn(8.);
        when(overworld.dimensionType()).thenReturn(ordinary);when(nether.dimensionType()).thenReturn(scaled);
        a=mock(Entity.class);b=mock(Entity.class);when(a.getDisplayName()).thenReturn(Component.literal("DisplayA"));when(a.getPlainTextName()).thenReturn("PlainA");when(a.getEyeHeight()).thenReturn(1.625f);
        when(b.getDisplayName()).thenReturn(Component.literal("DisplayB"));when(b.getPlainTextName()).thenReturn("PlainB");
        signing=mock(CommandSigningContext.class);chainer=mock(TaskChainer.class);
        base=new CommandSourceStack(normal,new Vec3(80.,64.,-16.),new Vec2(15f,-37f),overworld,LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4)),Component.literal("Server"),mock(MinecraftServer.class));
        var out=new JsonArray();out.add(snapshot("base",base));out.add(snapshot("source",base.withSource(other)));out.add(snapshot("entity",base.withEntity(a)));out.add(snapshot("entity_again",base.withEntity(a).withEntity(b)));
        out.add(snapshot("position",base.withPosition(new Vec3(-2.,99.,3.))));out.add(snapshot("rotation",base.withRotation(new Vec2(-91f,540f))));
        out.add(snapshot("callback",base.withCallback(first)));out.add(snapshot("combined",base.withCallback(first).withCallback(second,CommandResultCallback::chain)));
        out.add(snapshot("replaced",base.withCallback(first).withCallback(second,(left,right)->right)));
        out.add(snapshot("suppressed",base.withSuppressedOutput()));out.add(snapshot("always",base.withSource(always).withSuppressedOutput()));
        out.add(snapshot("permission",base.withPermission(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(1)))));out.add(snapshot("maximum",base.withMaximumPermission(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(0)))));
        out.add(snapshot("anchor",base.withAnchor(Anchor.EYES)));out.add(snapshot("level",base.withEntity(a).withLevel(nether)));out.add(snapshot("level_roundtrip",base.withLevel(nether).withLevel(overworld)));
        out.add(snapshot("signing",base.withSigningContext(signing,chainer)));
        for(var target:List.of(new Vec3(80,64,-16),new Vec3(90,70,-15),new Vec3(-500,-90,100),new Vec3(80,100,-16))) {
            out.add(snapshot("facing:"+target.x+","+target.y+","+target.z,base.facing(target)));
            out.add(snapshot("eyes_facing:"+target.x+","+target.y+","+target.z,base.withEntity(a).withAnchor(Anchor.EYES).facing(target)));
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
