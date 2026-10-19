import java.lang.reflect.*;
import java.util.*;
import net.minecraft.advancements.predicates.*;
import net.minecraft.advancements.predicates.entity.*;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.damagesource.DamageTypes;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.DamageResistant;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Canonical equipment, flag and supporting-position reads used by predicates. */
public final class EntityFactsVectors {
  static final EquipmentSlot[] slots={EquipmentSlot.HEAD,EquipmentSlot.CHEST,EquipmentSlot.LEGS,EquipmentSlot.FEET,EquipmentSlot.BODY,EquipmentSlot.MAINHAND,EquipmentSlot.OFFHAND};
  static final class GetterCow extends Cow {
    final StringJoiner reads=new StringJoiner(",");
    GetterCow(Level level){super(EntityTypes.COW,level);}
    @Override public ItemStack getItemBySlot(EquipmentSlot slot){reads.add(slot.getSerializedName());return super.getItemBySlot(slot);}
  }
  static final class World extends ServerLevel {
    Level blocks;
    World(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public boolean hasChunk(int x,int z){return true;}
    @Override public boolean isLoaded(BlockPos pos){return true;}
    @Override public net.minecraft.world.level.block.state.BlockState getBlockState(BlockPos pos){return blocks.getBlockState(pos);}
  }
  static EntityEquipmentPredicate equipment(ItemPredicate[] tests) {
    return new EntityEquipmentPredicate(Optional.ofNullable(tests[0]),Optional.ofNullable(tests[1]),Optional.ofNullable(tests[2]),Optional.ofNullable(tests[3]),Optional.ofNullable(tests[4]),Optional.ofNullable(tests[5]),Optional.ofNullable(tests[6]));
  }
  static String pos(BlockPos pos){return pos.getX()+","+pos.getY()+","+pos.getZ();}
  public static void run(Level level)throws Exception {
    var wanted=ItemPredicate.Builder.item().of(BuiltInRegistries.ITEM,Items.LEATHER_BOOTS).build();
    for(String kind:List.of("player","cow","tnt"))for(int slot=0;slot<slots.length;slot++)for(int count:new int[]{1,0,-1}) {
      Entity entity=switch(kind){case "player"->new LivingVectors(level);case "cow"->new GetterCow(level);default->new PrimedTnt(EntityTypes.TNT,level);};
      if(entity instanceof Player player)player.getInventory().setSelectedSlot(4);
      if(entity instanceof LivingEntity living){var stack=new ItemStack(Items.LEATHER_BOOTS);stack.setCount(count);living.setItemSlot(slots[slot],stack);}
      var tests=new ItemPredicate[7];tests[slot]=wanted;
      boolean matches=equipment(tests).matches(entity);
      var item=entity instanceof LivingEntity living?living.getItemBySlot(slots[slot]).getItem():Items.AIR;
      System.out.println("entity-equipment-fact\t"+kind+"\t"+slots[slot].getSerializedName()+"\t"+count+"\t"+BuiltInRegistries.ITEM.getKey(item)+"\t"+matches);
    }
    for(int failure=-2;failure<7;failure++) {
      var entity=new GetterCow(level);var tests=new ItemPredicate[7];
      if(failure!=-2)for(int i=0;i<7;i++){tests[i]=wanted;entity.setItemSlot(slots[i],new ItemStack(i==failure?Items.STONE:Items.LEATHER_BOOTS));}
      entity.reads.setEmptyValue("-");
      boolean matches=equipment(tests).matches(entity);
      System.out.println("entity-equipment-order\t"+failure+"\t"+matches+"\t"+entity.reads);
    }
    var allocatorField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");allocatorField.setAccessible(true);var allocator=(sun.misc.Unsafe)allocatorField.get(null);
    var world=(World)allocator.allocateInstance(World.class);
    world.blocks=level;
    try(var input=EntityFactsVectors.class.getResourceAsStream("/data/minecraft/advancement/adventure/walk_on_powder_snow_with_leather_boots.json")) {
      var json=com.google.gson.JsonParser.parseReader(new java.io.InputStreamReader(Objects.requireNonNull(input),java.nio.charset.StandardCharsets.UTF_8)).getAsJsonObject();
      var predicateJson=json.getAsJsonObject("criteria").getAsJsonObject("walk_on_powder_snow_with_leather_boots").getAsJsonObject("conditions").getAsJsonObject("player").get("predicate");
      var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
      var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),level.registryAccess().listRegistries()));
      var predicate=EntityPredicate.CODEC.parse(net.minecraft.resources.RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE,lookup),predicateJson).getOrThrow();
      var supporting=new BlockPos(7,63,9);
      for(int test=0;test<8;test++) {
        var player=new LivingVectors(level);var item=new ItemStack(test==1?Items.DIAMOND_BOOTS:Items.LEATHER_BOOTS);item.setCount(test==2?0:test==3?-1:1);
        player.setItemSlot(test==4?EquipmentSlot.OFFHAND:EquipmentSlot.FEET,item);
        player.setPos(8.5,64,8.5);player.setOnGround(test!=5);
        ClimbVectors.put(level,supporting,(test==6?Blocks.STONE:Blocks.POWDER_SNOW).defaultBlockState());
        ClimbVectors.put(level,new BlockPos(8,63,8),(test==7?Blocks.POWDER_SNOW:Blocks.STONE).defaultBlockState());
        player.mainSupportingBlockPos=test==7?Optional.empty():Optional.of(supporting);
        System.out.println("entity-leather-boots-location\t"+test+"\t"+pos(player.getOnPos())+"\t"+predicate.matches(world,null,player));
      }
    }
    var location=LocationPredicate.Builder.location().setX(MinMaxBounds.Doubles.exactly(7.5)).setY(MinMaxBounds.Doubles.exactly(63.5)).setZ(MinMaxBounds.Doubles.exactly(9.5)).build();
    var on=new SteppingOnPredicate(location);var affected=new MovementAffectedByPredicate(location);
    var offset=Entity.class.getDeclaredMethod("getOnPos",float.class);offset.setAccessible(true);
    Block[] supports={null,Blocks.STONE,Blocks.OAK_FENCE,Blocks.COBBLESTONE_WALL,Blocks.OAK_FENCE_GATE};
    for(String kind:List.of("player","tnt","orb"))for(int index=0;index<supports.length;index++)for(double y:new double[]{64.0,64.8}) {
      Entity entity=switch(kind){case "player"->new LivingVectors(level);case "orb"->new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level);default->new PrimedTnt(EntityTypes.TNT,level);};
      entity.setPos(8.5,y,8.5);entity.setOnGround(y==64.0);
      var support=new BlockPos(7,63,9);
      if(supports[index]!=null){ClimbVectors.put(level,support,supports[index].defaultBlockState());entity.mainSupportingBlockPos=Optional.of(support);}
      System.out.println("entity-position-fact\t"+kind+"\t"+(supports[index]==null?-1:Block.getId(supports[index].defaultBlockState()))+"\t"+Long.toHexString(Double.doubleToRawLongBits(y))+"\t"+entity.onGround()+"\t"+
        pos(entity.getOnPos())+"\t"+pos(entity.getOnPosLegacy())+"\t"+pos(entity.getBlockPosBelowThatAffectsMyMovement())+"\t"+pos((BlockPos)offset.invoke(entity,Float.NaN))+"\t"+on.matches(entity,world,null)+"\t"+affected.matches(entity,world,null));
    }
    var shared=Entity.class.getDeclaredMethod("setSharedFlag",int.class,boolean.class);shared.setAccessible(true);
    for(String scenario:List.of("base","player-base","player-flying","player-glide","player-spectator","cow-baby","cow-glide","tnt-glide","item-stone","item-netherite","item-zero","item-negative","item-direct","item-other")) {
      Entity entity;
      if(scenario.startsWith("item")) {
        var item=new ItemEntity(EntityTypes.ITEM,level);var stack=new ItemStack(scenario.equals("item-stone")||scenario.equals("item-direct")||scenario.equals("item-other")?Items.STONE:Items.NETHERITE_INGOT);
        if(scenario.equals("item-zero"))stack.setCount(0);if(scenario.equals("item-negative"))stack.setCount(-1);
        if(scenario.equals("item-direct")||scenario.equals("item-other"))stack.set(DataComponents.DAMAGE_RESISTANT,new DamageResistant(HolderSet.direct(level.registryAccess().lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(scenario.equals("item-direct")?DamageTypes.IN_FIRE:DamageTypes.LAVA))));
        item.setItem(stack);entity=item;
      } else if(scenario.startsWith("player")){
        var player=new Player(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"metadata")) {
          @Override public GameType gameMode(){return scenario.equals("player-spectator")?GameType.SPECTATOR:GameType.SURVIVAL;}
        };
        player.getAbilities().flying=scenario.equals("player-flying");entity=player;
      }
      else if(scenario.startsWith("cow")){var cow=new Cow(EntityTypes.COW,level);if(scenario.equals("cow-baby"))cow.setAge(-1);entity=cow;}
      else entity=new PrimedTnt(EntityTypes.TNT,level);
      entity.setOnGround(true);entity.setRemainingFireTicks(20);entity.setPose(Pose.CROUCHING);shared.invoke(entity,3,true);shared.invoke(entity,4,true);shared.invoke(entity,7,scenario.endsWith("glide"));
      ExperienceVectors.field(Entity.class,"wasTouchingWater").setBoolean(entity,true);
      boolean living=entity instanceof LivingEntity;boolean glide=living&&((LivingEntity)entity).isFallFlying();
      System.out.println("entity-flags-fact\t"+scenario+"\t"+entity.onGround()+","+entity.isOnFire()+","+entity.isCrouching()+","+entity.isSprinting()+","+entity.isSwimming()+","+
        (glide||entity instanceof Player player&&player.getAbilities().flying)+","+(living&&((LivingEntity)entity).isBaby())+","+living+","+entity.isInWater()+","+glide+"\t"+entity.fireImmune());
    }
  }
}
