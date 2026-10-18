import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.block.piston.PistonStructureResolver;
import net.minecraft.world.level.redstone.*;

/** Real vanilla SignalGetter, Level#setBlock, wire evaluators and piston resolver.
 * The fixture uses real chunks and CollectingNeighborUpdater, with unrelated
 * abstract services throwing. Scheduled-tick scenarios use the server harness. */
public final class RedstoneVectors extends MutationFixture {
    final net.minecraft.world.level.border.WorldBorder border = new net.minecraft.world.level.border.WorldBorder();
    final List<String> changes = new ArrayList<>();
    final List<String> notifications = new ArrayList<>();
    boolean recording;
    RedstoneVectors(RegistryAccess registries) { super(new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY).registries(),registries.registries())).freeze()); for(int x=-1;x<=1;x++)for(int z=-1;z<=1;z++){blockTicks.addContainer(new net.minecraft.world.level.ChunkPos(x,z),new net.minecraft.world.ticks.LevelChunkTicks<>());getChunkAt(new BlockPos(x*16,0,z*16)).setLoaded(true);} }
    final net.minecraft.world.level.entity.EntityLookup<net.minecraft.world.entity.Entity> actors=new net.minecraft.world.level.entity.EntityLookup<>();
    final net.minecraft.world.level.entity.EntitySectionStorage<net.minecraft.world.entity.Entity> sections=new net.minecraft.world.level.entity.EntitySectionStorage<>(net.minecraft.world.entity.Entity.class,key->net.minecraft.world.level.entity.Visibility.TICKING);
    final net.minecraft.world.level.entity.LevelEntityGetter<net.minecraft.world.entity.Entity> getter=new net.minecraft.world.level.entity.LevelEntityGetterAdapter<>(actors,sections);
    final net.minecraft.world.ticks.LevelTicks<Block> blockTicks=new net.minecraft.world.ticks.LevelTicks<>(key->true);
    long subTick;
    @Override public long nextSubTickCount(){return subTick++;}
    @Override public net.minecraft.world.ticks.LevelTicks<Block> getBlockTicks(){return blockTicks;}
    @Override public void scheduleTick(BlockPos pos,Block block,int delay){blockTicks.schedule(createTick(pos,block,delay));}
    @Override public void scheduleTick(BlockPos pos,Block block,int delay,net.minecraft.world.ticks.TickPriority priority){blockTicks.schedule(createTick(pos,block,delay,priority));}
    final net.minecraft.world.TickRateManager clock=new net.minecraft.world.TickRateManager();
    final LinkedHashSet<net.minecraft.world.level.BlockEventData> events=new LinkedHashSet<>();
    @Override public net.minecraft.world.TickRateManager tickRateManager(){return clock;}
    final List<String> sounds=new ArrayList<>();
    final List<Integer> soundSources=new ArrayList<>();
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,double x,double y,double z,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){sounds.add(BuiltInRegistries.SOUND_EVENT.getId((net.minecraft.sounds.SoundEvent)sound.value())+":"+source.ordinal()+":"+Float.floatToRawIntBits(volume)+":"+Float.floatToRawIntBits(pitch)+":"+seed);soundSources.add(except==null?0:except.getId());}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,net.minecraft.world.entity.Entity entity,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){}
    final List<String> gameplay=new ArrayList<>();
    @Override public void gameEvent(Holder event,net.minecraft.world.phys.Vec3 pos,net.minecraft.world.level.gameevent.GameEvent.Context context){gameplay.add("G:"+BuiltInRegistries.GAME_EVENT.getId((net.minecraft.world.level.gameevent.GameEvent)event.value())+":"+(context.sourceEntity()==null?0:context.sourceEntity().getId())+":"+(context.affectedState()==null?-1:Block.getId(context.affectedState())));}
    @Override public void levelEvent(net.minecraft.world.entity.Entity source,int event,BlockPos pos,int data){gameplay.add("L:"+event+":"+data+":"+(source==null?0:source.getId()));}
    @Override public void blockEvent(BlockPos pos,Block block,int a,int b){events.add(new net.minecraft.world.level.BlockEventData(pos,block,a,b));}
    void drainEvents(){
        while(!events.isEmpty()){
            var event=events.removeFirst();var state=getBlockState(event.pos());
            if(state.is(event.block()))state.triggerEvent(this,event.pos(),event.paramA(),event.paramB());
        }
    }
    long time;
    @Override public net.minecraft.world.level.entity.LevelEntityGetter<net.minecraft.world.entity.Entity> getEntities(){return getter;}
    @Override public long getGameTime(){return time;}
    @Override public boolean isRaining(){return false;}
    @Override public java.util.Collection<net.minecraft.world.entity.boss.enderdragon.EnderDragonPart> dragonParts(){return List.of();}
    int nextActorId=1;
    @Override public boolean addFreshEntity(net.minecraft.world.entity.Entity entity){entity.setId(nextActorId++);actor(entity);return true;}
    void actor(net.minecraft.world.entity.Entity entity){actors.add(entity);sections.getOrCreateSection(SectionPos.asLong(entity.blockPosition())).add(entity);}

    @Override public net.minecraft.world.level.border.WorldBorder getWorldBorder() { return border; }
    @Override public void sendBlockUpdated(BlockPos pos, BlockState old, BlockState state, int flags) {
        if (recording) changes.add(pos.toShortString()+"="+Block.getId(state)+"/"+flags);
    }
    @Override public void updateNeighborsAt(BlockPos pos, Block block) {
        updateNeighborsAt(pos,block,ExperimentalRedstoneUtils.initialOrientation(this,null,null));
    }
    @Override public void updateNeighborsAt(BlockPos pos, Block block, Orientation orientation) {
        if (recording) notifications.add(pos.toShortString()+"/"+(orientation==null?-1:orientation.getIndex()));
        neighborUpdater.updateNeighborsAtExceptFromFacing(pos,block,null,orientation);
    }
    @Override public void updateNeighborsAtExceptFromFacing(BlockPos pos, Block block, Direction skip, Orientation orientation) {
        neighborUpdater.updateNeighborsAtExceptFromFacing(pos,block,skip,orientation);
    }
    @Override public void neighborChanged(BlockPos pos, Block block, Orientation orientation) { neighborUpdater.neighborChanged(pos,block,orientation); }
    @Override public void neighborChanged(BlockState state,BlockPos pos,Block block,Orientation orientation,boolean moved) { neighborUpdater.neighborChanged(state,pos,block,orientation,moved); }
    void raw(BlockPos pos,BlockState state) { getChunkAt(pos).setBlockState(pos,state,816); }
    void begin() { recording=true; changes.clear(); notifications.clear(); trace.clear(); }
    static String positions(List<BlockPos> list) { return String.join(";",list.stream().map(BlockPos::toShortString).toList()); }
    static final BlockPos ORIGIN = new BlockPos(8,0,8);
    static void signals(RegistryAccess r) {
        var sources=new ArrayList<Block>();
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof PoweredBlock||block instanceof RedstoneTorchBlock||block instanceof ObserverBlock||block instanceof DiodeBlock||block instanceof LeverBlock||block instanceof ButtonBlock||block instanceof BasePressurePlateBlock||block instanceof LightningRodBlock||block instanceof TripWireHookBlock||block instanceof TripWireBlock||block instanceof LecternBlock||block instanceof DaylightDetectorBlock||block instanceof SculkSensorBlock||block instanceof TargetBlock||block instanceof RedstoneWireBlock)sources.add(block);
        for (var block:sources) {
            for (var state:block.getStateDefinition().getPossibleStates()) {
                // Every direction, all powered/orientation/delay/button states.
                var level=new RedstoneVectors(r);level.raw(ORIGIN,state);
                var values=new ArrayList<String>();
                values.add(""+state.isSignalSource());values.add(""+state.getOwnSignal(level,ORIGIN));
                for(var d:Direction.values()) {
                    values.add(level.getSignal(ORIGIN,d)+":"+level.getDirectSignal(ORIGIN,d)+":"+level.getControlInputSignal(ORIGIN,d,false)+":"+level.getControlInputSignal(ORIGIN,d,true));
                }
                System.out.println("signal:"+Block.getId(state)+"\t"+String.join(";",values));
            }
        }
        for(var state:Blocks.COMPARATOR.getStateDefinition().getPossibleStates())for(int output:new int[]{Integer.MIN_VALUE,-1,0,15,16,Integer.MAX_VALUE}){
            var level=new RedstoneVectors(r);level.raw(ORIGIN,state);
            ((net.minecraft.world.level.block.entity.ComparatorBlockEntity)level.getBlockEntity(ORIGIN)).setOutputSignal(output);
            var receiver=ORIGIN.relative(state.getValue(ComparatorBlock.FACING).getOpposite());level.raw(receiver,Blocks.STONE.defaultBlockState());
            var values=new ArrayList<String>();values.add(""+state.isSignalSource());values.add(""+state.getOwnSignal(level,ORIGIN));
            for(var d:Direction.values())values.add(level.getSignal(ORIGIN,d)+":"+level.getDirectSignal(ORIGIN,d)+":"+level.getControlInputSignal(ORIGIN,d,false)+":"+level.getControlInputSignal(ORIGIN,d,true));
            System.out.println("stored-signal:"+Block.getId(state)+":"+output+"\t"+String.join(";",values)+"|"+level.getSignal(receiver,Direction.DOWN)+":"+level.getDirectSignalTo(receiver)+":"+level.getBestNeighborSignal(receiver)+":"+level.hasNeighborSignal(receiver)+":"+level.getBestOwnOrNeighbourSignal(ORIGIN));
        }
        for(var source:List.of(Blocks.REDSTONE_BLOCK.defaultBlockState(),Blocks.REDSTONE_TORCH.defaultBlockState(),Blocks.OBSERVER.defaultBlockState().setValue(ObserverBlock.POWERED,true)))for(var d:Direction.values()) {
            var level=new RedstoneVectors(r);level.raw(ORIGIN,Blocks.STONE.defaultBlockState());level.raw(ORIGIN.relative(d),source);
            System.out.println("relay:"+Block.getId(source)+":"+d.get3DDataValue()+"\t"+level.getSignal(ORIGIN,Direction.DOWN)+":"+level.getDirectSignalTo(ORIGIN)+":"+level.getBestNeighborSignal(ORIGIN)+":"+level.hasNeighborSignal(ORIGIN));
        }
    }
    static void piston(RegistryAccess r) {
        for(boolean extending:new boolean[]{true,false})for(var direction:Direction.values())for(int length:new int[]{0,1,11,12,13}) {
            var level=new RedstoneVectors(r);var piston=ORIGIN;
            for(int i=0;i<length;i++)level.raw(piston.relative(direction,i+(extending?1:2)),Blocks.STONE.defaultBlockState());
            var resolver=new PistonStructureResolver(level,piston,direction,extending);boolean ok=resolver.resolve();
            System.out.println("piston-line:"+direction.get3DDataValue()+":"+extending+":"+length+"\t"+ok+"|"+positions(resolver.getToPush())+"|"+positions(resolver.getToDestroy()));
        }
        for(var sticky:List.of(Blocks.SLIME_BLOCK,Blocks.HONEY_BLOCK))for(int kind=0;kind<6;kind++) {
            var level=new RedstoneVectors(r);var s=ORIGIN.east();level.raw(s,sticky.defaultBlockState());
            level.raw(s.above(),(kind==1?Blocks.OBSIDIAN:kind==2?Blocks.REDSTONE_WIRE:kind==3?(sticky==Blocks.SLIME_BLOCK?Blocks.HONEY_BLOCK:Blocks.SLIME_BLOCK):sticky).defaultBlockState());
            level.raw(s.below(),Blocks.STONE.defaultBlockState());level.raw(s.north(),Blocks.STONE.defaultBlockState());level.raw(s.south(),Blocks.STONE.defaultBlockState());
            if(kind==4){level.raw(s.east(),sticky.defaultBlockState());level.raw(s.above().east(),sticky.defaultBlockState());level.raw(s.above().east(2),sticky.defaultBlockState());}
            if(kind==5)for(int i=0;i<12;i++)level.raw(s.south(i+1),Blocks.STONE.defaultBlockState());
            var resolver=new PistonStructureResolver(level,ORIGIN,Direction.EAST,true);boolean ok=resolver.resolve();
            System.out.println("piston-branch:"+(sticky==Blocks.SLIME_BLOCK?"slime":"honey")+":"+kind+"\t"+ok+"|"+positions(resolver.getToPush())+"|"+positions(resolver.getToDestroy()));
        }
        var level=new RedstoneVectors(r);level.border.setCenter(8,8);level.border.setSize(10);
        var states=new ArrayList<BlockState>();
        for(var block:List.of(Blocks.AIR,Blocks.STONE,Blocks.OBSIDIAN,Blocks.BEDROCK,Blocks.BARRIER,Blocks.REDSTONE_WIRE,Blocks.SLIME_BLOCK,Blocks.HONEY_BLOCK,Blocks.CHEST,Blocks.FURNACE,Blocks.HOPPER,Blocks.DISPENSER,Blocks.DROPPER,Blocks.COMPARATOR,Blocks.MOVING_PISTON,Blocks.PISTON_HEAD,Blocks.PISTON,Blocks.STICKY_PISTON,Blocks.REINFORCED_DEEPSLATE,Blocks.GLAZED_TERRACOTTA.pick(net.minecraft.world.item.DyeColor.BLUE),Blocks.SHULKER_BOX,Blocks.LECTERN,Blocks.DAYLIGHT_DETECTOR))states.add(block.defaultBlockState());
        states.add(Blocks.PISTON.defaultBlockState().setValue(net.minecraft.world.level.block.piston.PistonBaseBlock.EXTENDED,true));
        states.add(Blocks.STICKY_PISTON.defaultBlockState().setValue(net.minecraft.world.level.block.piston.PistonBaseBlock.EXTENDED,true));
        var positions=new ArrayList<BlockPos>();positions.add(ORIGIN);
        for(int y:new int[]{level.getMinY()-1,level.getMinY(),level.getMinY()+1,level.getMaxY()-1,level.getMaxY(),level.getMaxY()+1})positions.add(ORIGIN.atY(y));
        for(int x:new int[]{2,3,12,13,14})positions.add(new BlockPos(x,0,8));
        for(var state:states)for(var pos:positions){
            if(!pos.equals(ORIGIN)&&!state.isAir()&&!state.is(Blocks.STONE)&&!state.is(Blocks.PISTON))continue;
            for(var direction:Direction.values())for(var connection:pos.equals(ORIGIN)?Direction.values():new Direction[]{direction})for(boolean allow:new boolean[]{false,true}){
                boolean result=net.minecraft.world.level.block.piston.PistonBaseBlock.isPushable(state,level,pos,direction,allow,connection);
                System.out.println("piston-pushable:"+Block.getId(state)+":"+direction.get3DDataValue()+":"+connection.get3DDataValue()+":"+allow+":"+pos.getX()+":"+pos.getY()+":"+pos.getZ()+"\t"+result);
            }
        }
    }
    static void wires(RegistryAccess r) {
        for(boolean experimental:new boolean[]{false,true})for(int shape=0;shape<3;shape++) {
            var level=new RedstoneVectors(r);level.experiments=experimental;level.random.setSeed(0);
            var wires=new ArrayList<BlockPos>();
            for(int x=0;x<7;x++)wires.add(ORIGIN.east(x));
            if(shape>0)for(int z=1;z<=3;z++){wires.add(ORIGIN.east(2).south(z));wires.add(ORIGIN.east(4).north(z));}
            if(shape==2){wires.add(ORIGIN.east(3).above());level.raw(ORIGIN.east(3),Blocks.STONE.defaultBlockState());wires.remove(ORIGIN.east(3));}
            for(var pos:wires){level.raw(pos.below(),Blocks.STONE.defaultBlockState());level.raw(pos,Blocks.REDSTONE_WIRE.defaultBlockState());}
            for(var pos:wires)level.raw(pos,Block.updateFromNeighbourShapes(level.getBlockState(pos),level,pos));
            for(int change=0;change<3;change++) {
                var power=ORIGIN.west();level.begin();
                level.setBlock(power,(change==1?Blocks.AIR:Blocks.REDSTONE_BLOCK).defaultBlockState(),3);
                var states=new ArrayList<String>();for(var pos:wires)states.add(pos.toShortString()+"="+Block.getId(level.getBlockState(pos)));
                System.out.println("wire:"+experimental+":"+shape+":"+change+"\t"+String.join(";",states)+"|"+String.join(";",level.changes)+"|"+String.join(";",level.notifications));
            }
        }
    }
    static void moving(RegistryAccess r) throws Exception {
        var progress=net.minecraft.world.level.block.piston.PistonMovingBlockEntity.class.getDeclaredField("progress");progress.setAccessible(true);
        var previous=net.minecraft.world.level.block.piston.PistonMovingBlockEntity.class.getDeclaredField("progressO");previous.setAccessible(true);
        var noClip=net.minecraft.world.level.block.piston.PistonMovingBlockEntity.class.getDeclaredField("NOCLIP");noClip.setAccessible(true);
        @SuppressWarnings("unchecked") var noclip=(ThreadLocal<Direction>)noClip.get(null);
        var level=new RedstoneVectors(r);
        for(var moved:List.of(Blocks.STONE.defaultBlockState(),Blocks.SLIME_BLOCK.defaultBlockState(),Blocks.PISTON.defaultBlockState(),Blocks.STICKY_PISTON.defaultBlockState()))for(var direction:Direction.values())for(boolean extending:new boolean[]{false,true})for(boolean source:new boolean[]{false,true})for(float value:new float[]{0.0F,0.25F,0.5F,0.75F,1.0F})for(boolean suppress:new boolean[]{false,true}){
            var movedState=moved.hasProperty(BlockStateProperties.FACING)?moved.setValue(BlockStateProperties.FACING,direction):moved;
            var entity=new net.minecraft.world.level.block.piston.PistonMovingBlockEntity(ORIGIN,Blocks.MOVING_PISTON.defaultBlockState(),movedState,direction,extending,source);
            progress.setFloat(entity,value);previous.setFloat(entity,value-0.25F);
            noclip.set(suppress?entity.getMovementDirection():null);
            var shape=entity.getCollisionShape(level,ORIGIN);var boxes=new ArrayList<String>();
            for(var box:shape.toAabbs())boxes.add(Long.toHexString(Double.doubleToRawLongBits(box.minX))+","+Long.toHexString(Double.doubleToRawLongBits(box.minY))+","+Long.toHexString(Double.doubleToRawLongBits(box.minZ))+","+Long.toHexString(Double.doubleToRawLongBits(box.maxX))+","+Long.toHexString(Double.doubleToRawLongBits(box.maxY))+","+Long.toHexString(Double.doubleToRawLongBits(box.maxZ)));
            var samples=new ArrayList<String>();for(float partial:new float[]{-0.25F,0.0F,0.5F,1.0F,2.0F})samples.add(Integer.toHexString(Float.floatToRawIntBits(entity.getProgress(partial))));
            System.out.println("moving:"+Block.getId(movedState)+":"+direction.get3DDataValue()+":"+extending+":"+source+":"+value+":"+suppress+"\t"+String.join(";",boxes)+"|"+String.join(",",samples));
        }
        noclip.set(null);
    }
    static void pushing(RegistryAccess r) throws Exception {
        for(var moved:List.of(Blocks.STONE,Blocks.SLIME_BLOCK,Blocks.HONEY_BLOCK))for(boolean stuck:new boolean[]{false,true}) {
            var level=new RedstoneVectors(r);var state=Blocks.MOVING_PISTON.defaultBlockState().setValue(BlockStateProperties.FACING,Direction.EAST);level.raw(ORIGIN,state);
            var entity=new net.minecraft.world.level.block.piston.PistonMovingBlockEntity(ORIGIN,state,moved.defaultBlockState(),Direction.EAST,true,false);
            level.getChunkAt(ORIGIN).setBlockEntity(entity);
            var actor=new net.minecraft.world.entity.item.PrimedTnt(net.minecraft.world.entity.EntityTypes.TNT,level);
            actor.setId(1);actor.setPos(ORIGIN.getX()+(stuck?-0.4:0.1),ORIGIN.getY()+(stuck?0.9375:0.1),ORIGIN.getZ()+0.5);actor.setOnGround(stuck);level.actor(actor);
            var samples=new ArrayList<String>();
            for(int tick=1;tick<=3;tick++){
                level.time=tick;net.minecraft.world.level.block.piston.PistonMovingBlockEntity.tick(level,ORIGIN,state,entity);
                var position=actor.position();var velocity=actor.getDeltaMovement();
                samples.add(Long.toHexString(Double.doubleToRawLongBits(position.x))+","+Long.toHexString(Double.doubleToRawLongBits(position.y))+","+Long.toHexString(Double.doubleToRawLongBits(position.z))+"/"+Long.toHexString(Double.doubleToRawLongBits(velocity.x))+","+Long.toHexString(Double.doubleToRawLongBits(velocity.y))+","+Long.toHexString(Double.doubleToRawLongBits(velocity.z))+"/"+Block.getId(level.getBlockState(ORIGIN)));
            }
            System.out.println("push:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getId(moved)+":"+stuck+"\t"+String.join(";",samples));
        }
    }
    static void comparators(RegistryAccess r) throws Exception {
        var calculate=ComparatorBlock.class.getDeclaredMethod("calculateOutputSignal",net.minecraft.world.level.Level.class,BlockPos.class,BlockState.class);calculate.setAccessible(true);
        var should=ComparatorBlock.class.getDeclaredMethod("shouldTurnOn",net.minecraft.world.level.Level.class,BlockPos.class,BlockState.class);should.setAccessible(true);
        for(var direction:Direction.Plane.HORIZONTAL) {
            var level=new RedstoneVectors(r);
            for(var mode:ComparatorMode.values())for(int main=0;main<=15;main++)for(int side=0;side<=15;side++) {
                var state=Blocks.COMPARATOR.defaultBlockState().setValue(ComparatorBlock.FACING,direction).setValue(ComparatorBlock.MODE,mode);
                level.raw(ORIGIN,state);
                level.raw(ORIGIN.relative(direction),Blocks.REDSTONE_WIRE.defaultBlockState().setValue(RedstoneWireBlock.POWER,main));
                level.raw(ORIGIN.relative(direction.getClockWise()),Blocks.REDSTONE_WIRE.defaultBlockState().setValue(RedstoneWireBlock.POWER,side));
                System.out.println("comparator:"+Block.getId(state)+":"+main+":"+side+"\t"+calculate.invoke(Blocks.COMPARATOR,level,ORIGIN,state)+":"+should.invoke(Blocks.COMPARATOR,level,ORIGIN,state));
            }
        }
    }
    static void placements(RegistryAccess r) {
        var blocks=new ArrayList<>(List.<Block>of(Blocks.REDSTONE_WIRE,Blocks.REPEATER,Blocks.COMPARATOR,Blocks.OBSERVER,Blocks.REDSTONE_WALL_TORCH,Blocks.LEVER,Blocks.STONE_BUTTON,Blocks.OAK_BUTTON,Blocks.PISTON,Blocks.STICKY_PISTON,Blocks.DISPENSER,Blocks.DROPPER,Blocks.HOPPER,Blocks.OAK_DOOR,Blocks.IRON_DOOR,Blocks.COPPER_DOOR.weathering().unaffected(),Blocks.OAK_TRAPDOOR,Blocks.IRON_TRAPDOOR,Blocks.COPPER_TRAPDOOR.weathering().unaffected(),Blocks.OAK_FENCE_GATE,Blocks.REDSTONE_LAMP,Blocks.RAIL,Blocks.POWERED_RAIL,Blocks.ACTIVATOR_RAIL,Blocks.DETECTOR_RAIL,Blocks.NOTE_BLOCK,Blocks.TRIPWIRE,Blocks.TRIPWIRE_HOOK));
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof LightningRodBlock)blocks.add(block);
        for(var block:blocks)for(var direction:Direction.values())for(var face:Direction.values())for(int layout=0;layout<3;layout++) {
            var level=new RedstoneVectors(r);level.raw(ORIGIN.below(),Blocks.STONE.defaultBlockState());
            if(layout==1)level.raw(ORIGIN.relative(direction),Blocks.STONE.defaultBlockState());
            if(layout==2)level.raw(ORIGIN.relative(direction),Blocks.REDSTONE_BLOCK.defaultBlockState());
            var context=new net.minecraft.world.item.context.DirectionalPlaceContext(level,ORIGIN,direction,net.minecraft.world.item.ItemStack.EMPTY,face);
            var state=block.getStateForPlacement(context);
            System.out.println("placement:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getId(block)+":"+direction.get3DDataValue()+":"+face.get3DDataValue()+":"+layout+"\t"+(state==null?-1:Block.getId(state)));
        }
    }
    static void hoppers(RegistryAccess r) throws Exception {
        var cooldown=net.minecraft.world.level.block.entity.HopperBlockEntity.class.getDeclaredField("cooldownTime");cooldown.setAccessible(true);
        for(boolean reverse:new boolean[]{false,true})for(boolean locked:new boolean[]{false,true})for(int amount:new int[]{8,64}) {
            var level=new RedstoneVectors(r);var hoppers=new ArrayList<net.minecraft.world.level.block.entity.HopperBlockEntity>();
            for(int y=0;y<3;y++) {
                var pos=ORIGIN.below(y);var state=Blocks.HOPPER.defaultBlockState();level.raw(pos,state);
                var hopper=new net.minecraft.world.level.block.entity.HopperBlockEntity(pos,state);level.getChunkAt(pos).setBlockEntity(hopper);hoppers.add(hopper);
            }
            var bottom=ORIGIN.below(3);var barrelState=Blocks.BARREL.defaultBlockState();level.raw(bottom,barrelState);
            var barrel=new net.minecraft.world.level.block.entity.BarrelBlockEntity(bottom,barrelState);level.getChunkAt(bottom).setBlockEntity(barrel);
            hoppers.get(0).setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,amount));
            var steps=new ArrayList<String>();
            for(int tick=1;tick<=20;tick++) {
                level.time=tick;
                if(locked && (tick==4 || tick==11))level.setBlock(ORIGIN.below().east(),(tick==4?Blocks.REDSTONE_BLOCK:Blocks.AIR).defaultBlockState(),3);
                for(int index=0;index<3;index++) {
                    int i=reverse?2-index:index;var hopper=hoppers.get(i);var pos=ORIGIN.below(i);net.minecraft.world.level.block.entity.HopperBlockEntity.pushItemsTick(level,pos,level.getBlockState(pos),hopper);
                    var values=new ArrayList<String>();
                    for(int j=0;j<3;j++){var h=hoppers.get(j);var items=new ArrayList<String>();for(int slot=0;slot<5;slot++)items.add(""+h.getItem(slot).getCount());values.add(String.join(",",items)+"/"+cooldown.getInt(h)+"/"+level.getBlockState(ORIGIN.below(j)).getValue(HopperBlock.ENABLED));}
                    steps.add(String.join(";",values)+";"+barrel.getItem(0).getCount());
                }
            }
            System.out.println("hopper:"+reverse+":"+locked+":"+amount+"\t"+String.join("|",steps));
        }
    }
    static void pistonTimelines(RegistryAccess r) {
        for(boolean sticky:new boolean[]{false,true})for(boolean quasi:new boolean[]{false,true})for(int length:new int[]{0,1,12,13})for(int off:new int[]{0,1,2,4}) {
            var level=new RedstoneVectors(r);var piston=(sticky?Blocks.STICKY_PISTON:Blocks.PISTON).defaultBlockState().setValue(BlockStateProperties.FACING,Direction.EAST);level.raw(ORIGIN,piston);
            for(int i=1;i<=length;i++)level.raw(ORIGIN.east(i),Blocks.STONE.defaultBlockState());
            var power=quasi?ORIGIN.above(2):ORIGIN.below();var steps=new ArrayList<String>();
            level.begin();level.setBlock(power,Blocks.REDSTONE_BLOCK.defaultBlockState(),3);
            if(quasi){steps.add(pistonSnapshot(level));level.neighborChanged(ORIGIN,Blocks.STONE,null);}
            level.drainEvents();steps.add(pistonSnapshot(level));
            if(off==0){level.setBlock(power,Blocks.AIR.defaultBlockState(),3);if(quasi)level.neighborChanged(ORIGIN,Blocks.STONE,null);level.drainEvents();steps.add(pistonSnapshot(level));}
            for(int tick=1;tick<=7;tick++){
                level.time=tick;level.tickBlockEntities();if(tick==3&&off==4&&length<=12&&!level.getBlockState(ORIGIN.east()).is(Blocks.PISTON_HEAD))throw new AssertionError("moving piston ticker not registered");steps.add(pistonSnapshot(level));
                if(tick==off){level.setBlock(power,Blocks.AIR.defaultBlockState(),3);if(quasi)level.neighborChanged(ORIGIN,Blocks.STONE,null);level.drainEvents();steps.add(pistonSnapshot(level));}
            }
            System.out.println("piston-timeline:"+sticky+":"+quasi+":"+length+":"+off+"\t"+String.join("|",steps)+"#"+String.join(";",level.changes));
        }
    }
    static String pistonSnapshot(RedstoneVectors level) {
        var states=new ArrayList<String>();for(int i=0;i<=14;i++)states.add(""+Block.getId(level.getBlockState(ORIGIN.east(i))));return String.join(",",states);
    }
    static void dispensing(RegistryAccess r) {
        for(long seed:new long[]{0,1,12345})for(var direction:Direction.values()) {
            var level=new RedstoneVectors(r);level.random.setSeed(seed);
            var state=Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING,direction);
            var source=new net.minecraft.core.dispenser.BlockSource(null,ORIGIN,state,null);
            net.minecraft.core.dispenser.DefaultDispenseItemBehavior.spawnItem(level,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE),6,direction,DispenserBlock.getDispensePosition(source));
            var list=new ArrayList<net.minecraft.world.entity.Entity>();level.getEntities().getAll().forEach(list::add);var item=list.getFirst();
            var pos=item.position();var v=item.getDeltaMovement();
            System.out.println("dispense-item:"+seed+":"+direction.get3DDataValue()+"\t"+Long.toHexString(Double.doubleToRawLongBits(pos.x))+","+Long.toHexString(Double.doubleToRawLongBits(pos.y))+","+Long.toHexString(Double.doubleToRawLongBits(pos.z))+"/"+Long.toHexString(Double.doubleToRawLongBits(v.x))+","+Long.toHexString(Double.doubleToRawLongBits(v.y))+","+Long.toHexString(Double.doubleToRawLongBits(v.z))+"/"+level.random.nextLong());
        }
        for(var item:net.minecraft.core.registries.BuiltInRegistries.ITEM)if(item instanceof net.minecraft.world.item.ProjectileItem projectile)for(var direction:Direction.values()) {
            var config=projectile.createDispenseConfig();var state=Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING,direction);
            var source=new net.minecraft.core.dispenser.BlockSource(null,ORIGIN,state,null);var pos=config.positionFunction().getDispensePosition(source,direction);
            System.out.println("projectile-config:"+net.minecraft.core.registries.BuiltInRegistries.ITEM.getId(item)+":"+direction.get3DDataValue()+"\t"+Long.toHexString(Double.doubleToRawLongBits(pos.x()))+","+Long.toHexString(Double.doubleToRawLongBits(pos.y()))+","+Long.toHexString(Double.doubleToRawLongBits(pos.z()))+"/"+Integer.toHexString(Float.floatToRawIntBits(config.power()))+","+Integer.toHexString(Float.floatToRawIntBits(config.uncertainty()))+"/"+config.overrideDispenseEvent().map(Object::toString).orElse("none"));
        }
    }
    static void dispenserTriggers(RegistryAccess r) {
        for(var block:List.of(Blocks.DISPENSER,Blocks.DROPPER))for(int layout=0;layout<4;layout++) {
            var level=new RedstoneVectors(r);var state=block.defaultBlockState();level.raw(ORIGIN,state);
            var power=switch(layout){case 0->ORIGIN.below();case 1->ORIGIN.above();case 2->ORIGIN.above(2);default->ORIGIN.east();};
            var samples=new ArrayList<String>();
            for(int tick=0;tick<=7;tick++) {
                level.time=tick;
                if(tick==0||tick==1||tick==2||tick==6)level.setBlock(power,(tick==0||tick==2?Blocks.REDSTONE_BLOCK:Blocks.AIR).defaultBlockState(),3);
                String before=level.getBlockState(ORIGIN).getValue(DispenserBlock.TRIGGERED)+"/"+level.blockTicks.hasScheduledTick(ORIGIN,block);
                level.neighborChanged(ORIGIN,Blocks.STONE,null);
                String poked=level.getBlockState(ORIGIN).getValue(DispenserBlock.TRIGGERED)+"/"+level.blockTicks.hasScheduledTick(ORIGIN,block);
                level.blockTicks.tick(tick,65536,(p,b)->{});
                samples.add(before+";"+poked+";"+level.blockTicks.hasScheduledTick(ORIGIN,block));
            }
            System.out.println("dispenser-trigger:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getId(block)+":"+layout+"\t"+String.join("|",samples));
        }
    }
    static void rails(RegistryAccess r) throws Exception {
        var directions=List.of(Direction.NORTH,Direction.SOUTH,Direction.WEST,Direction.EAST);
        for(var block:List.of(Blocks.RAIL,Blocks.POWERED_RAIL,Blocks.ACTIVATOR_RAIL))for(int layout=0;layout<256;layout++)for(boolean powered:new boolean[]{false,true})for(boolean eastWest:new boolean[]{false,true}) {
            var level=new RedstoneVectors(r);var positions=new ArrayList<BlockPos>();positions.add(ORIGIN);
            level.raw(ORIGIN.below(),Blocks.STONE.defaultBlockState());
            int pattern=layout;
            for(var direction:directions){
                int value=pattern&3;pattern>>=2;
                var pos=ORIGIN.relative(direction).above(value==1?-1:value==3?1:0);positions.add(pos);
                if(value!=0){level.raw(pos.below(),Blocks.STONE.defaultBlockState());level.raw(pos,block.defaultBlockState());}
            }
            var property=((BaseRailBlock)block).getShapeProperty();var initial=eastWest?RailShape.EAST_WEST:RailShape.NORTH_SOUTH;
            var state=block.defaultBlockState().setValue(property,initial);level.raw(ORIGIN,state);level.begin();
            var rail=new RailState(level,ORIGIN,state).place(powered,true,initial);
            var states=new ArrayList<String>();for(var pos:positions)states.add(pos.toShortString()+"="+Block.getId(level.getBlockState(pos)));
            System.out.println("rail-place:"+BuiltInRegistries.BLOCK.getKey(block).getPath()+":"+layout+":"+powered+":"+eastWest+"\t"+Block.getId(rail.getState())+"|"+positions(rail.getConnections())+"|"+String.join(";",states)+"|"+String.join(";",level.changes));
        }
        var finder=PoweredRailBlock.class.getDeclaredMethod("findPoweredRailSignal",net.minecraft.world.level.Level.class,BlockPos.class,BlockState.class,boolean.class,int.class);finder.setAccessible(true);
        for(var block:List.of(Blocks.POWERED_RAIL,Blocks.ACTIVATOR_RAIL))for(var direction:directions)for(int length=1;length<=10;length++)for(int obstruction=0;obstruction<4;obstruction++) {
            var level=new RedstoneVectors(r);var property=((BaseRailBlock)block).getShapeProperty();var railShape=direction.getAxis()==Direction.Axis.X?RailShape.EAST_WEST:RailShape.NORTH_SOUTH;
            var state=block.defaultBlockState().setValue(property,railShape).setValue(PoweredRailBlock.POWERED,true);
            for(int i=0;i<=length;i++)level.raw(ORIGIN.relative(direction,i),state);
            if(obstruction>0){var middle=ORIGIN.relative(direction,(length+1)/2);level.raw(middle,obstruction==1?state.setValue(PoweredRailBlock.POWERED,false):obstruction==2?state.setValue(property,railShape==RailShape.EAST_WEST?RailShape.NORTH_SOUTH:RailShape.EAST_WEST):(block==Blocks.ACTIVATOR_RAIL?Blocks.POWERED_RAIL:Blocks.ACTIVATOR_RAIL).defaultBlockState().setValue(property,railShape).setValue(PoweredRailBlock.POWERED,true));}
            level.raw(ORIGIN.relative(direction,length).above(),Blocks.REDSTONE_BLOCK.defaultBlockState());
            var values=new ArrayList<String>();for(int depth=0;depth<=8;depth++)values.add(""+finder.invoke(block,level,ORIGIN,state,direction==Direction.SOUTH||direction==Direction.WEST,depth));
            System.out.println("rail-find:"+BuiltInRegistries.BLOCK.getKey(block).getPath()+":"+direction.get3DDataValue()+":"+length+":"+obstruction+"\t"+String.join(";",values));
        }
        for(var block:List.of(Blocks.POWERED_RAIL,Blocks.ACTIVATOR_RAIL))for(var direction:directions) {
            var level=new RedstoneVectors(r);var property=((BaseRailBlock)block).getShapeProperty();var railShape=direction.getAxis()==Direction.Axis.X?RailShape.EAST_WEST:RailShape.NORTH_SOUTH;
            var positions=new ArrayList<BlockPos>();for(int i=0;i<13;i++){var pos=ORIGIN.relative(direction,i);positions.add(pos);level.raw(pos.below(),Blocks.STONE.defaultBlockState());level.raw(pos,block.defaultBlockState().setValue(property,railShape));}
            for(int change=0;change<3;change++){
                level.begin();level.setBlock(ORIGIN.relative(direction.getOpposite()),(change==1?Blocks.AIR:Blocks.REDSTONE_BLOCK).defaultBlockState(),3);
                var states=new ArrayList<String>();for(var pos:positions)states.add(pos.toShortString()+"="+Block.getId(level.getBlockState(pos)));
                System.out.println("rail-power:"+BuiltInRegistries.BLOCK.getKey(block).getPath()+":"+direction.get3DDataValue()+":"+change+"\t"+String.join(";",states)+"|"+String.join(";",level.changes));
            }
        }
    }
    static void notes(RegistryAccess r) {
        var representatives=new LinkedHashMap<NoteBlockInstrument,Block>();
        representatives.put(NoteBlockInstrument.HARP,Blocks.AIR);
        for(var block:BuiltInRegistries.BLOCK)representatives.putIfAbsent(block.defaultBlockState().instrument(),block);
        for(var below:representatives.values())for(var above:representatives.values()){
            var level=new RedstoneVectors(r);level.raw(ORIGIN.below(),below.defaultBlockState());level.raw(ORIGIN.above(),above.defaultBlockState());
            var placement=new net.minecraft.world.item.context.DirectionalPlaceContext(level,ORIGIN,Direction.DOWN,net.minecraft.world.item.ItemStack.EMPTY,Direction.UP);
            var state=Blocks.NOTE_BLOCK.getStateForPlacement(placement);
            System.out.println("note-instrument:"+BuiltInRegistries.BLOCK.getId(below)+":"+BuiltInRegistries.BLOCK.getId(above)+"\t"+Block.getId(state));
        }
        for(var instrument:NoteBlockInstrument.values())for(int note=0;note<=24;note++){
            var level=new RedstoneVectors(r);level.random.setSeed(0);
            var state=Blocks.NOTE_BLOCK.defaultBlockState().setValue(NoteBlock.INSTRUMENT,instrument).setValue(NoteBlock.NOTE,note);level.raw(ORIGIN,state);
            var played=state.triggerEvent(level,ORIGIN,0,0);
            System.out.println("note-event:"+instrument.getSerializedName()+":"+note+"\t"+played+"|"+String.join(";",level.sounds)+"|"+level.random.nextLong());
        }
        for(var instrument:NoteBlockInstrument.values())for(boolean blocked:new boolean[]{false,true}){
            var level=new RedstoneVectors(r);level.random.setSeed(0);var state=Blocks.NOTE_BLOCK.defaultBlockState().setValue(NoteBlock.INSTRUMENT,instrument);level.raw(ORIGIN,state);
            if(blocked)level.raw(ORIGIN.above(),Blocks.STONE.defaultBlockState());
            for(int change=0;change<3;change++){
                level.begin();level.sounds.clear();level.setBlock(ORIGIN.west(),(change==1?Blocks.AIR:Blocks.REDSTONE_BLOCK).defaultBlockState(),3);level.drainEvents();
                System.out.println("note-power:"+instrument.getSerializedName()+":"+blocked+":"+change+"\t"+Block.getId(level.getBlockState(ORIGIN))+"|"+String.join(";",level.sounds));
            }
        }
    }
    static RedstoneVectors tripwireLevel(RegistryAccess r){
        var level=new RedstoneVectors(r);
        for(int x=-3;x<=3;x++)for(int z=-3;z<=3;z++)if(x < -1 || x>1 || z < -1 || z>1){level.blockTicks.addContainer(new net.minecraft.world.level.ChunkPos(x,z),new net.minecraft.world.ticks.LevelChunkTicks<>());level.getChunkAt(new BlockPos(x*16,0,z*16)).setLoaded(true);}
        return level;
    }
    static void tripwires(RegistryAccess r) throws Exception {
        for(var direction:Direction.Plane.HORIZONTAL)for(int length:new int[]{1,2,3,16,41,42})for(int mode=0;mode<12;mode++){
            var level=tripwireLevel(r);level.random.setSeed(0);int middle=Math.max(1,length/2);
            boolean attached=mode==4||mode==5;boolean powered=mode==5;
            var state=Blocks.TRIPWIRE_HOOK.defaultBlockState().setValue(TripWireHookBlock.FACING,direction).setValue(TripWireHookBlock.ATTACHED,attached).setValue(TripWireHookBlock.POWERED,powered);
            level.raw(ORIGIN.relative(direction.getOpposite()),Blocks.STONE.defaultBlockState());level.raw(ORIGIN.relative(direction,length+1),Blocks.STONE.defaultBlockState());
            var positions=new ArrayList<BlockPos>();
            for(int i=0;i<=length;i++){var pos=ORIGIN.relative(direction,i);positions.add(pos);level.raw(pos,i==0?state:i==length?state.setValue(TripWireHookBlock.FACING,mode==8?direction:direction.getOpposite()):Blocks.TRIPWIRE.defaultBlockState().setValue(TripWireBlock.ATTACHED,attached).setValue(TripWireBlock.POWERED,(mode==1||mode==2)&&i==middle).setValue(TripWireBlock.DISARMED,mode==2&&i==middle));}
            if(mode==3||mode==7||mode==10)level.raw(ORIGIN.relative(direction,middle),Blocks.AIR.defaultBlockState());
            if(mode==6)level.raw(ORIGIN,Blocks.AIR.defaultBlockState());if(mode==9)level.raw(ORIGIN.relative(direction,length),Blocks.AIR.defaultBlockState());
            var override=Blocks.TRIPWIRE.defaultBlockState().setValue(TripWireBlock.POWERED,true).setValue(TripWireBlock.DISARMED,mode==10);
            level.begin();TripWireHookBlock.calculateState(level,ORIGIN,state,mode==6,true,mode==7||mode==10?middle:-1,override);
            var states=new ArrayList<String>();for(var pos:positions)states.add(pos.toShortString()+"="+Block.getId(level.getBlockState(pos)));
            System.out.println("tripwire-calc:"+direction.get3DDataValue()+":"+length+":"+mode+"\t"+String.join(";",states)+"|"+String.join(";",level.changes)+"|"+level.blockTicks.hasScheduledTick(ORIGIN,Blocks.TRIPWIRE_HOOK)+":"+level.blockTicks.hasScheduledTick(ORIGIN.relative(direction,length),Blocks.TRIPWIRE_HOOK)+"|"+level.random.nextLong());
        }
        var check=TripWireBlock.class.getDeclaredMethod("checkPressed",net.minecraft.world.level.Level.class,BlockPos.class);check.setAccessible(true);
        for(var direction:Direction.Plane.HORIZONTAL){
            var level=tripwireLevel(r);level.random.setSeed(0);var positions=new ArrayList<BlockPos>();
            var hook=Blocks.TRIPWIRE_HOOK.defaultBlockState().setValue(TripWireHookBlock.FACING,direction);
            level.raw(ORIGIN.relative(direction.getOpposite()),Blocks.STONE.defaultBlockState());level.raw(ORIGIN.relative(direction,5),Blocks.STONE.defaultBlockState());
            for(int i=0;i<=4;i++){var pos=ORIGIN.relative(direction,i);positions.add(pos);level.raw(pos,i==0?hook:i==4?hook.setValue(TripWireHookBlock.FACING,direction.getOpposite()):Blocks.TRIPWIRE.defaultBlockState());}
            TripWireHookBlock.calculateState(level,ORIGIN,hook,false,true,-1,null);
            var pos=ORIGIN.relative(direction,2);var actor=new net.minecraft.world.entity.item.PrimedTnt(net.minecraft.world.entity.EntityTypes.TNT,level);actor.setPos(pos.getX()+0.5,pos.getY()+0.1,pos.getZ()+0.5);level.addFreshEntity(actor);
            var samples=new ArrayList<String>();
            for(int tick=0;tick<=22;tick++){
                level.time=tick;level.begin();
                if(tick==0)level.getBlockState(pos).entityInside(level,pos,actor,net.minecraft.world.entity.InsideBlockEffectApplier.NOOP,true);
                if(tick==7){actor.discard();level.actors.remove(actor);level.sections.getOrCreateSection(SectionPos.asLong(actor.blockPosition())).remove(actor);}
                level.blockTicks.tick(tick,65536,(target,block)->{var current=level.getBlockState(target);if(current.is(block)){if(block==Blocks.TRIPWIRE_HOOK)TripWireHookBlock.calculateState(level,target,current,false,true,-1,null);else if(block==Blocks.TRIPWIRE&&current.getValue(TripWireBlock.POWERED))try{check.invoke(block,level,target);}catch(Exception error){throw new RuntimeException(error);}}});
                var states=new ArrayList<String>();for(var target:positions)states.add(target.toShortString()+"="+Block.getId(level.getBlockState(target)));
                samples.add(String.join(";",states)+"/"+level.blockTicks.hasScheduledTick(pos,Blocks.TRIPWIRE)+"/"+level.blockTicks.hasScheduledTick(ORIGIN,Blocks.TRIPWIRE_HOOK)+"/"+String.join(";",level.changes));
            }
            System.out.println("tripwire-timeline:"+direction.get3DDataValue()+"\t"+String.join("|",samples));
        }
    }
    static final class RedstonePlayer extends net.minecraft.world.entity.player.Player {
        net.minecraft.world.level.GameType mode=net.minecraft.world.level.GameType.SURVIVAL;
        RedstonePlayer(net.minecraft.world.level.Level level){super(level,new com.mojang.authlib.GameProfile(new java.util.UUID(1,2),"redstone"));}
        @Override public net.minecraft.world.level.GameType gameMode(){return mode;}
    }
    static void plates(RegistryAccess r)throws Exception{
        var check=BasePressurePlateBlock.class.getDeclaredMethod("checkPressed",net.minecraft.world.entity.Entity.class,net.minecraft.world.level.Level.class,BlockPos.class,BlockState.class,int.class);check.setAccessible(true);
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof BasePressurePlateBlock){
            boolean weighted=block instanceof WeightedPressurePlateBlock;
            for(int kind=0;kind<3;kind++)for(int count:weighted?new int[]{0,1,9,10,11,14,15,16,149,150,151}:new int[]{0,1}){
                var level=new RedstoneVectors(r);level.raw(ORIGIN.below(),Blocks.STONE.defaultBlockState());
                var state=weighted?block.defaultBlockState().setValue(WeightedPressurePlateBlock.POWER,count==0?15:0):block.defaultBlockState().setValue(PressurePlateBlock.POWERED,count==0);
                level.raw(ORIGIN,state);var actors=new ArrayList<net.minecraft.world.entity.Entity>();
                for(int i=0;i<count;i++){
                    net.minecraft.world.entity.Entity actor;
                    if(kind==0)actor=new net.minecraft.world.entity.item.PrimedTnt(net.minecraft.world.entity.EntityTypes.TNT,level);
                    else{var player=new RedstonePlayer(level);if(kind==2)player.mode=net.minecraft.world.level.GameType.SPECTATOR;actor=player;}
                    actor.setUUID(new java.util.UUID(1,i+2));actor.setPos(ORIGIN.getX()+0.5,ORIGIN.getY()+0.1,ORIGIN.getZ()+0.5);level.addFreshEntity(actor);actors.add(actor);
                }
                var samples=new ArrayList<String>();
                for(int tick=0;tick<=24;tick++){
                    level.time=tick;level.begin();level.gameplay.clear();
                    if(tick==0){if(count==0)check.invoke(block,null,level,ORIGIN,state,15);else state.entityInside(level,ORIGIN,actors.getFirst(),net.minecraft.world.entity.InsideBlockEffectApplier.NOOP,true);}
                    if(tick==7)for(var actor:actors){actor.discard();level.actors.remove(actor);level.sections.getOrCreateSection(SectionPos.asLong(actor.blockPosition())).remove(actor);}
                    // The private Level callback is exactly the body selected by
                    // BasePressurePlateBlock#tick after its positive-signal gate.
                    level.blockTicks.tick(tick,65536,(pos,b)->{
                        var current=level.getBlockState(pos);if(!current.is(b))return;
                        int signal=weighted?current.getValue(WeightedPressurePlateBlock.POWER):(current.getValue(PressurePlateBlock.POWERED)?15:0);
                        if(signal>0)try{check.invoke(b,null,level,pos,current,signal);}catch(Exception error){throw new RuntimeException(error);}
                    });
                    samples.add(Block.getId(level.getBlockState(ORIGIN))+"/"+level.blockTicks.hasScheduledTick(ORIGIN,block)+"/"+String.join(";",level.changes)+"/"+String.join(";",level.gameplay));
                }
                System.out.println("plate:"+BuiltInRegistries.BLOCK.getId(block)+":"+kind+":"+count+"\t"+String.join("|",samples));
            }
        }
    }
    static void interactions(RegistryAccess r){
        for(var block:BuiltInRegistries.BLOCK){
            boolean gate=block instanceof FenceGateBlock;
            if(!(gate||block instanceof DoorBlock||block instanceof TrapDoorBlock||block instanceof LeverBlock||block instanceof ButtonBlock))continue;
            for(var state:block.getStateDefinition().getPossibleStates()){
                if(block instanceof DoorBlock&&(state.getValue(DoorBlock.HALF)!=DoubleBlockHalf.LOWER||state.getValue(DoorBlock.FACING)!=Direction.NORTH||state.getValue(DoorBlock.HINGE)!=DoorHingeSide.LEFT))continue;
                if(block instanceof TrapDoorBlock&&(state.getValue(TrapDoorBlock.HALF)!=Half.BOTTOM||state.getValue(TrapDoorBlock.FACING)!=Direction.NORTH))continue;
                if(gate&&state.getValue(FenceGateBlock.IN_WALL))continue;
                for(int yaw:gate?new int[]{0,180}:new int[]{0}){
                    var level=new RedstoneVectors(r);level.raw(ORIGIN.below(),Blocks.STONE.defaultBlockState());
                    if(block instanceof DoorBlock)level.raw(ORIGIN.above(),state.setValue(DoorBlock.HALF,DoubleBlockHalf.UPPER));
                    if(block instanceof LeverBlock||block instanceof ButtonBlock){
                        var support=switch(state.getValue(FaceAttachedHorizontalDirectionalBlock.FACE)){
                            case FLOOR->Direction.DOWN;case CEILING->Direction.UP;case WALL->state.getValue(HorizontalDirectionalBlock.FACING).getOpposite();
                        };
                        level.raw(ORIGIN.relative(support),Blocks.STONE.defaultBlockState());
                    }
                    level.raw(ORIGIN,state);var player=new RedstonePlayer(level);player.setYRot(yaw);level.addFreshEntity(player);level.random.setSeed(123);
                    var samples=new ArrayList<String>();
                    for(int click=0;click<2;click++){
                        level.begin();level.sounds.clear();level.soundSources.clear();level.gameplay.clear();
                        var result=level.getBlockState(ORIGIN).useWithoutItem(level,player,new net.minecraft.world.phys.BlockHitResult(net.minecraft.world.phys.Vec3.atCenterOf(ORIGIN),Direction.UP,ORIGIN,false));
                        String outcome=result==net.minecraft.world.InteractionResult.SUCCESS?"success":result==net.minecraft.world.InteractionResult.CONSUME?"consume":result==net.minecraft.world.InteractionResult.PASS?"pass":result.toString();
                        var soundValues=new ArrayList<String>();for(int i=0;i<level.sounds.size();i++){String sound=level.sounds.get(i);soundValues.add(sound.substring(0,sound.lastIndexOf(':'))+":"+level.soundSources.get(i));}
                        samples.add(outcome+"|"+Block.getId(level.getBlockState(ORIGIN))+","+Block.getId(level.getBlockState(ORIGIN.above()))+"|"+String.join(";",level.changes)+"|"+String.join(";",level.gameplay)+"|"+String.join(";",soundValues)+"|"+level.blockTicks.hasScheduledTick(ORIGIN,block)+"|"+level.random.nextLong());
                    }
                    System.out.println("use:"+Block.getId(state)+":"+yaw+"\t"+String.join("#",samples));
                }
            }
        }
    }
    static void placedBy(RegistryAccess r){
        for(var block:List.of(Blocks.STONE,Blocks.REPEATER,Blocks.COMPARATOR,Blocks.PISTON,Blocks.STICKY_PISTON,Blocks.OAK_DOOR,Blocks.TRIPWIRE_HOOK))for(var state:block.getStateDefinition().getPossibleStates())for(int layout=0;layout<4;layout++){
            if(state.hasProperty(DoorBlock.HALF)&&state.getValue(DoorBlock.HALF)==DoubleBlockHalf.UPPER)continue;
            if(state.hasProperty(net.minecraft.world.level.block.piston.PistonBaseBlock.EXTENDED)&&state.getValue(net.minecraft.world.level.block.piston.PistonBaseBlock.EXTENDED))continue;
            if(state.hasProperty(DiodeBlock.POWERED)&&state.getValue(DiodeBlock.POWERED))continue;
            if(state.hasProperty(TripWireHookBlock.ATTACHED)&&state.getValue(TripWireHookBlock.ATTACHED))continue;
            var level=tripwireLevel(r);var direction=state.getOptionalValue(BlockStateProperties.FACING).orElse(state.getOptionalValue(BlockStateProperties.HORIZONTAL_FACING).orElse(Direction.EAST));
            level.raw(ORIGIN.below(),Blocks.STONE.defaultBlockState());level.raw(ORIGIN.relative(direction.getOpposite()),Blocks.STONE.defaultBlockState());
            if(block==Blocks.TRIPWIRE_HOOK){for(int i=1;i<4;i++)level.raw(ORIGIN.relative(direction,i),Blocks.TRIPWIRE.defaultBlockState());level.raw(ORIGIN.relative(direction,4),Blocks.TRIPWIRE_HOOK.defaultBlockState().setValue(TripWireHookBlock.FACING,direction.getOpposite()));level.raw(ORIGIN.relative(direction,5),Blocks.STONE.defaultBlockState());}
            if(layout>0)level.raw(layout==3?ORIGIN.above(2):layout==2?ORIGIN.relative(direction.getAxis()==Direction.Axis.Y?Direction.EAST:direction.getClockWise()):(block==Blocks.REPEATER||block==Blocks.COMPARATOR)?ORIGIN.relative(direction):ORIGIN.below(),Blocks.REDSTONE_BLOCK.defaultBlockState());
            level.raw(ORIGIN,state);level.begin();block.setPlacedBy(level,ORIGIN,state,null,net.minecraft.world.item.ItemStack.EMPTY);
            var positions=new ArrayList<BlockPos>();positions.add(ORIGIN);positions.add(ORIGIN.above());for(int i=1;i<=4;i++)positions.add(ORIGIN.relative(direction,i));
            var states=new ArrayList<String>();for(var pos:positions)states.add(pos.toShortString()+"="+Block.getId(level.getBlockState(pos)));
            System.out.println("placed-by:"+Block.getId(state)+":"+layout+"\t"+String.join(";",states)+"|"+String.join(";",level.changes)+"|"+level.blockTicks.hasScheduledTick(ORIGIN,block));
        }
        for(boolean attached:new boolean[]{false,true})for(boolean powered:new boolean[]{false,true})for(boolean disarmed:new boolean[]{false,true})for(int tool=0;tool<4;tool++){
            var level=new RedstoneVectors(r);var state=Blocks.TRIPWIRE.defaultBlockState().setValue(TripWireBlock.ATTACHED,attached).setValue(TripWireBlock.POWERED,powered).setValue(TripWireBlock.DISARMED,disarmed);level.raw(ORIGIN,state);
            var player=new RedstonePlayer(level);level.addFreshEntity(player);
            player.getInventory().setItem(0,tool==0?net.minecraft.world.item.ItemStack.EMPTY:new net.minecraft.world.item.ItemStack(tool==2?net.minecraft.world.item.Items.STICK:net.minecraft.world.item.Items.SHEARS,tool==3?0:1));level.gameplay.clear();
            var returned=Blocks.TRIPWIRE.playerWillDestroy(level,ORIGIN,state,player);
            System.out.println("tripwire-shears:"+Block.getId(state)+":"+tool+"\t"+Block.getId(returned)+"|"+Block.getId(level.getBlockState(ORIGIN))+"|"+String.join(";",level.gameplay));
        }
    }
    public static void main(String[] args) {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
        var registries=NeighborVectors.load();
        var builtin=RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        signals(registries);piston(registries);wires(registries);try{moving(registries);pushing(registries);comparators(registries);placements(registries);hoppers(registries);pistonTimelines(registries);dispensing(registries);dispenserTriggers(registries);rails(registries);notes(registries);tripwires(registries);placedBy(registries);interactions(registries);plates(registries);}catch(Exception error){throw new RuntimeException(error);}
    }
}

// Bootstrap before loading any Level subclass: Level's own static fields refer
// to particle/game-event registries, which must already be initialized.
final class RedstoneBootstrap {
    public static void main(String[] args) {
        var out=System.out;var err=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
        RedstoneVectors.main(args);
    }
}
