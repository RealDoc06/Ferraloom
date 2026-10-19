import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.dispenser.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.*;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.Vec3;

/** Actual native hopper helpers and dispenser ticks. Callback fixtures replace
 * slots synchronously; Java object identity is recorded alongside stack values. */
public final class RedstoneIdentityVectors extends ServerLevel {
    static final BlockPos POS = new BlockPos(8,64,8);
    final Map<ChunkPos,LevelChunk> chunks = new HashMap<>();
    final List<Integer> events = new ArrayList<>();
    Runnable onAdd;
    RedstoneIdentityVectors(GameTestServer server,LevelStorageSource.LevelStorageAccess storage) {
        super(server,Runnable::run,storage,server.getWorldData().overworldData(),Level.OVERWORLD,
            server.registryAccess().lookupOrThrow(Registries.LEVEL_STEM).getValue(LevelStem.OVERWORLD),false,0,List.of(),false);
    }
    @Override public ChunkAccess getChunk(int x,int z,ChunkStatus status,boolean load) {
        var pos=new ChunkPos(x,z);
        return load?chunks.computeIfAbsent(pos,p->new LevelChunk(this,p){
            @Override public FullChunkStatus getFullStatus(){return FullChunkStatus.BLOCK_TICKING;}
        }):chunks.get(pos);
    }
    @Override public void sendBlockUpdated(BlockPos p,BlockState old,BlockState state,int flags){}
    @Override public void updatePOIOnBlockStateChange(BlockPos p,BlockState old,BlockState state){}
    @Override public void levelEvent(Entity source,int event,BlockPos p,int data){events.add(event);}
    @Override public void gameEvent(Holder<GameEvent> event,Vec3 p,GameEvent.Context context){}
    @Override public boolean addFreshEntity(Entity entity){if(onAdd!=null){var callback=onAdd;onAdd=null;callback.run();}return true;}
    static String stack(ItemStack value){return BuiltInRegistries.ITEM.getId(value.getItem())+","+value.getCount();}
    static void vector(String name,String value){System.out.println("IDENTITY\t"+name+"\t"+value);}
    void reset(){chunks.clear();events.clear();onAdd=null;random.setSeed(0);}

    static void insertion() throws Exception {
        var move=HopperBlockEntity.class.getDeclaredMethod("tryMoveInItem",Container.class,Container.class,ItemStack.class,int.class,Direction.class);move.setAccessible(true);
        for(boolean replace:new boolean[]{false,true}){
            var target=new SimpleContainer(27);target.setItem(0,new ItemStack(Items.STONE,60));
            var held=target.getItem(0);
            class LazyFirst extends SimpleContainer {
                boolean first=true;
                LazyFirst(){super(27);}
                @Override public boolean isEmpty(){
                    if(first){first=false;setItem(0,new ItemStack(Items.STONE));held.grow(2);if(replace)target.setItem(0,new ItemStack(Items.DIRT,5));}
                    return super.isEmpty();
                }
            }
            var input=new ItemStack(Items.STONE,10);var split=input.split(3);
            var result=(ItemStack)move.invoke(null,null,new CompoundContainer(new LazyFirst(),target),split,27,null);
            if(result!=split)throw new AssertionError("merge must return the fresh split receiver");
            vector("merge:"+replace,stack(input)+"|"+stack(result)+"|"+stack(held)+"|"+stack(target.getItem(0))+"|"+(target.getItem(0)==held));
        }
        for(boolean replace:new boolean[]{false,true}){
            var trace=new ArrayList<String>();
            class Target extends SimpleContainer {
                boolean first=true;
                Target(){super(27);}
                @Override public void setChanged(){
                    if(first&&!getItem(0).isEmpty()){
                        first=false;var installed=getItem(0);installed.grow(2);
                        if(replace)setItem(0,new ItemStack(Items.DIRT,5));
                        installed.grow(3);trace.add(stack(installed)+":"+(installed==getItem(0)));
                    }
                }
            }
            var target=new Target();var input=new ItemStack(Items.STONE,10);var split=input.split(3);
            var result=HopperBlockEntity.addItem(null,target,split,null);
            if(result==split)throw new AssertionError("placing returns EMPTY while the split receiver lives in the target");
            vector("place:"+replace,stack(input)+"|"+stack(result)+"|"+stack(target.getItem(0))+"|"+String.join(";",trace));
        }
    }
    static void restore() throws Exception {
        var take=HopperBlockEntity.class.getDeclaredMethod("tryTakeInItemFromSlot",Hopper.class,Container.class,int.class,Direction.class);take.setAccessible(true);
        class Sink extends SimpleContainer implements Hopper {
            Sink(){super(5);for(int i=0;i<5;i++)setItem(i,new ItemStack(Items.STONE,64));}
            public double getLevelX(){return 0;}public double getLevelY(){return 0;}public double getLevelZ(){return 0;}public boolean isGridAligned(){return true;}
        }
        for(int count:new int[]{1,5})for(boolean replace:new boolean[]{false,true}){
            class Source extends SimpleContainer {
                boolean active,first=true;ItemStack held;
                Source(){super(27);setItem(0,new ItemStack(Items.STONE,count));held=getItem(0);active=true;}
                @Override public void setChanged(){if(active&&first){first=false;held.grow(2);if(replace)setItem(0,new ItemStack(Items.DIAMOND,7));}}
            }
            var source=new Source();var success=(boolean)take.invoke(null,new Sink(),source,0,Direction.DOWN);
            vector("restore:"+count+":"+replace,success+"|"+stack(source.held)+"|"+stack(source.getItem(0))+"|"+(source.held==source.getItem(0)));
        }
    }
    void dispensing() {
        var prior=DispenserBlock.DISPENSER_REGISTRY.get(Items.STONE);
        for(boolean replace:new boolean[]{false,true}){
            reset();var state=Blocks.DISPENSER.defaultBlockState();getChunkAt(POS).setBlockState(POS,state,816);
            var receiver=(DispenserBlockEntity)getBlockEntity(POS);receiver.setItem(0,new ItemStack(Items.STONE,4));var held=receiver.getItem(0);
            DispenserBlock.registerBehavior(Items.STONE,new DefaultDispenseItemBehavior(){
                @Override protected void playAnimation(BlockSource source,Direction direction){super.playAnimation(source,direction);if(replace)receiver.setItem(0,new ItemStack(Items.DIRT,7));held.grow(3);}
            });
            state.tick(this,POS,random);
            vector("dispenser:"+replace,stack(held)+"|"+stack(receiver.getItem(0))+"|"+(held==receiver.getItem(0))+"|"+events);
        }
        if(prior==null)DispenserBlock.DISPENSER_REGISTRY.remove(Items.STONE);else DispenserBlock.registerBehavior(Items.STONE,prior);
        for(boolean replace:new boolean[]{false,true}){
            reset();var state=Blocks.DISPENSER.defaultBlockState();getChunkAt(POS).setBlockState(POS,state,816);
            var receiver=(DispenserBlockEntity)getBlockEntity(POS);receiver.setItem(0,new ItemStack(Items.ARROW,4));var held=receiver.getItem(0);
            onAdd=()->{if(replace)receiver.setItem(0,new ItemStack(Items.DIRT,7));held.grow(3);};
            state.tick(this,POS,random);
            vector("projectile:"+replace,stack(held)+"|"+stack(receiver.getItem(0))+"|"+(held==receiver.getItem(0))+"|"+events);
        }
        for(boolean accepts:new boolean[]{false,true})for(boolean replace:new boolean[]{false,true}){
            reset();var state=Blocks.DROPPER.defaultBlockState();getChunkAt(POS).setBlockState(POS,state,816);
            var receiver=(DispenserBlockEntity)getBlockEntity(POS);receiver.setItem(0,new ItemStack(Items.STONE,4));var held=receiver.getItem(0);
            var target=POS.relative(state.getValue(DispenserBlock.FACING));getChunkAt(target).setBlockState(target,Blocks.BARREL.defaultBlockState(),816);
            var sink=(BarrelBlockEntity)getBlockEntity(target);
            if(!accepts)for(int i=0;i<27;i++)sink.setItem(i,new ItemStack(Items.DIRT,64));
            // Interpose the container virtual while native DropperBlock owns
            // its original receiver and invokes native HopperBlockEntity#addItem.
            var callback=new BarrelBlockEntity(target,Blocks.BARREL.defaultBlockState()){
                boolean first=true;
                @Override public ItemStack getItem(int slot){if(first){first=false;if(replace)receiver.setItem(0,new ItemStack(Items.DIRT,7));held.grow(3);}return sink.getItem(slot);}
                @Override public boolean isEmpty(){return sink.isEmpty();}
                @Override public void setItem(int slot,ItemStack item){sink.setItem(slot,item);}
                @Override public void setChanged(){sink.setChanged();}
            };
            getChunkAt(target).setBlockEntity(callback);state.tick(this,POS,random);
            vector("dropper:"+accepts+":"+replace,stack(held)+"|"+stack(receiver.getItem(0))+"|"+(held==receiver.getItem(0)));
        }
    }
    void pickups() {
        for(boolean replace:new boolean[]{false,true}){
            reset();
            var entity=new net.minecraft.world.entity.item.ItemEntity(this,8.5,65,8.5,new ItemStack(Items.STONE,4));
            var held=entity.getItem();
            var target=new HopperBlockEntity(POS,Blocks.HOPPER.defaultBlockState()){
                boolean first=true;
                @Override public void setChanged(){
                    if(first&&!getItem(0).isEmpty()){
                        first=false;if(replace)entity.setItem(new ItemStack(Items.DIAMOND,7));held.grow(3);
                    }
                    super.setChanged();
                }
            };
            target.setLevel(this);
            var success=HopperBlockEntity.addItem(target,entity);
            vector("pickup:"+replace,success+"|"+stack(held)+"|"+stack(entity.getItem())+"|"+entity.isRemoved()+"|"+stack(target.getItem(0)));
        }
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"redstone-identity")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        insertion();restore();var level=new RedstoneIdentityVectors(server,storage);level.dispensing();level.pickups();System.out.flush();System.exit(0);
    }
}

final class RedstoneIdentityCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        System.setOut(output);System.setErr(errors);
        RedstoneIdentityVectors.run();
    }
}
