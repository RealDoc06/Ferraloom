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
  static void popResourceRng(RegistryAccess r){
    // This real Level has no ServerLevel services, so native's deferred entity
    // supplier is suppressed after the public helper's eager random draws.
    for(long seed:new long[]{0,1,9123456})for(int count:new int[]{0,3})for(var face:new net.minecraft.core.Direction[]{null,Direction.UP,Direction.EAST}){
      var level=new BlockEntityFixture(r);level.random.setSeed(seed);
      var stack=new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,count);
      if(face==null)net.minecraft.world.level.block.Block.popResource(level,new BlockPos(-2,64,3),stack);
      else net.minecraft.world.level.block.Block.popResourceFromFace(level,new BlockPos(-2,64,3),face,stack);
      System.out.println("pop-resource-rng\t"+seed+"\t"+(face==null?"NONE":face)+"\t"+count+"\t"+level.random.nextInt());
    }
  }
  static final class SignPlayer extends net.minecraft.world.entity.player.Player {
    final boolean filtered;
    SignPlayer(BlockEntityFixture level,boolean filtered){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"sign"));this.filtered=filtered;}
    @Override public net.minecraft.world.level.GameType gameMode(){return net.minecraft.world.level.GameType.SURVIVAL;}
    @Override public boolean isTextFilteringEnabled(){return filtered;}
  }
  static void signEdits(RegistryAccess r){
    class EditLevel extends BlockEntityFixture {
      SignBlockEntity sign;final List<Boolean> notifications=new ArrayList<>();
      EditLevel(){super(r);}
      @Override public void sendBlockUpdated(BlockPos pos,net.minecraft.world.level.block.state.BlockState oldState,net.minecraft.world.level.block.state.BlockState newState,int flags){notifications.add(sign.getPlayerWhoMayEdit()!=null);}
    }
    for(boolean waxed:new boolean[]{false,true})for(boolean allowed:new boolean[]{false,true})for(boolean filtered:new boolean[]{false,true}){
      var level=new EditLevel();var sign=new SignBlockEntity(BlockPos.ZERO,net.minecraft.world.level.block.Blocks.OAK_SIGN.defaultBlockState());level.sign=sign;sign.setLevel(level);
      var player=new SignPlayer(level,filtered);sign.setWaxed(waxed);sign.setAllowedPlayerEditor(allowed?player.getUUID():new UUID(3,4));
      level.notifications.clear();
      var lines=List.of(net.minecraft.server.network.FilteredText.passThrough("first"),net.minecraft.server.network.FilteredText.fullyFiltered("second"),net.minecraft.server.network.FilteredText.passThrough("third"),net.minecraft.server.network.FilteredText.passThrough("fourth"));
      sign.updateSignText(player,net.minecraft.world.level.block.entity.SignTextSlot.FRONT,lines);
      System.out.println("sign-edit\t"+waxed+"\t"+allowed+"\t"+filtered+"\t"+sign.saveWithoutMetadata(r)+"\t"+(sign.getPlayerWhoMayEdit()==null));
      System.out.println("sign-edit-updates\t"+waxed+"\t"+allowed+"\t"+filtered+"\t"+level.notifications.toString().replace(" ",""));
    }
  }
  static void signFacing(RegistryAccess r){
    var level=new BlockEntityFixture(r);var player=new SignPlayer(level,false);
    for(var block:List.of(net.minecraft.world.level.block.Blocks.OAK_SIGN,net.minecraft.world.level.block.Blocks.OAK_WALL_SIGN,net.minecraft.world.level.block.Blocks.OAK_HANGING_SIGN,net.minecraft.world.level.block.Blocks.OAK_WALL_HANGING_SIGN)){
      var visited=new HashSet<String>();
      for(var state:block.getStateDefinition().getPossibleStates()){
        var orientation=state.hasProperty(net.minecraft.world.level.block.state.properties.BlockStateProperties.ROTATION_16)?state.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.ROTATION_16).toString():state.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.HORIZONTAL_FACING).toString();
        if(!visited.add(orientation))continue;
        var sign=(SignBlockEntity)((net.minecraft.world.level.block.EntityBlock)block).newBlockEntity(BlockPos.ZERO,state);
        for(var offset:new double[][]{{0.5,3.5},{3.5,3.5},{3.5,0.5},{3.5,-2.5},{0.5,-2.5},{-2.5,-2.5},{-2.5,0.5},{-2.5,3.5}}){
          player.setPos(offset[0],0.0,offset[1]);
          System.out.println("sign-facing\t"+net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(state)+"\t"+bits(offset[0])+"\t"+bits(offset[1])+"\t"+(sign.getSlotPlayerIsFacing(player)==SignTextSlot.FRONT));
        }
      }
    }
  }
  static void signEditorTick(RegistryAccess r){
    class EditorLevel extends BlockEntityFixture {
      final List<net.minecraft.world.entity.player.Player> editors=new ArrayList<>();
      EditorLevel(){super(r);}
      @Override public List<net.minecraft.world.entity.player.Player> players(){return editors;}
    }
    for(boolean present:new boolean[]{false,true})for(double x:new double[]{0.5,8.4,9.4,10.0}) {
      var level=new EditorLevel();var player=new SignPlayer(level,false);player.setPos(x,0,0.5);
      if(present)level.editors.add(player);
      var sign=new SignBlockEntity(BlockPos.ZERO,net.minecraft.world.level.block.Blocks.OAK_SIGN.defaultBlockState());
      sign.setLevel(level);sign.setAllowedPlayerEditor(player.getUUID());
      SignBlockEntity.tick(level,BlockPos.ZERO,sign.getBlockState(),sign);
      System.out.println("sign-editor-tick\t"+present+"\t"+x+"\t"+(sign.getPlayerWhoMayEdit()!=null));
    }
  }
  static void menuBroadcastReads(RegistryAccess r){
    class CountingContainer extends net.minecraft.world.SimpleContainer {
      int reads;
      CountingContainer(){super(27);}
      @Override public net.minecraft.world.item.ItemStack getItem(int slot){reads++;return super.getItem(slot);}
    }
    var level=new BlockEntityFixture(r);var player=new SignPlayer(level,false);var container=new CountingContainer();
    var menu=net.minecraft.world.inventory.ChestMenu.threeRows(1,player.getInventory(),container);
    container.reads=0;menu.broadcastChanges();System.out.println("menu-broadcast-reads\tnormal\t"+container.reads);
    container.reads=0;menu.suppressRemoteUpdates();menu.broadcastChanges();System.out.println("menu-broadcast-reads\tsuppressed\t"+container.reads);
  }
  static void menuTransferSources(RegistryAccess r)throws Exception{
    var method=net.minecraft.world.inventory.AbstractContainerMenu.class.getDeclaredMethod("moveItemStackTo",net.minecraft.world.item.ItemStack.class,int.class,int.class,boolean.class);
    method.setAccessible(true);
    for(boolean replace:new boolean[]{false,true}){
      var level=new BlockEntityFixture(r);var player=new SignPlayer(level,false);
      var source=new net.minecraft.world.SimpleContainer(1);
      source.setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,10));
      var held=source.getItem(0);var events=new ArrayList<String>();
      class ObservedContainer extends net.minecraft.world.SimpleContainer {
        boolean observe;
        ObservedContainer(){super(9);}
        @Override public void setChanged(){
          if(observe){
            events.add(source.getItem(0).getItem().toString()+":"+source.getItem(0).getCount()+":"+held.getCount());
            if(replace&&events.size()==1)source.setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIAMOND,7));
          }
          super.setChanged();
        }
      }
      var target=new ObservedContainer();
      target.setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,60));
      target.setItem(1,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,63));
      var menu=new net.minecraft.world.inventory.ChestMenu(net.minecraft.world.inventory.MenuType.GENERIC_9x1,1,player.getInventory(),target,1);
      target.observe=true;
      method.invoke(menu,held,0,9,false);
      if(held.getCount()!=0||events.size()!=5)throw new AssertionError("incomplete transfer");
      if(replace&&(!source.getItem(0).is(net.minecraft.world.item.Items.DIAMOND)||source.getItem(0).getCount()!=7))throw new AssertionError("orphaned source redirected to replacement");
      System.out.println("menu-transfer-source\t"+replace+"\t"+String.join(",",events)+"\t"+held.getCount()+"\t"+source.getItem(0).getItem()+":"+source.getItem(0).getCount());
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
