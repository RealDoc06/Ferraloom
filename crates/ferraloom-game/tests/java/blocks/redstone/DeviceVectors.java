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
            calls.add("open:"+provider.getDisplayName().getString());
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
    static void runDevices()throws Exception{
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"redstone-devices")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var level=new DeviceVectors(server,storage);level.daylight();level.rods();level.notes();level.containers();level.destruction();level.ore();System.out.flush();System.exit(0);
    }
}
final class DeviceCapture{
    public static void main(String[] args)throws Exception{var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);DeviceVectors.runDevices();}
}
