import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.*;

/** Extracts only BlockStateBase.Cache's existing CENTER support entries. A
 * dynamic block has no cache; its live query is deliberately never sampled. */
public final class ExtractCenterSupport {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var values=new JsonArray();
        for(var state:Block.BLOCK_STATE_REGISTRY) {
            int bits=0;
            if(!state.getBlock().hasDynamicShape()) {
                bits=64;
                for(var face:Direction.values())if(state.isFaceSturdy(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,face,SupportType.CENTER))bits|=1<<face.get3DDataValue();
            }
            values.add(bits);
        }
        Files.writeString(Path.of(args[0]),new Gson().toJson(values)+"\n");
    }
}
