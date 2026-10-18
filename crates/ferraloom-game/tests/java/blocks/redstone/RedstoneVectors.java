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
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,double x,double y,double z,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){sounds.add(BuiltInRegistries.SOUND_EVENT.getId((net.minecraft.sounds.SoundEvent)sound.value())+":"+source.ordinal()+":"+Float.floatToRawIntBits(volume)+":"+Float.floatToRawIntBits(pitch)+":"+seed);}
    @Override public void playSeededSound(net.minecraft.world.entity.Entity except,net.minecraft.world.entity.Entity entity,Holder sound,net.minecraft.sounds.SoundSource source,float volume,float pitch,long seed){}
    @Override public void gameEvent(Holder event,net.minecraft.world.phys.Vec3 pos,net.minecraft.world.level.gameevent.GameEvent.Context context){}
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
        for (var block:List.of(Blocks.REDSTONE_BLOCK,Blocks.REDSTONE_TORCH,Blocks.REDSTONE_WALL_TORCH,Blocks.OBSERVER,Blocks.REPEATER,Blocks.LEVER,Blocks.STONE_BUTTON,Blocks.LIGHT_WEIGHTED_PRESSURE_PLATE)) {
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
        for(var block:List.<Block>of(Blocks.REDSTONE_WIRE,Blocks.REPEATER,Blocks.COMPARATOR,Blocks.OBSERVER,Blocks.REDSTONE_WALL_TORCH,Blocks.LEVER,Blocks.STONE_BUTTON,Blocks.OAK_BUTTON,Blocks.PISTON,Blocks.STICKY_PISTON,Blocks.DISPENSER,Blocks.DROPPER,Blocks.HOPPER,Blocks.OAK_DOOR,Blocks.IRON_DOOR,Blocks.COPPER_DOOR.weathering().unaffected(),Blocks.OAK_TRAPDOOR,Blocks.IRON_TRAPDOOR,Blocks.COPPER_TRAPDOOR.weathering().unaffected(),Blocks.OAK_FENCE_GATE))for(var direction:Direction.values())for(var face:Direction.values())for(int layout=0;layout<3;layout++) {
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
    public static void main(String[] args) {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
        var registries=NeighborVectors.load();
        var builtin=RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        signals(registries);piston(registries);wires(registries);try{moving(registries);pushing(registries);comparators(registries);placements(registries);hoppers(registries);pistonTimelines(registries);dispensing(registries);dispenserTriggers(registries);rails(registries);notes(registries);}catch(Exception error){throw new RuntimeException(error);}
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
