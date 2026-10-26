import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerPlayer;

/** Actual ServerPlayer.SavedPosition MAP_CODEC, including partial record values
 * consumed by TagValueInput#read; no replacement position/rotation decoder. */
public class SavedPositionVectors {
    static String doubles(net.minecraft.world.phys.Vec3 p) {
        return Long.toUnsignedString(Double.doubleToRawLongBits(p.x),16)+","+Long.toUnsignedString(Double.doubleToRawLongBits(p.y),16)+","+Long.toUnsignedString(Double.doubleToRawLongBits(p.z),16);
    }
    static String floats(net.minecraft.world.phys.Vec2 p) {
        return Integer.toUnsignedString(Float.floatToRawIntBits(p.x),16)+","+Integer.toUnsignedString(Float.floatToRawIntBits(p.y),16);
    }
    public static void main(String[] args) throws Exception {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        List<String> dimensions=List.of("", "Dimension:'minecraft:overworld'", "Dimension:'minecraft:the_nether'", "Dimension:'fixture:absent'", "Dimension:'BAD:value'", "Dimension:4", "Dimension:'the_end'", "Dimension:''");
        List<String> positions=List.of("", "Pos:[1.5d,-60d,8d]", "Pos:[1d,2d]", "Pos:[1d,2d,3d,4d]", "Pos:[1d,'bad',2d,3d]", "Pos:[I;1,2,3]", "Pos:1");
        List<String> rotations=List.of("", "Rotation:[90f,-20f]", "Rotation:[1f]", "Rotation:[1f,2f,3f]", "Rotation:[1f,'bad',2f]", "Rotation:[L;1L,2L]", "Rotation:2");
        System.out.println("SAVED_POSITION\t-\t-\t-\t-");
        for(String d:dimensions)for(String p:positions)for(String r:rotations){
            String input="{"+String.join(",", java.util.stream.Stream.of(d,p,r).filter(s->!s.isEmpty()).toList())+"}";
            var tag=TagParser.parseCompoundFully(input);
            var decoded=ServerPlayer.SavedPosition.MAP_CODEC.codec().parse(NbtOps.INSTANCE,tag).resultOrPartial(message->{}).orElse(ServerPlayer.SavedPosition.EMPTY);
            System.out.println("SAVED_POSITION\t"+input+"\t"+decoded.dimension().map(k->k.identifier().toString()).orElse("-")+"\t"+decoded.position().map(SavedPositionVectors::doubles).orElse("-")+"\t"+decoded.rotation().map(SavedPositionVectors::floats).orElse("-"));
        }
    }
}
