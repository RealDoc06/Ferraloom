import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;

/** Real registry validity and base persistence; no copies of Mojang code. */
public final class BlockEntityVectors {
  public static void main(String[] args) throws Exception {
    var out=System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(out);
    for(var type:BuiltInRegistries.BLOCK_ENTITY_TYPE){
      var blocks=new ArrayList<String>();
      for(var block:BuiltInRegistries.BLOCK)if(type.isValid(block.defaultBlockState()))blocks.add(BuiltInRegistries.BLOCK.getKey(block).toString());
      System.out.println("registry\t"+BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type)+"\t"+String.join(",",blocks));
    }
    var registries=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var portal=new TheEndPortalBlockEntity(new BlockPos(17,-4,-1),Blocks.END_PORTAL.defaultBlockState());
    for(var pos:List.of(new BlockPos(17,-4,-1),new BlockPos(-1,60,33),new BlockPos(0,0,0))){
      var tag=new CompoundTag();tag.putInt("x",pos.getX());tag.putInt("y",pos.getY());tag.putInt("z",pos.getZ());
      var corrected=BlockEntity.getPosFromTag(new net.minecraft.world.level.ChunkPos(2,-3),tag);
      System.out.println("position\t"+pos.toShortString()+"\t"+corrected.toShortString());
    }
    System.out.println("base-save\t"+portal.saveWithoutMetadata(registries));
    System.out.println("base-full\t"+portal.saveWithFullMetadata(registries));
    System.out.println("base-update\t"+portal.getUpdateTag(registries)+"\t"+(portal.getUpdatePacket()!=null));
    var patch=DataComponentPatch.builder().set(DataComponents.MAX_STACK_SIZE,7).set(DataComponents.CUSTOM_NAME,net.minecraft.network.chat.Component.literal("kept")).set(DataComponents.BLOCK_STATE,net.minecraft.world.item.component.BlockItemStateProperties.EMPTY).build();
    portal.applyComponents(DataComponentMap.EMPTY,patch);
    System.out.println("base-components\t"+portal.saveWithoutMetadata(registries));
  }
}
