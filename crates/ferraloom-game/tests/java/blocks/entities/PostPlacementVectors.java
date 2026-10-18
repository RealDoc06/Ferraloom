import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.TypedEntityData;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.pattern.BlockPattern;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.ticks.*;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real placement callbacks, data codecs and pattern matcher on explicit world services. */
public final class PostPlacementVectors {
  static final BlockPos POS=new BlockPos(8,5,8);
  static HolderLookup.Provider registries;
  static final class World extends ServerLevel {
    Map<BlockPos,BlockState> cells; Map<BlockPos,BlockEntity> entities;
    List<String> writes; GameRules rules; boolean power;
    LevelTicks<Block> ticks; LevelChunkTicks<Block> chunkTicks; long ordinal; int entityId;
    World(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public int getNextEntityId(){return ++entityId;}
    @Override public BlockState getBlockState(BlockPos pos){return cells.getOrDefault(pos,Blocks.AIR.defaultBlockState());}
    @Override public net.minecraft.world.level.material.FluidState getFluidState(BlockPos pos){return getBlockState(pos).getFluidState();}
    @Override public BlockEntity getBlockEntity(BlockPos pos){return entities.get(pos);}
    @Override public boolean hasChunkAt(BlockPos pos){return true;}
    @Override public boolean hasNeighborSignal(BlockPos pos){return power;}
    @Override public GameRules getGameRules(){return rules;}
    @Override public long getGameTime(){return 0;}
    @Override public long nextSubTickCount(){return ordinal++;}
    @Override public LevelTicks<Block> getBlockTicks(){return ticks;}
    @Override public boolean setBlock(BlockPos pos,BlockState state,int flags,int limit){cells.put(pos.immutable(),state);writes.add(Block.getId(state)+":"+flags);return true;}
    @Override public void scheduleTick(BlockPos pos,Block block,int delay){ticks.schedule(createTick(pos,block,delay));}
    @Override public void sendBlockUpdated(BlockPos pos,BlockState oldState,BlockState newState,int flags){}
    @Override public void blockEntityChanged(BlockPos pos){}
  }
  static void copyFields(Object from,Object to,Class<?> first)throws Exception{
    for(Class<?> owner=first;owner!=Object.class;owner=owner.getSuperclass())for(Field field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){field.setAccessible(true);field.set(to,field.get(from));}
  }
  static sun.misc.Unsafe allocator()throws Exception{var f=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");f.setAccessible(true);return (sun.misc.Unsafe)f.get(null);}
  static World world(RegistryAccess access)throws Exception{
    var world=(World)allocator().allocateInstance(World.class);copyFields(new NeighborFixture(access,100),world,Level.class);
    world.cells=new HashMap<>();world.entities=new HashMap<>();world.writes=new ArrayList<>();world.rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);
    world.ticks=new LevelTicks<>(key->true);world.chunkTicks=new LevelChunkTicks<>();world.ticks.addContainer(new ChunkPos(0,0),world.chunkTicks);return world;
  }
  static final class ProfilePlayer extends Player {
    ProfilePlayer(Level level){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"placement"));}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
  }
  static final class Editor extends ServerPlayer {
    boolean filtered;
    Editor(){super(null,null,null,null);}
    @Override public boolean isTextFilteringEnabled(){return filtered;}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
  }
  static Editor editor(World world,List<String> packets)throws Exception{
    var player=(Editor)allocator().allocateInstance(Editor.class);copyFields(new ProfilePlayer(world),player,Player.class);
    player.connection=mock(ServerGamePacketListenerImpl.class);
    doAnswer(call->{var packet=call.getArgument(0);packets.add(packet instanceof ClientboundBlockUpdatePacket?"block":packet instanceof ClientboundOpenSignEditorPacket p?"editor:"+p.slot().name():packet.getClass().getSimpleName());return null;}).when(player.connection).send(any(Packet.class));
    return player;
  }
  static void install(World world,BlockState state,BlockEntity entity){world.cells.put(POS,state);world.entities.put(POS,entity);entity.setLevel(world);}
  static void load(BlockEntity entity,CompoundTag tag){entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,registries,tag));}
  static String ticks(World world){return String.join(",",world.chunkTicks.getAll().map(t->t.triggerTick()+":"+BuiltInRegistries.BLOCK.getId(t.type())).toList());}
  static void beds(World world)throws Exception{
    for(var block:BuiltInRegistries.BLOCK)if(block instanceof AbstractBedBlock)for(var facing:Direction.Plane.HORIZONTAL)for(boolean occupied:new boolean[]{false,true}){
      var state=block.defaultBlockState().setValue(AbstractBedBlock.FACING,facing).setValue(AbstractBedBlock.OCCUPIED,occupied);
      world.cells.clear();world.writes.clear();block.setPlacedBy(world,POS,state,null,null);
      System.out.println("bed-placed\t"+Block.getId(state)+"\t"+Block.getId(world.getBlockState(POS.relative(facing)))+"\t"+String.join(",",world.writes));
      for(var part:BedPart.values())for(var side:Direction.values())for(int neighbor=0;neighbor<7;neighbor++){
        var current=state.setValue(AbstractBedBlock.PART,part);
        var other=switch(neighbor){case 0->Blocks.AIR.defaultBlockState();case 1->Blocks.STONE.defaultBlockState();case 2->BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace("red_bed")).defaultBlockState();default->block.defaultBlockState().setValue(AbstractBedBlock.PART,(neighbor&1)==0?BedPart.FOOT:BedPart.HEAD).setValue(AbstractBedBlock.OCCUPIED,neighbor>=5);};
        var result=current.updateShape(world,world,POS,side,POS.relative(side),other,net.minecraft.util.RandomSource.create(263));
        System.out.println("bed-shape\t"+Block.getId(current)+"\t"+side.get3DDataValue()+"\t"+Block.getId(other)+"\t"+Block.getId(result));
      }
    }
  }
  static void signs(World world)throws Exception{
    var packets=new ArrayList<String>();var player=editor(world,packets);
    for(var block:BuiltInRegistries.BLOCK)if(block instanceof SignBlock)for(boolean waxed:new boolean[]{false,true})for(boolean filtered:new boolean[]{false,true})for(int message=0;message<4;message++){
      var state=block.defaultBlockState();var entity=(SignBlockEntity)((EntityBlock)block).newBlockEntity(POS,state);install(world,state,entity);player.filtered=filtered;
      var raw=message==1||message==3?Component.translatable("test.message"):Component.literal("raw");
      var cleaned=message>=2?Component.translatable("test.filtered"):Component.literal("filtered");
      entity.setText(new SignText(List.of(raw,Component.empty(),Component.empty(),Component.empty()),List.of(cleaned,Component.empty(),Component.empty(),Component.empty()),DyeColor.BLACK,false),SignTextSlot.FRONT);entity.setWaxed(waxed);entity.setAllowedPlayerEditor(new UUID(3,4));
      player.getAbilities().mayBuild=false;packets.clear();block.setPlacedBy(world,POS,state,player,null);
      System.out.println("sign-placed\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+waxed+"\t"+filtered+"\t"+message+"\t"+new UUID(1,2).equals(entity.getPlayerWhoMayEdit())+"\t"+String.join(",",packets));
    }
  }
  static void command(World world)throws Exception{
    for(var block:List.of(Blocks.COMMAND_BLOCK,Blocks.REPEATING_COMMAND_BLOCK,Blocks.CHAIN_COMMAND_BLOCK))for(int mask=0;mask<128;mask++)for(int behind=0;behind<3;behind++){
      world.cells.clear();world.entities.clear();world.ticks=new LevelTicks<>(key->true);world.chunkTicks=new LevelChunkTicks<>();world.ticks.addContainer(new ChunkPos(0,0),world.chunkTicks);world.ordinal=0;
      var state=block.defaultBlockState().setValue(CommandBlock.CONDITIONAL,(mask&1)!=0);var entity=(CommandBlockEntity)((EntityBlock)block).newBlockEntity(POS,state);
      var tag=new CompoundTag();tag.putBoolean("auto",(mask&16)!=0);tag.putBoolean("powered",(mask&32)!=0);tag.putBoolean("TrackOutput",(mask&64)!=0);load(entity,tag);install(world,state,entity);
      world.rules.set(GameRules.SEND_COMMAND_FEEDBACK,(mask&2)!=0,null);world.power=(mask&4)!=0;
      if(behind!=0){var pos=POS.relative(state.getValue(CommandBlock.FACING).getOpposite());var other=new CommandBlockEntity(pos,Blocks.COMMAND_BLOCK.defaultBlockState());other.getCommandBlock().setSuccessCount(behind-1);other.setLevel(world);world.cells.put(pos,other.getBlockState());world.entities.put(pos,other);}
      var stack=new ItemStack(Items.COMMAND_BLOCK);if((mask&8)!=0)stack.set(DataComponents.BLOCK_ENTITY_DATA,TypedEntityData.of(BlockEntityTypes.CHEST,new CompoundTag()));
      block.setPlacedBy(world,POS,state,null,stack);
      System.out.println("command-placed\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+mask+"\t"+behind+"\t"+entity.saveWithoutMetadata(registries)+"\t"+ticks(world));
    }
  }
  static void utilities(World world)throws Exception{
    for(boolean triggered:new boolean[]{false,true}){
      world.ticks=new LevelTicks<>(key->true);world.chunkTicks=new LevelChunkTicks<>();world.ticks.addContainer(new ChunkPos(0,0),world.chunkTicks);
      var state=Blocks.CRAFTER.defaultBlockState().setValue(CrafterBlock.TRIGGERED,triggered);Blocks.CRAFTER.setPlacedBy(world,POS,state,null,null);Blocks.CRAFTER.setPlacedBy(world,POS,state,null,null);
      System.out.println("crafter-placed\t"+triggered+"\t"+ticks(world));
    }
    for(int kind=0;kind<5;kind++){
      world.writes.clear();var state=Blocks.JUKEBOX.defaultBlockState();world.cells.put(POS,state);var stack=new ItemStack(Items.JUKEBOX);var tag=new CompoundTag();
      if(kind==1)tag.putInt("RecordItem",7);if(kind==2)tag.put("RecordItem",new CompoundTag());if(kind==3)tag.putString("recorditem","ignored");
      if(kind!=0)stack.set(DataComponents.BLOCK_ENTITY_DATA,TypedEntityData.of(BlockEntityTypes.CHEST,tag));
      Blocks.JUKEBOX.setPlacedBy(world,POS,state,null,stack);System.out.println("jukebox-placed\t"+kind+"\t"+Block.getId(world.getBlockState(POS))+"\t"+String.join(",",world.writes));
    }
    var player=new ProfilePlayer(world);player.setCustomName(Component.literal("different display"));
    var state=Blocks.STRUCTURE_BLOCK.defaultBlockState();var entity=new StructureBlockEntity(POS,state);install(world,state,entity);
    Blocks.STRUCTURE_BLOCK.setPlacedBy(world,POS,state,player,null);System.out.println("structure-author\t"+entity.saveWithoutMetadata(registries).getStringOr("author",""));
  }
  static void data(World world)throws Exception{
    for(var block:List.of(Blocks.COMMAND_BLOCK,Blocks.REPEATING_COMMAND_BLOCK,Blocks.CHAIN_COMMAND_BLOCK,Blocks.STRUCTURE_BLOCK,Blocks.PLAYER_HEAD,Blocks.WITHER_SKELETON_SKULL)){
      for(int variant=0;variant<3;variant++){
        var entity=((EntityBlock)block).newBlockEntity(POS,block.defaultBlockState());var tag=new CompoundTag();
        if(variant!=0)tag=TagParser.parseCompoundFully("{Command:'say hi',SuccessCount:4,CustomName:'native',TrackOutput:false,LastOutput:'ignored',UpdateLastExecution:false,LastExecution:42L,powered:true,conditionMet:true,auto:true,name:'test:house',author:'Doc',metadata:'marker',posX:90,posY:-90,posZ:2,sizeX:-2,sizeY:90,sizeZ:3,rotation:'CLOCKWISE_90',mirror:'FRONT_BACK',mode:'SAVE',ignoreEntities:false,strict:true,showair:true,showboundingbox:false,integrity:0.25f,seed:7654321L,note_block_sound:'minecraft:block.note_block.harp',custom_name:'skull'}");
        if(variant==2)tag=TagParser.parseCompoundFully("{TrackOutput:true,LastOutput:'output',LastExecution:99L,CustomName:'named',rotation:'bad',mirror:'bad',mode:'bad',name:'invalid name',profile:{name:'Doc'},custom_name:'skull'}");
        load(entity,tag);entity.setLevel(world);System.out.println("data\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+variant+"\t"+tag+"\t"+entity.saveWithoutMetadata(registries)+"\t"+entity.getUpdateTag(registries)+"\t"+(entity.getUpdatePacket()!=null));
      }
    }
  }
  static void components(World world)throws Exception{
    for(var block:List.of(Blocks.COMMAND_BLOCK,Blocks.STRUCTURE_BLOCK,Blocks.PLAYER_HEAD))for(int mask=0;mask<4;mask++){
      var entity=((EntityBlock)block).newBlockEntity(POS,block.defaultBlockState());var patch=DataComponentPatch.builder();
      if((mask&1)!=0)patch.set(DataComponents.CUSTOM_NAME,Component.literal("placed"));
      if((mask&2)!=0){patch.set(DataComponents.PROFILE,net.minecraft.world.item.component.ResolvableProfile.createUnresolved("Doc"));patch.set(DataComponents.NOTE_BLOCK_SOUND,net.minecraft.resources.Identifier.withDefaultNamespace("block.note_block.harp"));}
      var built=patch.build();entity.applyComponents(DataComponentMap.EMPTY,built);
      var input=DataComponentPatch.CODEC.encodeStart(net.minecraft.resources.RegistryOps.create(NbtOps.INSTANCE,registries),built).getOrThrow();
      var collected=DataComponentMap.CODEC.encodeStart(net.minecraft.resources.RegistryOps.create(NbtOps.INSTANCE,registries),entity.collectComponents()).getOrThrow();
      System.out.println("components\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+input+"\t"+entity.saveWithoutMetadata(registries)+"\t"+collected);
    }
  }
  static void liveLoads(World world)throws Exception{
    for(var block:List.of(Blocks.COMMAND_BLOCK,Blocks.REPEATING_COMMAND_BLOCK,Blocks.CHAIN_COMMAND_BLOCK,Blocks.STRUCTURE_BLOCK))for(int mask=0;mask<8;mask++){
      world.cells.clear();world.entities.clear();world.writes.clear();world.ticks=new LevelTicks<>(key->true);world.chunkTicks=new LevelChunkTicks<>();world.ticks.addContainer(new ChunkPos(0,0),world.chunkTicks);
      var state=block.defaultBlockState();var entity=((EntityBlock)block).newBlockEntity(POS,state);
      if(entity instanceof CommandBlockEntity command)command.setAutomatic((mask&1)!=0);
      install(world,state,entity);var tag=new CompoundTag();tag.putBoolean("auto",(mask&2)!=0);tag.putBoolean("powered",(mask&4)!=0);tag.putString("mode",(mask&2)!=0?"LOAD":"SAVE");
      load(entity,tag);
      System.out.println("live-load\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+mask+"\t"+tag+"\t"+entity.saveWithoutMetadata(registries)+"\t"+Block.getId(world.getBlockState(POS))+"\t"+ticks(world)+"\t"+String.join(",",world.writes));
    }
  }
  static void wither(World world)throws Exception{
    var method=WitherSkullBlock.class.getDeclaredMethod("getOrCreateWitherFull");method.setAccessible(true);var pattern=(BlockPattern)method.invoke(null);
    for(var forward:Direction.values())for(var up:Direction.values())if(up!=forward&&up!=forward.getOpposite())for(int changed=-1;changed<9;changed++){
      world.cells.clear();var origin=POS.offset(1,1,1);
      for(int x=0;x<3;x++)for(int y=0;y<3;y++){
        var right=forward.getUnitVec3i().cross(up.getUnitVec3i());var pos=origin.offset(right.getX()*x-up.getStepX()*y,right.getY()*x-up.getStepY()*y,right.getZ()*x-up.getStepZ()*y);
        var block=y==0?((x&1)==0?Blocks.WITHER_SKELETON_SKULL:Blocks.WITHER_SKELETON_WALL_SKULL):y==1||x==1?Blocks.SOUL_SAND:Blocks.AIR;
        if(x*3+y==changed)block=Blocks.STONE;world.cells.put(pos,block.defaultBlockState());
      }
      var match=pattern.find(world,POS);String result=match==null?"none":match.getFrontTopLeft().toShortString()+":"+match.getForwards().get3DDataValue()+":"+match.getUp().get3DDataValue();
      System.out.println("wither-pattern\t"+forward.get3DDataValue()+"\t"+up.get3DDataValue()+"\t"+changed+"\t"+result);
    }
  }
  public static void main(String[] args)throws Exception{
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);var access=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    registries=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),access.listRegistries()));BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(DataComponentInitializers.PendingComponents::apply);
    var world=world(access);beds(world);signs(world);command(world);utilities(world);data(world);components(world);liveLoads(world);wither(world);
  }
}
