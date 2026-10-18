import com.google.gson.*;
import com.mojang.brigadier.StringReader;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.coordinates.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.TeleportCommand;
import net.minecraft.server.commands.LookAt;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.world.entity.*;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Invokes vanilla's private performTeleport and captures its real arguments to Entity#teleportTo. */
public final class TeleportVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var perform = TeleportCommand.class.getDeclaredMethod("performTeleport", net.minecraft.commands.CommandSourceStack.class, Entity.class, ServerLevel.class, double.class, double.class, double.class, Set.class, float.class, float.class, LookAt.class);
        perform.setAccessible(true);
        var relativesMethod = TeleportCommand.class.getDeclaredMethod("getRelatives", Coordinates.class, Coordinates.class, boolean.class);
        relativesMethod.setAccessible(true);
        var results = new JsonArray();
        for (var input : List.of("10 70 -5", "~1 ~ ~-2", "^1 ^2 ^3", "29999999 64 -30000000", "30000000 64 0", "0 -20000001 0")) {
            for (var rotationText : List.of("", "~30 ~-20", "450 100")) {
                var source = Commands.createCompilationContext(LevelBasedPermissionSet.ALL).withPosition(new Vec3(80,64,-16)).withRotation(new Vec2(-60,35));
                var coordinates = Vec3Argument.vec3().parse(new StringReader(input));
                var rotation = rotationText.isEmpty() ? null : RotationArgument.rotation().parse(new StringReader(rotationText));
                var position = coordinates.getPosition(source);
                var requestedRotation = rotation == null ? new Vec2(30,370) : rotation.getRotation(source);
                var relatives = (Set<Relative>)relativesMethod.invoke(null, coordinates, rotation, true);
                var vector = new JsonObject(); vector.addProperty("input", input); vector.addProperty("rotation", rotationText);
                var victim = mock(Entity.class);
                doAnswer(invocation -> {
                    var change = new PositionMoveRotation(new Vec3(invocation.getArgument(1), invocation.getArgument(2), invocation.getArgument(3)), Vec3.ZERO, invocation.getArgument(5), invocation.getArgument(6));
                    var absolute = PositionMoveRotation.calculateAbsolute(new PositionMoveRotation(new Vec3(1,2,3), Vec3.ZERO, 370,30), change, invocation.getArgument(4));
                    vector.add("position", new Gson().toJsonTree(List.of(absolute.position().x, absolute.position().y, absolute.position().z)));
                    vector.add("change", new Gson().toJsonTree(List.of(change.position().x, change.position().y, change.position().z, (double)change.yRot(), (double)change.xRot())));
                    vector.add("absolute_rotation", new Gson().toJsonTree(List.of(absolute.yRot(), absolute.xRot())));
                    vector.addProperty("relatives", Relative.pack(relatives));
                    return false;
                }).when(victim).teleportTo(any(), anyDouble(), anyDouble(), anyDouble(), anySet(), anyFloat(), anyFloat(), anyBoolean());
                try {
                    perform.invoke(null, source, victim, null, position.x, position.y, position.z, relatives, requestedRotation.y, requestedRotation.x, null);
                } catch (java.lang.reflect.InvocationTargetException failure) {
                    var error = (com.mojang.brigadier.exceptions.CommandSyntaxException)failure.getCause();
                    vector.addProperty("error", error.getRawMessage().getString());
                }
                results.add(vector);
            }
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(results) + "\n");
    }
}
