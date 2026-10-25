import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.stats.*;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.piston.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.material.*;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.*;
import net.minecraft.world.ticks.*;

/** Real ServerLevel, chunks, neighbor updater, scheduled ticks and block methods.
 * Only external packet/sound consumers and Player's open/stat sinks are observed. */
public final class DeviceVectors extends RedstoneIdentityVectors {
    final List<String> updates=new ArrayList<>(), neighbors=new ArrayList<>(), gameplay=new ArrayList<>(), particles=new ArrayList<>();
    final LevelTicks<Block> ticks=new LevelTicks<>(p->true);
    final Map<ChunkPos,LevelChunkTicks<Block>> containers=new HashMap<>();
    long time,subTick;
    DeviceVectors(GameTestServer server,LevelStorageSource.LevelStorageAccess storage){super(server,storage);}
    @Override public long getGameTime(){return time;}
    @Override public long nextSubTickCount(){return subTick++;}
    @Override public LevelTicks<Block> getBlockTicks(){return ticks;}
    @Override public void sendBlockUpdated(BlockPos pos,BlockState old,BlockState state,int flags){updates.add(pos.toShortString()+"="+Block.getId(state)+"/"+flags);}
    @Override public void updateNeighborsAt(BlockPos pos,Block block,net.minecraft.world.level.redstone.Orientation orientation){neighbors.add(pos.toShortString()+"/"+(orientation==null?-1:orientation.getIndex()));super.updateNeighborsAt(pos,block,orientation);}
    @Override public void gameEvent(Holder<net.minecraft.world.level.gameevent.GameEvent> event,Vec3 pos,net.minecraft.world.level.gameevent.GameEvent.Context context){gameplay.add(BuiltInRegistries.GAME_EVENT.getId(event.value())+":"+(context.sourceEntity()==null?0:context.sourceEntity().getId())+":"+(context.affectedState()==null?-1:Block.getId(context.affectedState())));}
    @Override public void addParticle(net.minecraft.core.particles.ParticleOptions particle,double x,double y,double z,double dx,double dy,double dz){particles.add(Double.doubleToRawLongBits(x)+","+Double.doubleToRawLongBits(y)+","+Double.doubleToRawLongBits(z));}
    @Override public void blockEvent(BlockPos pos,Block block,int a,int b){gameplay.add("B:"+a+":"+b);}
    @Override public void levelEvent(Entity actor,int event,BlockPos pos,int data){gameplay.add("L:"+event+":"+data+":"+(actor==null?0:actor.getId()));}
    void raw(BlockPos pos,BlockState state){getChunkAt(pos).setBlockState(pos,state,816);}
    void prepare(BlockState state){
        for(var entry:containers.entrySet())ticks.removeContainer(entry.getKey());containers.clear();
        super.reset();time=0;subTick=0;
        for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++){
            var pos=new ChunkPos(x,z);var container=new LevelChunkTicks<Block>();containers.put(pos,container);ticks.addContainer(pos,container);getChunkAt(new BlockPos(x*16,64,z*16)).setLoaded(true);
        }
        raw(POS.below(),Blocks.STONE.defaultBlockState());raw(POS,state);clear();
    }
    void clear(){updates.clear();neighbors.clear();gameplay.clear();particles.clear();}
    static void out(String name,String value){System.out.println("DEVICE\t"+name+"\t"+value);}
    String mutations(){return String.join(";",updates)+"|"+String.join(";",neighbors);}
    static final class Actor extends Player {
        final List<String> calls=new ArrayList<>();boolean accept=true;
        Actor(Level level){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"redstone"));setId(1);setPos(Vec3.atCenterOf(POS));}
        @Override public GameType gameMode(){return GameType.SURVIVAL;}
        @Override public OptionalInt openMenu(net.minecraft.world.MenuProvider provider){
            calls.add("open:"+((net.minecraft.network.chat.contents.TranslatableContents)provider.getDisplayName().getContents()).getKey());
            if(!accept)return OptionalInt.empty();
            var menu=provider.createMenu(7,getInventory(),this);
            calls.add("menu:"+(menu==null?-1:BuiltInRegistries.MENU.getId(menu.getType())));
            return OptionalInt.of(7);
        }
        @Override public void awardStat(Stat<?> stat,int amount){calls.add("stat:"+stat.getValue()+":"+amount);}
    }
    BlockHitResult hit(Direction face){return new BlockHitResult(Vec3.atCenterOf(POS),face,POS,false);}
    Integer skyBrightness;
    net.minecraft.world.attribute.EnvironmentAttributeSystem attributes;
    @Override public int getEffectiveSkyBrightness(BlockPos pos){return skyBrightness==null?super.getEffectiveSkyBrightness(pos):skyBrightness;}
    @Override public net.minecraft.world.attribute.EnvironmentAttributeSystem environmentAttributes(){return attributes==null?super.environmentAttributes():attributes;}
    void daylight()throws Exception{
        var method=DaylightDetectorBlock.class.getDeclaredMethod("updateSignalStrength",BlockState.class,Level.class,BlockPos.class);method.setAccessible(true);
        prepare(Blocks.DAYLIGHT_DETECTOR.defaultBlockState());
        for(int angle=-360;angle<=720;angle+=3){
            float fixedAngle=angle;
            attributes=net.minecraft.world.attribute.EnvironmentAttributeSystem.builder().addConstantLayer(net.minecraft.world.attribute.EnvironmentAttributes.SUN_ANGLE,prior->fixedAngle).build();
            for(int brightness=0;brightness<=15;brightness++)for(boolean inverted:new boolean[]{false,true}){
                skyBrightness=brightness;
                var state=Blocks.DAYLIGHT_DETECTOR.defaultBlockState().setValue(DaylightDetectorBlock.INVERTED,inverted);
                raw(POS,state);method.invoke(null,state,this,POS);
                out("daylight:"+brightness+":"+angle+":"+inverted,""+getBlockState(POS).getValue(DaylightDetectorBlock.POWER));clear();
            }
        }
        skyBrightness=null;attributes=null;
    }
    void rods(){
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof LightningRodBlock rod)for(var state:block.getStateDefinition().getPossibleStates())for(int operation=0;operation<3;operation++){
            prepare(state);random.setSeed(42);
            if(operation==0)state.onPlace(this,POS,Blocks.AIR.defaultBlockState(),false);
            else if(operation==1)rod.onLightningStrike(state,this,POS);
            else state.affectNeighborsAfterRemoval(this,POS,false);
            var samples=new ArrayList<String>();
            for(int tick=0;tick<=8;tick++){
                time=tick;ticks.tick(tick,65536,(pos,b)->{var current=getBlockState(pos);if(current.is(b))current.tick(this,pos,random);});
                samples.add(Block.getId(getBlockState(POS))+":"+ticks.hasScheduledTick(POS,block)+"|"+mutations()+"|"+String.join(";",gameplay));clear();
            }
            out("rod:"+Block.getId(state)+":"+operation,String.join("#",samples)+"|"+random.nextLong());
        }
    }
    void notes(){
        for(var state:Blocks.NOTE_BLOCK.getStateDefinition().getPossibleStates())for(boolean blocked:new boolean[]{false,true}){
            prepare(state);if(blocked)raw(POS.above(),Blocks.STONE.defaultBlockState());clear();var actor=new Actor(this);
            var result=state.useWithoutItem(this,actor,hit(Direction.UP));
            out("note:"+Block.getId(state)+":"+blocked,(result==InteractionResult.SUCCESS)+"|"+Block.getId(getBlockState(POS))+"|"+String.join(";",gameplay)+"|"+String.join(";",actor.calls));
        }
        for(var item:List.of(Items.AIR,Items.STONE,Items.SKELETON_SKULL,Items.CREEPER_HEAD,Items.PLAYER_HEAD,Items.REDSTONE))for(var face:Direction.values())for(int count:new int[]{0,1}){
            prepare(Blocks.NOTE_BLOCK.defaultBlockState());var actor=new Actor(this);var stack=new ItemStack(item,count);actor.setItemInHand(InteractionHand.MAIN_HAND,stack);
            var result=getBlockState(POS).useItemOn(stack,this,actor,InteractionHand.MAIN_HAND,hit(face));
            out("note-item:"+BuiltInRegistries.ITEM.getId(item)+":"+count+":"+face.get3DDataValue(),result==InteractionResult.PASS?"pass":"empty");
        }
    }
    void containers(){
        for(var block:List.of(Blocks.DISPENSER,Blocks.DROPPER,Blocks.HOPPER))for(boolean accept:new boolean[]{false,true}){
            prepare(block.defaultBlockState());var actor=new Actor(this);actor.accept=accept;
            var result=getBlockState(POS).useWithoutItem(this,actor,hit(Direction.UP));
            out("container:"+BuiltInRegistries.BLOCK.getId(block)+":"+accept,(result==InteractionResult.SUCCESS)+"|"+String.join(";",actor.calls));
        }
    }
    void destruction(){
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof DoorBlock)for(var state:block.getStateDefinition().getPossibleStates())for(int mode=0;mode<3;mode++)for(int lower=0;lower<3;lower++){
            prepare(state);var actor=new Actor(this);actor.getAbilities().instabuild=mode==1;
            if(mode==2)actor.setItemInHand(InteractionHand.MAIN_HAND,new ItemStack(Items.IRON_PICKAXE));
            if(lower!=0)raw(POS.below(),lower==1?state.setValue(DoorBlock.HALF,DoubleBlockHalf.LOWER):Blocks.STONE.defaultBlockState());clear();
            var returned=block.playerWillDestroy(this,POS,state,actor);
            out("door:"+Block.getId(state)+":"+mode+":"+lower,Block.getId(returned)+"|"+Block.getId(getBlockState(POS.below()))+"|"+String.join(";",gameplay));
        }
        for(var state:Blocks.PISTON_HEAD.getStateDefinition().getPossibleStates())for(int kind=0;kind<5;kind++)for(boolean creative:new boolean[]{false,true}){
            prepare(state);var actor=new Actor(this);actor.getAbilities().instabuild=creative;
            var facing=state.getValue(PistonHeadBlock.FACING);var base=POS.relative(facing.getOpposite());
            var block=kind==0?Blocks.AIR:kind<3?Blocks.PISTON:Blocks.STICKY_PISTON;
            var b=block.defaultBlockState();if(kind!=0)b=b.setValue(PistonBaseBlock.FACING,facing).setValue(PistonBaseBlock.EXTENDED,kind%2==0);
            raw(base,b);clear();var returned=Blocks.PISTON_HEAD.playerWillDestroy(this,POS,state,actor);
            out("head:"+Block.getId(state)+":"+kind+":"+creative,Block.getId(returned)+"|"+Block.getId(getBlockState(base))+"|"+String.join(";",gameplay));
        }
    }
    void ore(){
        for(var block:List.of(Blocks.REDSTONE_ORE,Blocks.DEEPSLATE_REDSTONE_ORE))for(var state:block.getStateDefinition().getPossibleStates())for(int mask=0;mask<64;mask++)for(boolean careful:new boolean[]{false,true}){
            prepare(state);for(var face:Direction.values())if((mask&(1<<face.get3DDataValue()))!=0)raw(POS.relative(face),Blocks.STONE.defaultBlockState());clear();
            var actor=new Actor(this);actor.setShiftKeyDown(careful);random.setSeed(42);state.getBlock().stepOn(this,POS,state,actor);
            out("ore-step:"+Block.getId(state)+":"+mask+":"+careful,Block.getId(getBlockState(POS))+"|"+String.join(";",particles)+"|"+random.nextLong());
        }
        for(var block:List.of(Blocks.REDSTONE_ORE,Blocks.DEEPSLATE_REDSTONE_ORE))for(var state:block.getStateDefinition().getPossibleStates()){
            prepare(state);state.randomTick(this,POS,random);out("ore-tick:"+Block.getId(state),""+Block.getId(getBlockState(POS)));
            for(var item:List.of(Items.AIR,Items.STONE,Items.REDSTONE,Items.SKELETON_SKULL))for(var face:Direction.values())for(boolean blocked:new boolean[]{false,true}){
                prepare(state);if(blocked)raw(POS.relative(face),Blocks.STONE.defaultBlockState());clear();var actor=new Actor(this);var stack=new ItemStack(item);actor.setItemInHand(InteractionHand.MAIN_HAND,stack);random.setSeed(42);
                var result=state.useItemOn(stack,this,actor,InteractionHand.MAIN_HAND,hit(face));
                out("ore-use:"+Block.getId(state)+":"+BuiltInRegistries.ITEM.getId(item)+":"+face.get3DDataValue()+":"+blocked,(result==InteractionResult.PASS?"pass":"success")+"|"+Block.getId(getBlockState(POS))+"|"+random.nextLong());
            }
        }
    }
    void movingDrops(){
        var moving=Blocks.MOVING_PISTON.defaultBlockState();
        var doubleSlab=Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.DOUBLE);
        for(var moved:List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),doubleSlab))for(boolean present:new boolean[]{false,true}){
            prepare(moving);if(present)getChunkAt(POS).setBlockEntity(MovingPistonBlock.newMovingBlockEntity(POS,moving,moved,Direction.EAST,true,false));
            var params=new net.minecraft.world.level.storage.loot.LootParams.Builder(this).withParameter(net.minecraft.world.level.storage.loot.parameters.LootContextParams.ORIGIN,Vec3.atCenterOf(POS)).withParameter(net.minecraft.world.level.storage.loot.parameters.LootContextParams.TOOL,ItemStack.EMPTY);
            out("moving-drops:"+Block.getId(moved)+":"+present,String.join(";",moving.getDrops(params).stream().map(RedstoneIdentityVectors::stack).toList()));
        }
    }
    void detectors(){
        for(var state:Blocks.DETECTOR_RAIL.getStateDefinition().getPossibleStates())for(int operation=0;operation<3;operation++){
            prepare(state);raw(POS.north(),Blocks.STONE.defaultBlockState());raw(POS.south(),Blocks.STONE.defaultBlockState());raw(POS.east(),Blocks.STONE.defaultBlockState());raw(POS.west(),Blocks.STONE.defaultBlockState());clear();
            if(operation==0)state.onPlace(this,POS,Blocks.AIR.defaultBlockState(),false);
            else if(operation==1)state.tick(this,POS,random);
            else state.entityInside(this,POS,new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,this),InsideBlockEffectApplier.NOOP,true);
            out("detector:"+Block.getId(state)+":"+operation,Block.getId(getBlockState(POS))+"|"+mutations()+"|"+getBlockState(POS).getAnalogOutputSignal(this,POS,Direction.UP)+"|"+ticks.hasScheduledTick(POS,Blocks.DETECTOR_RAIL));
        }
    }
    void crafterPlacement(){
        for(int yaw:new int[]{0,90,180,270})for(int pitch:new int[]{-90,-45,0,45,90})for(boolean signal:new boolean[]{false,true}){
            prepare(Blocks.AIR.defaultBlockState());if(signal)raw(POS.west(),Blocks.REDSTONE_BLOCK.defaultBlockState());
            var actor=new Actor(this);actor.setYRot(yaw);actor.setYHeadRot(yaw);actor.setXRot(pitch);var stack=new ItemStack(Items.CRAFTER);
            var context=new net.minecraft.world.item.context.BlockPlaceContext(actor,InteractionHand.MAIN_HAND,stack,hit(Direction.UP));
            out("crafter-placement:"+yaw+":"+pitch+":"+signal,""+Block.getId(Blocks.CRAFTER.getStateForPlacement(context)));
        }
    }
    void crafterAliases(){
        for(boolean full:new boolean[]{false,true})for(boolean empty:new boolean[]{false,true}){
            prepare(Blocks.CRAFTER.defaultBlockState());var entity=(CrafterBlockEntity)getBlockEntity(POS);
            int slot=full?0:4;entity.setItem(slot,new ItemStack(Items.MILK_BUCKET,1));if(full)entity.setItem(8,new ItemStack(Items.STONE,1));
            var held=entity.getItem(slot);var input=entity.asCraftInput();
            entity.setItem(slot,new ItemStack(Items.DIRT,5));if(empty)held.setCount(0);else held.grow(2);
            var remaining=net.minecraft.world.item.crafting.CraftingRecipe.defaultCraftingReminder(input);
            out("crafter-alias:"+full+":"+empty,input.width()+","+input.height()+","+input.ingredientCount()+"|"+stack(input.getItem(0))+"|"+(input.getItem(0)==held)+"|"+String.join(";",remaining.stream().map(RedstoneIdentityVectors::stack).toList()));
        }
    }
    void crafters()throws Exception{
        var remainingField=CrafterBlockEntity.class.getDeclaredField("craftingTicksRemaining");remainingField.setAccessible(true);
        for(var state:Blocks.CRAFTER.getStateDefinition().getPossibleStates())for(boolean signal:new boolean[]{false,true}){
            prepare(state);if(signal)raw(POS.west(),Blocks.REDSTONE_BLOCK.defaultBlockState());clear();
            neighborChanged(state,POS,Blocks.STONE,null,false);var entity=(CrafterBlockEntity)getBlockEntity(POS);
            out("crafter-edge:"+Block.getId(state)+":"+signal,Block.getId(getBlockState(POS))+"|"+entity.isTriggered()+"|"+ticks.hasScheduledTick(POS,Blocks.CRAFTER));
        }
        for(int count:new int[]{Integer.MIN_VALUE,-1,0,1,2,6,Integer.MAX_VALUE}){
            var state=Blocks.CRAFTER.defaultBlockState().setValue(CrafterBlock.CRAFTING,true);prepare(state);var entity=(CrafterBlockEntity)getBlockEntity(POS);entity.setCraftingTicksRemaining(count);
            CrafterBlockEntity.serverTick(this,POS,state,entity);
            out("crafter-ticker:"+count,remainingField.getInt(entity)+"|"+Block.getId(getBlockState(POS)));
        }
        for(var orientation:FrontAndTop.values())for(int input=0;input<4;input++)for(int target=0;target<3;target++){
            var state=Blocks.CRAFTER.defaultBlockState().setValue(BlockStateProperties.ORIENTATION,orientation);prepare(state);var entity=(CrafterBlockEntity)getBlockEntity(POS);
            if(input==1||input==2)entity.setItem(4,new ItemStack(Items.OAK_LOG,input==1?1:8));
            if(input==3)for(int i=0;i<9;i++)entity.setItem(i,new ItemStack(Items.IRON_INGOT,2));
            var into=POS.relative(orientation.front());
            if(target!=0){raw(into,Blocks.BARREL.defaultBlockState());if(target==2){var barrel=(BarrelBlockEntity)getBlockEntity(into);for(int i=0;i<27;i++)barrel.setItem(i,new ItemStack(Items.DIRT,64));}}
            var spawned=new ArrayList<String>();onAdd=()->{};
            callbackSpawn=actor->{if(actor instanceof net.minecraft.world.entity.item.ItemEntity item)spawned.add(stack(item.getItem()));};
            random.setSeed(42);clear();state.tick(this,POS,random);
            var destination=getBlockEntity(into);var stored=destination instanceof BarrelBlockEntity barrel?stack(barrel.getItem(0)):"-";
            out("crafter-execute:"+Block.getId(state)+":"+input+":"+target,Block.getId(getBlockState(POS))+"|"+remainingField.getInt(entity)+"|"+stack(entity.getItem(4))+"|"+stored+"|"+String.join(";",spawned)+"|"+String.join(";",gameplay)+"|"+random.nextLong());
            callbackSpawn=null;
        }
    }
    void crafterPulses()throws Exception{
        var remainingField=CrafterBlockEntity.class.getDeclaredField("craftingTicksRemaining");remainingField.setAccessible(true);
        for(int pulse=0;pulse<4;pulse++)for(boolean container:new boolean[]{false,true}){
            var state=Blocks.CRAFTER.defaultBlockState().setValue(BlockStateProperties.ORIENTATION,FrontAndTop.EAST_UP);
            prepare(state);var receiver=(CrafterBlockEntity)getBlockEntity(POS);receiver.setItem(4,new ItemStack(Items.OAK_LOG,2));
            if(container)raw(POS.east(),Blocks.BARREL.defaultBlockState());
            var spawned=new ArrayList<String>();callbackSpawn=entity->{if(entity instanceof net.minecraft.world.entity.item.ItemEntity item)spawned.add(stack(item.getItem()));};
            var samples=new ArrayList<String>();random.setSeed(42);clear();
            for(int tick=0;tick<=18;tick++){
                time=tick;
                if(tick==0||(pulse==2&&tick==2)||(pulse==3&&tick==6))setBlock(POS.west(),Blocks.REDSTONE_BLOCK.defaultBlockState(),3);
                if((pulse!=0&&tick==1)||(pulse==2&&tick==3)||(pulse==3&&tick==7))setBlock(POS.west(),Blocks.AIR.defaultBlockState(),3);
                ticks.tick(tick,65536,(pos,block)->{var current=getBlockState(pos);if(current.is(block))current.tick(this,pos,random);});
                CrafterBlockEntity.serverTick(this,POS,getBlockState(POS),receiver);
                var into=container?(BarrelBlockEntity)getBlockEntity(POS.east()):null;
                samples.add(Block.getId(getBlockState(POS))+","+receiver.isTriggered()+","+remainingField.getInt(receiver)+","+stack(receiver.getItem(4))+","+ticks.hasScheduledTick(POS,Blocks.CRAFTER)+"|"+String.join(";",updates)+"|"+String.join(";",gameplay)+"|"+(into==null?String.join(";",spawned):stack(into.getItem(0))));
                clear();
            }
            out("crafter-pulse:"+pulse+":"+container,String.join("#",samples)+"|"+random.nextLong());callbackSpawn=null;
        }
    }
    java.util.function.Consumer<Entity> callbackSpawn;
    @Override public boolean addFreshEntity(Entity entity){if(callbackSpawn!=null)callbackSpawn.accept(entity);return super.addFreshEntity(entity);}
    static void runDevices()throws Exception{
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"redstone-devices")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var level=new DeviceVectors(server,storage);level.daylight();level.rods();level.notes();level.containers();level.destruction();level.ore();level.movingDrops();level.detectors();level.crafterPlacement();level.crafterAliases();level.crafters();level.crafterPulses();System.out.flush();System.exit(0);
    }
}
final class DeviceCapture{
    public static void main(String[] args)throws Exception{var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);DeviceVectors.runDevices();}
}
