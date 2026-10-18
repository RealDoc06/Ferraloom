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
    var dynamic=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var registries=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),dynamic.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(DataComponentInitializers.PendingComponents::apply);
    BlockEntityFixture.tickerPhases(dynamic);
    BlockEntityFixture.scatter(dynamic);
    BlockEntityFixture.signEdits(dynamic);
    var level=new BlockEntityFixture(dynamic);
    for(var block:List.of(Blocks.CHEST,Blocks.COPPER_CHEST.asList().getFirst(),Blocks.TRAPPED_CHEST,Blocks.BARREL,Blocks.SHULKER_BOX,Blocks.CRAFTER,Blocks.FURNACE,Blocks.SMOKER,Blocks.BLAST_FURNACE,Blocks.OAK_SIGN,Blocks.OAK_HANGING_SIGN)){
      var state=block.defaultBlockState();var entity=((EntityBlock)block).newBlockEntity(new BlockPos(1,64,2),state);
      System.out.println("default-save\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+entity.saveWithoutMetadata(registries));
      if(entity instanceof net.minecraft.world.Container c){
        c.setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,31));
        c.setItem(c.getContainerSize()-1,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIAMOND,3));
      }
      var saved=entity.saveWithFullMetadata(registries);
      var loaded=BlockEntity.loadStatic(entity.getBlockPos(),state,saved,registries);
      System.out.println("filled-save\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+saved);
      System.out.println("roundtrip\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+loaded.saveWithFullMetadata(registries).equals(saved));
      System.out.println("update-tag\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+entity.getUpdateTag(registries));
      entity.setLevel(level);
      System.out.println("update-packet\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+(entity.getUpdatePacket()!=null));
      if(entity instanceof RandomizableContainerBlockEntity random){
        random.setLootTable(net.minecraft.resources.ResourceKey.create(Registries.LOOT_TABLE,net.minecraft.resources.Identifier.withDefaultNamespace("chests/simple_dungeon")),-9123456789L);
        System.out.println("loot-save\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+entity.saveWithoutMetadata(registries));
        random.setLootTableSeed(0);
        System.out.println("loot-zero\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+entity.saveWithoutMetadata(registries));
      }
    }
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
