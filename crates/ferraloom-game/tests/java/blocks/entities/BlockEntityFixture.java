import java.util.*;
import net.minecraft.core.*;
import net.minecraft.world.TickRateManager;
import net.minecraft.world.level.block.entity.*;

/** Real Level + LevelChunk storage. Only absent external services are replaced. */
class BlockEntityFixture extends MutationFixture {
  final TickRateManager clock=new TickRateManager();
  BlockEntityFixture(RegistryAccess r){super(r);}
  @Override public BlockEntity getBlockEntity(BlockPos pos){return getChunkAt(pos).getBlockEntity(pos);}
  @Override public TickRateManager tickRateManager(){return clock;}
  @Override public boolean shouldTickBlocksAt(BlockPos pos){return true;}
  @Override public void blockEntityChanged(BlockPos pos){}
  final List<net.minecraft.world.entity.item.ItemEntity> dropped=new ArrayList<>();
  @Override public boolean addFreshEntity(net.minecraft.world.entity.Entity entity){dropped.add((net.minecraft.world.entity.item.ItemEntity)entity);return true;}
  static String bits(double value){return Long.toUnsignedString(Double.doubleToRawLongBits(value),16);}
  static void scatter(RegistryAccess r){
    for(long seed:new long[]{0,1,9123456})for(int count:new int[]{0,1,64,99}){
      var level=new BlockEntityFixture(r);level.random.setSeed(seed);
      var stack=new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,count);
      net.minecraft.world.Containers.dropItemStack(level,-1.2,64.7,2.8,stack);
      var drops=new ArrayList<String>();
      for(var item:level.dropped){var v=item.getDeltaMovement();drops.add(item.getItem().getCount()+"|"+bits(item.getX())+","+bits(item.getY())+","+bits(item.getZ())+"|"+bits(v.x)+","+bits(v.y)+","+bits(v.z));}
      System.out.println("scatter\t"+seed+"\t"+count+"\t"+String.join(";",drops)+"\t"+level.random.nextInt());
    }
  }
  static final class SignPlayer extends net.minecraft.world.entity.player.Player {
    final boolean filtered;
    SignPlayer(BlockEntityFixture level,boolean filtered){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"sign"));this.filtered=filtered;}
    @Override public net.minecraft.world.level.GameType gameMode(){return net.minecraft.world.level.GameType.SURVIVAL;}
    @Override public boolean isTextFilteringEnabled(){return filtered;}
  }
  static void signEdits(RegistryAccess r){
    for(boolean waxed:new boolean[]{false,true})for(boolean allowed:new boolean[]{false,true})for(boolean filtered:new boolean[]{false,true}){
      var level=new BlockEntityFixture(r);var sign=new SignBlockEntity(BlockPos.ZERO,net.minecraft.world.level.block.Blocks.OAK_SIGN.defaultBlockState());sign.setLevel(level);
      var player=new SignPlayer(level,filtered);sign.setWaxed(waxed);sign.setAllowedPlayerEditor(allowed?player.getUUID():new UUID(3,4));
      var lines=List.of(net.minecraft.server.network.FilteredText.passThrough("first"),net.minecraft.server.network.FilteredText.fullyFiltered("second"),net.minecraft.server.network.FilteredText.passThrough("third"),net.minecraft.server.network.FilteredText.passThrough("fourth"));
      sign.updateSignText(player,net.minecraft.world.level.block.entity.SignTextSlot.FRONT,lines);
      System.out.println("sign-edit\t"+waxed+"\t"+allowed+"\t"+filtered+"\t"+sign.saveWithoutMetadata(r)+"\t"+(sign.getPlayerWhoMayEdit()==null));
    }
  }
  static void tickerPhases(RegistryAccess r){
    var level=new BlockEntityFixture(r);var events=new ArrayList<String>();
    class Ticker implements TickingBlockEntity {
      final String name;boolean removed;Runnable action=()->{};
      Ticker(String name){this.name=name;}
      public boolean isRemoved(){return removed;}
      public BlockPos getPos(){return BlockPos.ZERO;}
      public String getType(){return name;}
      public void tick(){events.add(name);action.run();}
    }
    var a=new Ticker("a");var b=new Ticker("b");var c=new Ticker("c");
    a.action=()->{b.removed=true;level.addBlockEntityTicker(c);a.action=()->{};};
    level.addBlockEntityTicker(a);level.addBlockEntityTicker(b);
    level.tickBlockEntities();System.out.println("ticker-phase\tfirst\t"+String.join(",",events));events.clear();
    level.tickBlockEntities();System.out.println("ticker-phase\tsecond\t"+String.join(",",events));events.clear();
    level.clock.setFrozen(true);level.clock.tick();a.removed=true;
    level.tickBlockEntities();System.out.println("ticker-phase\tfrozen\t"+String.join(",",events));
  }
}
