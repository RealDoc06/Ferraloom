import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.Level;

/** Calls the jar's actual Entity.move on a controlled concrete Level. Unused
 * services fail immediately. Generated abstract-method stubs are test plumbing. */
public final class EntityVectors {
  public static void main(String[] args) throws Exception {
    if(args.length>0 && args[0].equals("generate")){generate(Path.of(args[1]));return;}
    var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
    var registries=NeighborVectors.load();
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    Class.forName("EntityFixture").getMethod("run",RegistryAccess.class).invoke(null,registries);
  }
  static void generate(Path path) throws Exception {
    var out=new StringBuilder("""
      import java.util.*;
      import net.minecraft.core.*;
      import net.minecraft.core.registries.*;
      import net.minecraft.world.level.*;
      import net.minecraft.world.level.block.*;
      import net.minecraft.world.level.block.state.*;
      import net.minecraft.world.level.block.state.properties.*;
      import net.minecraft.world.level.chunk.*;
      import net.minecraft.world.level.border.*;
      import net.minecraft.world.entity.*;
      import net.minecraft.world.entity.player.*;
      import net.minecraft.world.phys.*;
      class EntityFixture extends Level {
      EntityFixture(RegistryAccess r){super(null,Level.OVERWORLD,r,r.lookupOrThrow(net.minecraft.core.registries.Registries.DIMENSION_TYPE).getOrThrow(net.minecraft.world.level.dimension.BuiltinDimensionTypes.OVERWORLD),false,false,0,512);for(int x=-2;x<=2;x++)for(int z=-2;z<=2;z++)source.getChunk(x,z,net.minecraft.world.level.chunk.status.ChunkStatus.FULL,true);}
      """);
    var seen=new HashSet<String>();
    for(Class<?> cls=Level.class;cls!=null;cls=cls.getSuperclass())for(var m:cls.getDeclaredMethods())if(Modifier.isAbstract(m.getModifiers()))stub(out,m,seen);
    for(var m:Level.class.getMethods())if(Modifier.isAbstract(m.getModifiers()))stub(out,m,seen);
    out.append("""
      final net.minecraft.world.level.entity.EntitySectionStorage<Entity> contactSections=new net.minecraft.world.level.entity.EntitySectionStorage<>(Entity.class,key->net.minecraft.world.level.entity.Visibility.TICKING);
      final net.minecraft.world.level.entity.EntityLookup<Entity> contactLookup=new net.minecraft.world.level.entity.EntityLookup<>();
      final net.minecraft.world.level.entity.LevelEntityGetter<Entity> contactGetter=new net.minecraft.world.level.entity.LevelEntityGetterAdapter<>(contactLookup,contactSections);
      // Controlled Level service: observe the real Entity constructor's ID calls.
      boolean captureCreationIds;
      int creationCounter;
      @Override public int getNextEntityId(){return captureCreationIds?++creationCounter:super.getNextEntityId();}
      AABB lastContactBounds;
      void addContact(Entity entity){contactLookup.add(entity);contactSections.getOrCreateSection(SectionPos.asLong(entity.blockPosition())).add(entity);}
      void clearContacts(){for(Entity entity:List.copyOf(java.util.stream.StreamSupport.stream(contactLookup.getAllEntities().spliterator(),false).toList())){contactSections.getOrCreateSection(SectionPos.asLong(entity.blockPosition())).remove(entity);contactLookup.remove(entity);}lastContactBounds=null;}
      @Override protected net.minecraft.world.level.entity.LevelEntityGetter<Entity> getEntities(){return contactGetter;}
      @Override public Collection<net.minecraft.world.entity.boss.enderdragon.EnderDragonPart> dragonParts(){return List.of();}
      @Override public List<Entity> getEntities(Entity except,AABB bounds,java.util.function.Predicate<? super Entity> predicate){lastContactBounds=bounds;return super.getEntities(except,bounds,predicate);}
      boolean captureEntitySounds;
      final List<String> soundCalls=new ArrayList<>();
      final List<String> gameEvents=new ArrayList<>();
      final Map<ChunkPos,LevelChunk> chunks=new HashMap<>();
      final WorldBorder border=new WorldBorder();
      final Source source=new Source();
      final class Source extends ChunkSource {
        final net.minecraft.world.level.lighting.LevelLightEngine lights=new net.minecraft.world.level.lighting.LevelLightEngine(this,false,false);
        @Override public BlockGetter getLevel(){return EntityFixture.this;}
        @Override public ChunkAccess getChunk(int x,int z,net.minecraft.world.level.chunk.status.ChunkStatus status,boolean load){var p=new ChunkPos(x,z);return load?chunks.computeIfAbsent(p,k->new LevelChunk(EntityFixture.this,k)):chunks.get(p);}
        @Override public void tick(java.util.function.BooleanSupplier t,boolean tick){}
        @Override public String gatherStats(){return "entity-fixture";}
        @Override public int getLoadedChunksCount(){return chunks.size();}
        @Override public net.minecraft.world.level.lighting.LevelLightEngine getLightEngine(){return lights;}
      }
      @Override public ChunkSource getChunkSource(){return source;}
      @Override public net.minecraft.world.flag.FeatureFlagSet enabledFeatures(){return net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS;}
      @Override public WorldBorder getWorldBorder(){return border;}
      @Override public BlockGetter getChunkForCollisions(int x,int z){return this;}
      @Override public java.util.List<net.minecraft.world.phys.shapes.VoxelShape> getEntityCollisions(Entity e,AABB bb){return List.of();}
      @Override public net.minecraft.world.level.block.entity.BlockEntity getBlockEntity(BlockPos pos){return null;}
      @Override public java.util.List<? extends net.minecraft.world.entity.player.Player> players(){return List.of();}
      final net.minecraft.world.scores.Scoreboard scoreboard=new net.minecraft.world.scores.Scoreboard();
      @Override public net.minecraft.world.scores.Scoreboard getScoreboard(){return scoreboard;}
      @Override public long getGameTime(){return 0;}
      @Override public net.minecraft.world.attribute.EnvironmentAttributeSystem environmentAttributes(){return net.minecraft.world.attribute.EnvironmentAttributeSystem.builder().build();}
      @Override public void scheduleTick(BlockPos pos,Block block,int delay){}
      @Override public void scheduleTick(BlockPos pos,net.minecraft.world.level.material.Fluid fluid,int delay){}
      void put(BlockPos pos,BlockState state){getChunkAt(pos).getSection(getSectionIndex(pos.getY())).setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,state);}
      void floor(){for(int x=-4;x<24;x++)for(int z=-4;z<24;z++)put(new BlockPos(x,63,z),Blocks.STONE.defaultBlockState());}
      static String bits(double v){return Long.toUnsignedString(Double.doubleToRawLongBits(v),16);}
      static String vec(Vec3 v){return bits(v.x)+","+bits(v.y)+","+bits(v.z);}
      static String flags(Entity e){return (e.onGround()?"1":"0")+(e.horizontalCollision?"1":"0")+(e.verticalCollision?"1":"0")+(e.verticalCollisionBelow?"1":"0")+(e.minorHorizontalCollision?"1":"0");}
      static void scenario(RegistryAccess r,String name,BlockState obstacle,Vec3 from,Vec3 delta,boolean ground,boolean player,boolean sneak,double fall){
        var f=new EntityFixture(r);f.floor();
        if(obstacle!=null)f.put(new BlockPos(4,64,4),obstacle);
        Entity e=player?new TestPlayer(f):new TestEntity(f);
        e.setPos(from);e.setOnGround(ground);e.setShiftKeyDown(sneak);e.fallDistance=fall;e.setDeltaMovement(delta);
        e.move(MoverType.SELF,delta);
        System.out.println("move\\t"+name+"\\t"+(player?"player":"cow")+"\\t"+(obstacle==null?-1:Block.getId(obstacle))+"\\t"+vec(from)+"\\t"+vec(delta)+"\\t"+ground+"\\t"+sneak+"\\t"+bits(fall)+"\\t"+vec(e.position())+"\\t"+flags(e)+"\\t"+bits(e.fallDistance)+"\\t"+vec(e.getDeltaMovement()));
      }
      public static void run(RegistryAccess r)throws Exception {
        TrackingVectors.run(new EntityFixture(r));
        PersistenceVectors.run(new EntityFixture(r));
        SpawnVectors.run(new EntityFixture(r));
        BorderVectors.run(new EntityFixture(r));
        RidingVectors.run(new EntityFixture(r));
        SynchedDataVectors.run(new EntityFixture(r));
        AttributeVectors.run();
        LivingVectors.run(new EntityFixture(r));
        TravelVectors.run(new EntityFixture(r));
        AbilityVectors.run(new EntityFixture(r));
        AbilitiesVectors.run();
        AirTravelVectors.run(new EntityFixture(r));
        FluidTravelVectors.run(new EntityFixture(r));
        FallFlyingVectors.run(new EntityFixture(r));
        GlidingVectors.run(new EntityFixture(r));
        FoodVectors.run(new EntityFixture(r));
        ShoulderVectors.run(new EntityFixture(r));
        AiStepVectors.run(new EntityFixture(r));
        TravelDispatchVectors.run(new EntityFixture(r));
        JumpVectors.run(new EntityFixture(r));
        ServerJumpVectors.run(new EntityFixture(r));
        ItemTickVectors.run();
        EffectVectors.run();
        EffectReadVectors.run(new EntityFixture(r));
        DamageVectors.run(new EntityFixture(r));
        DifficultyVectors.run();
        ExperienceVectors.run(new EntityFixture(r));
        DropVectors.run(new EntityFixture(r));
        StuckVectors.run(new EntityFixture(r));
        EquipmentBreakVectors.run(new EntityFixture(r));
        EntityFactsVectors.run(new EntityFixture(r));
        NbtVectors.run(new EntityFixture(r));
        NameVectors.run(new EntityFixture(r));
        FreezingVectors.run(new EntityFixture(r));
        FreezePhaseVectors.run(new EntityFixture(r));
        ClimbVectors.run(new EntityFixture(r));
        PushingVectors.run(new EntityFixture(r));
        CrammingVectors.run(new EntityFixture(r));
        DimensionVectors.run(new EntityFixture(r));
        PlayerTrackingVectors.run(new EntityFixture(r));
        PlayerTickVectors.run(new EntityFixture(r));
        PlayerSyncVectors.run(new EntityFixture(r));
        WorldgenVectors.run(new EntityFixture(r));
        CollisionVectors.run();
        PlacementContextVectors.run(new EntityFixture(r));
        LandingVectors.run(new EntityFixture(r));
        SoundVectors.run(new EntityFixture(r));
        TeleportVectors.run(new EntityFixture(r));
        PushVectors.run(new EntityFixture(r));
        InteractionVectors.run(new EntityFixture(r));
        ToolVectors.run(new EntityFixture(r));
        UseItemVectors.run(new EntityFixture(r));
        ContactVectors.run(new EntityFixture(r));
        for(String kind:List.of("tnt","player","flying"))for(boolean above:List.of(false,true))for(boolean drag:List.of(false,true))for(double speed:new double[]{-1,-0.25,0.65,2}){
          var f=new EntityFixture(r);Entity e=kind.equals("tnt")?new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,f):new TestPlayer(f);
          if(kind.equals("flying"))((Player)e).getAbilities().flying=true;
          e.setDeltaMovement(0.25,speed,-0.125);e.fallDistance=8;
          if(above)e.onAboveBubbleColumn(drag,BlockPos.ZERO);else e.onInsideBubbleColumn(drag);
          System.out.println("bubble\\t"+kind+"\\t"+(above?"above":"inside")+"\\t"+drag+"\\t"+speed+"\\t"+vec(e.getDeltaMovement())+"\\t"+bits(e.fallDistance));
        }
        var orbLevel=new EntityFixture(r);orbLevel.floor();var orb=new net.minecraft.world.entity.ExperienceOrb(EntityTypes.EXPERIENCE_ORB,orbLevel);orb.setId(4);orb.setPos(8.5,64.7,8.5);orb.setDeltaMovement(0.1,0,0.01);
        for(int tick=1;tick<=12;tick++){orb.commonTick();orb.tick();System.out.println("orb-tick\\t"+tick+"\\t"+vec(orb.position())+"\\t"+vec(orb.getDeltaMovement())+"\\t"+flags(orb)+"\\t"+bits(orb.fallDistance));}
        var retained=new net.minecraft.world.entity.ExperienceOrb(EntityTypes.EXPERIENCE_ORB,orbLevel);retained.setPos(1.5,64,1.5);var disconnected=new TestPlayer(orbLevel);disconnected.setPos(4.5,64,1.5);var following=net.minecraft.world.entity.ExperienceOrb.class.getDeclaredField("followingPlayer");following.setAccessible(true);following.set(retained,disconnected);disconnected.discard();var follow=net.minecraft.world.entity.ExperienceOrb.class.getDeclaredMethod("followNearbyPlayer");follow.setAccessible(true);follow.invoke(retained);
        System.out.println("orb-removed-follow\\t"+vec(retained.getDeltaMovement())+"\\t"+(following.get(retained)==disconnected));
        var fallingLevel=new EntityFixture(r);fallingLevel.floor();var falling=new net.minecraft.world.entity.item.FallingBlockEntity(EntityTypes.FALLING_BLOCK,fallingLevel);falling.setId(3);falling.setPos(8.5,68,8.5);
        for(int tick=1;tick<=4;tick++){falling.commonTick();falling.tick();System.out.println("falling-tick\\t"+tick+"\\t"+vec(falling.position())+"\\t"+vec(falling.getDeltaMovement())+"\\t"+flags(falling)+"\\t"+falling.time+"\\t"+bits(falling.fallDistance));}
        for(float[] rotation:List.of(new float[]{0,0},new float[]{37.25f,-28.125f},new float[]{90,180})){
          var owner=new TestPlayer(new EntityFixture(r));owner.setPos(8.5,64,8.5);owner.setYRot(rotation[0]);owner.setXRot(rotation[1]);owner.getRandom().setSeed(123);
          var dropped=owner.createItemStackToDrop(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,3),false,true);
          System.out.println("drop\\t"+rotation[0]+"\\t"+rotation[1]+"\\t"+vec(dropped.position())+"\\t"+vec(dropped.getDeltaMovement()));
        }
        var itemLevel=new EntityFixture(r);itemLevel.floor();var groundItem=new net.minecraft.world.entity.item.ItemEntity(EntityTypes.ITEM,itemLevel);groundItem.setId(2);groundItem.setItem(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,64));groundItem.setPos(8.5,64.7,8.5);groundItem.setDeltaMovement(0.15,0,0.03);groundItem.setPickUpDelay(7);
        for(int tick=1;tick<=12;tick++){groundItem.commonTick();groundItem.tick();System.out.println("item-tick\\t"+tick+"\\t"+vec(groundItem.position())+"\\t"+vec(groundItem.getDeltaMovement())+"\\t"+flags(groundItem)+"\\t"+groundItem.getAge()+"\\t"+groundItem.hasPickUpDelay()+"\\t"+bits(groundItem.fallDistance));}
        System.out.println("pick\\titem\\t"+(groundItem.getPickResult()==null));
        var tntLevel=new EntityFixture(r);tntLevel.floor();var tnt=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,tntLevel);tnt.setId(1);tnt.setPos(8.5,66,8.5);tnt.setDeltaMovement(0.125,0.2,-0.03);tnt.setFuse(4);
        for(int tick=1;tick<=4;tick++){tnt.commonTick();tnt.tick();System.out.println("tnt\\t"+tick+"\\t"+vec(tnt.position())+"\\t"+vec(tnt.getDeltaMovement())+"\\t"+flags(tnt)+"\\t"+tnt.getFuse()+"\\t"+tnt.isRemoved()+"\\t"+tnt.getRequiresPrecisePosition());}
        for(int test=0;test<6;test++){
          var player=new TestPlayer(new EntityFixture(r));var inventory=player.getInventory();
          var stack=new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,test==0?70:8);
          if(test==1 || test==5){for(int slot=0;slot<36;slot++)inventory.setItem(slot,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,64));inventory.setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,test==1?63:64));stack.setCount(2);}
          if(test==2){inventory.setItem(0,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIRT,64));inventory.setItem(40,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,60));}
          if(test==3){inventory.setSelectedSlot(5);inventory.setItem(5,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,60));inventory.setItem(40,new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,60));}
          if(test==4){stack=new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIAMOND_PICKAXE);stack.setDamageValue(9);}
          if(test==5)player.getAbilities().instabuild=true;
          boolean added=inventory.add(stack);var slots=new ArrayList<String>();
          for(int slot=0;slot<43;slot++){var item=inventory.getItem(slot);if(!item.isEmpty())slots.add(slot+":"+BuiltInRegistries.ITEM.getKey(item.getItem())+":"+item.getCount()+":"+item.getDamageValue());}
          System.out.println("inventory\\t"+test+"\\t"+added+"\\t"+stack.getCount()+"\\t"+String.join(";",slots));
        }
        int traceIndex=0;
        for(var start:List.of(new Vec3(0.5,64,0.5),new Vec3(-0.3,64.00001,15.8)))for(var delta:List.of(Vec3.ZERO,new Vec3(0.000001,0,0),new Vec3(3.2,-0.7,1.1),new Vec3(-4.5,2.3,-2.3),new Vec3(1,1,1),new Vec3(0,-8,0),new Vec3(0,0,20))){
          var end=start.add(delta);var radius=(double)(EntityTypes.COW.getDimensions().width()/2f);var box=new AABB(end.x-radius,end.y,end.z-radius,end.x+radius,end.y+EntityTypes.COW.getDimensions().height(),end.z+radius).deflate(9.999999747378752E-6);
          var contacts=new ArrayList<String>();BlockGetter.forEachBlockIntersectedBetween(start,end,box,(pos,step)->{contacts.add(pos.getX()+","+pos.getY()+","+pos.getZ()+","+step);return true;});
          System.out.println("trace\\t"+(traceIndex++)+"\\t"+vec(start)+"\\t"+vec(end)+"\\t"+String.join(";",contacts));
        }
        for(var tag:List.of(net.minecraft.tags.BlockTags.FENCES,net.minecraft.tags.BlockTags.WALLS,net.minecraft.tags.BlockTags.CLIMBABLE,net.minecraft.tags.BlockTags.CAN_GLIDE_THROUGH,net.minecraft.tags.BlockTags.SUPPRESSES_BOUNCE,net.minecraft.tags.BlockTags.FALL_DAMAGE_RESETTING,net.minecraft.tags.BlockTags.BLOCKS_FLUID_FLOW,net.minecraft.tags.BlockTags.FIRE,net.minecraft.tags.BlockTags.ANVIL,net.minecraft.tags.BlockTags.INSIDE_STEP_SOUND_BLOCKS,net.minecraft.tags.BlockTags.COMBINATION_STEP_SOUND_BLOCKS,net.minecraft.tags.BlockTags.CRYSTAL_SOUND_BLOCKS)){
          var names=new ArrayList<String>();for(var block:BuiltInRegistries.BLOCK)if(block.defaultBlockState().is(tag))names.add(BuiltInRegistries.BLOCK.getKey(block).toString());
          System.out.println("block-tag\\t"+tag.location()+"\\t"+String.join(",",names));
        }
        for(var entityTag:List.of(net.minecraft.tags.EntityTypeTags.NON_CONTROLLING_RIDER,net.minecraft.tags.EntityTypeTags.FREEZE_IMMUNE_ENTITY_TYPES,net.minecraft.tags.EntityTypeTags.POWDER_SNOW_WALKABLE_MOBS,net.minecraft.tags.EntityTypeTags.CAN_FLOAT_WHILE_RIDDEN)){
          var entityNames=new ArrayList<String>();
          for(var type:BuiltInRegistries.ENTITY_TYPE)if(BuiltInRegistries.ENTITY_TYPE.wrapAsHolder(type).is(entityTag))entityNames.add(BuiltInRegistries.ENTITY_TYPE.getKey(type).toString());
          System.out.println("entity-tag\\t"+entityTag.location()+"\\t"+String.join(",",entityNames));
        }
        for(var tag:List.of(net.minecraft.tags.FluidTags.WATER,net.minecraft.tags.FluidTags.LAVA,net.minecraft.tags.FluidTags.ENTITY_FLOATABLE)){
          var names=new ArrayList<String>();for(var fluid:BuiltInRegistries.FLUID)if(fluid.is(tag))names.add(BuiltInRegistries.FLUID.getKey(fluid).toString());
          System.out.println("fluid-tag\\t"+tag.location()+"\\t"+String.join(",",names));
        }
        var obstacles=List.of(Blocks.STONE.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState().setValue(BlockStateProperties.SLAB_TYPE,SlabType.TOP),Blocks.OAK_STAIRS.defaultBlockState(),Blocks.OAK_FENCE.defaultBlockState(),Blocks.COBBLESTONE_WALL.defaultBlockState());
        int n=0;for(var state:obstacles)for(boolean grounded:List.of(false,true))for(var delta:List.of(new Vec3(1.5,0,0),new Vec3(1.5,-0.1,0),new Vec3(1.2,0,0.7),new Vec3(0.7,-0.2,1.2)))scenario(r,"obstacle-"+(n++),state,new Vec3(3.0,64,4.5),delta,grounded,false,false,0);
        scenario(r,"wall-slide",Blocks.STONE.defaultBlockState(),new Vec3(3.0,64,4.1),new Vec3(2,-0.2,1.5),true,false,false,0);
        scenario(r,"negative-motion",Blocks.STONE.defaultBlockState(),new Vec3(5.8,64,4.5),new Vec3(-2,-0.1,0),true,false,false,0);
        scenario(r,"fall-distance-cast",null,new Vec3(8.5,70.123456789,8.5),new Vec3(0,-0.123456789123,0),false,false,false,7.123456789);
        scenario(r,"fall-land",null,new Vec3(8.5,65.123456789,8.5),new Vec3(0,-2.23456789123,0),false,false,false,7.123456789);
        for(var state:List.of(Blocks.SOUL_SAND.defaultBlockState(),Blocks.HONEY_BLOCK.defaultBlockState(),Blocks.ICE.defaultBlockState(),Blocks.WATER.defaultBlockState()))scenario(r,"factor-"+Block.getId(state),state,new Vec3(4.5,66,4.5),new Vec3(0,-3,0),false,false,false,4.25);
        var edge=new EntityFixture(r);edge.put(new BlockPos(3,63,4),Blocks.STONE.defaultBlockState());
        var p=new TestPlayer(edge);var from=new Vec3(3.5,64,4.5);var delta=new Vec3(1.1,0,0.7);p.setPos(from);p.setOnGround(true);p.setShiftKeyDown(true);p.setDeltaMovement(delta);p.move(MoverType.SELF,delta);
        System.out.println("edge\\t"+vec(p.position())+"\\t"+flags(p)+"\\t"+vec(p.getDeltaMovement()));
        for(var kind:List.of(net.minecraft.world.level.material.Fluids.WATER,net.minecraft.world.level.material.Fluids.LAVA))for(int test=0;test<5;test++){
          var f=new EntityFixture(r);var pos=new BlockPos(8,64,8);f.put(pos,kind.getSource(false).createLegacyBlock());
          if(test==1)f.put(pos.above(),kind.getSource(false).createLegacyBlock());
          if(test==2)f.put(pos.east(),kind.getFlowing(4,false).createLegacyBlock());
          if(test==4)f.put(pos.above(),Blocks.STONE.defaultBlockState());
          var e=new TestEntity(f);var fluidFrom=new Vec3(8.5,test==3?64.9:test==4?63.7:64.0,8.5);e.setPos(fluidFrom);e.fallDistance=12.5;
          boolean any=e.refreshFluid();
          System.out.println("interaction\\t"+BuiltInRegistries.FLUID.getKey(kind)+"\\t"+test+"\\t"+vec(fluidFrom)+"\\t"+any+"\\t"+vec(e.getDeltaMovement())+"\\t"+bits(e.getFluidHeight(net.minecraft.tags.FluidTags.WATER))+"\\t"+bits(e.getFluidHeight(net.minecraft.tags.FluidTags.LAVA))+"\\t"+e.isEyeInFluid(net.minecraft.tags.FluidTags.WATER)+"\\t"+e.isEyeInFluid(net.minecraft.tags.FluidTags.LAVA)+"\\t"+bits(e.fallDistance));
        }
        for(var kind:List.of(net.minecraft.world.level.material.Fluids.WATER,net.minecraft.world.level.material.Fluids.LAVA))for(int test=0;test<5;test++){
          var f=new EntityFixture(r);var pos=new BlockPos(8,65,8);var fluid=kind.getSource(test==4);f.put(pos,fluid.createLegacyBlock());
          if(test==0)f.put(pos.east(),kind.getFlowing(4,false).createLegacyBlock());
          if(test==1)f.put(pos.west().below(),kind.getSource(false).createLegacyBlock());
          if(test==2)f.put(pos.south(),kind.getFlowing(2,false).createLegacyBlock());
          if(test==3)f.put(pos.north(),(kind==net.minecraft.world.level.material.Fluids.WATER?net.minecraft.world.level.material.Fluids.LAVA:net.minecraft.world.level.material.Fluids.WATER).getSource(false).createLegacyBlock());
          if(test==4)f.put(pos.east(),Blocks.STONE.defaultBlockState());
          System.out.println("flow\\t"+BuiltInRegistries.FLUID.getKey(kind)+"\\t"+test+"\\t"+vec(fluid.getFlow(f,pos)));
        }
      }
      static final class TestEntity extends Entity {
        TestEntity(Level level){super(EntityTypes.COW,level);}
        @Override protected void defineSynchedData(net.minecraft.network.syncher.SynchedEntityData.Builder b){}
        @Override protected void readAdditionalSaveData(net.minecraft.world.level.storage.ValueInput i){}
        @Override protected void addAdditionalSaveData(net.minecraft.world.level.storage.ValueOutput o){}
        @Override public boolean hurtServer(net.minecraft.server.level.ServerLevel l,net.minecraft.world.damagesource.DamageSource s,float f){return false;}
        @Override public float maxUpStep(){return 0.6F;}
        boolean refreshFluid(){return updateFluidInteraction();}
      }
      static final class TestPlayer extends Player {
        TestPlayer(Level level){super(level,new com.mojang.authlib.GameProfile(new java.util.UUID(1,2),"fixture"));}
        @Override public GameType gameMode(){return GameType.SURVIVAL;}
      }
      }
      """);
    Files.writeString(path,out.toString());
  }
  static void stub(StringBuilder out,Method m,Set<String> seen){
    var skip=Set.of("enabledFeatures","getChunkSource","getWorldBorder","getBlockEntity","players","getScoreboard","getGameTime","environmentAttributes","getEntities","dragonParts");
    var params=m.getParameterTypes();var key=m.getName()+Arrays.toString(params);if(!seen.add(key)||skip.contains(m.getName()))return;
    out.append("@Override public ").append(m.getReturnType().getCanonicalName()).append(' ').append(m.getName()).append('(');
    for(int i=0;i<params.length;i++){if(i>0)out.append(',');out.append(params[i].getCanonicalName()).append(" p").append(i);}
    if(m.getName().equals("playSeededSound")&&params.length==9)out.append("){if(captureEntitySounds)soundCalls.add(SoundVectors.capture(p0,p1,p2,p3,p4,p5,p6,p7,p8));}\n");
    else if(m.getName().equals("gameEvent")&&params.length==3)out.append("){if(captureEntitySounds)gameEvents.add(SoundVectors.capture_game_event(p0,p1,p2));}\n");
    else if(Set.of("playSeededSound","gameEvent").contains(m.getName()))out.append("){}\n");
    else out.append("){throw new UnsupportedOperationException(\"").append(m.getName()).append("\");}\n");
  }
}
