import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.TeamMsgCommand;
import net.minecraft.world.scores.*;
public final class TeamMessageVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var field=TeamMsgCommand.class.getDeclaredField("SUGGEST_STYLE");field.setAccessible(true);Style style=(Style)field.get(null);
        var rows=new JsonArray();
        for(boolean color:new boolean[]{false,true}) {
            var board=new Scoreboard();var team=board.addPlayerTeam("parity");team.setDisplayName(Component.literal("Parity Team").withStyle(s->s.withBold(true)));
            if(color)team.setColor(Optional.of(TeamColor.GOLD));
            var row=new JsonObject();row.addProperty("color",color);row.add("component",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,team.getFormattedDisplayName().withStyle(style)).getOrThrow());rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
    }
}
