import java.util.*;
import com.mojang.serialization.JsonOps;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.FallingBlockEntity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.scores.Scoreboard;
import static org.mockito.Mockito.*;

/** Actual registered block description IDs and FallingBlockEntity virtual names. */
public final class FallingNameVectors {
  static String json(Component c){return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,c).getOrThrow().toString();}
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    for(var block:BuiltInRegistries.BLOCK)System.out.println("block-description\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+block.getDescriptionId());
    var level=mock(Level.class);when(level.getScoreboard()).thenReturn(new Scoreboard());
    var field=FallingBlockEntity.class.getDeclaredField("blockState");field.setAccessible(true);
    for(String name:List.of("sand","red_concrete_powder","wall_torch","oak_wall_sign","white_wall_banner","skeleton_wall_skull","water"))for(boolean custom:List.of(false,true)) {
      var block=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(name));
      var entity=new FallingBlockEntity(EntityTypes.FALLING_BLOCK,level);entity.setUUID(new UUID(1,2));field.set(entity,block.defaultBlockState());
      if(custom)entity.setCustomName(Component.literal("custom falling"));
      System.out.println("falling-name\t"+Block.getId(block.defaultBlockState())+"\t"+custom+"\t"+json(entity.getName())+"\t"+json(entity.getDisplayName()));
    }
  }
}
