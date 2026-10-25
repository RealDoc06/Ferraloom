import java.util.*;
import net.minecraft.advancements.predicates.NbtPredicate;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.TagValueInput;

/** Actual live Entity.load on all non-player ported families. The projection
 * keeps only implemented fields; every input remains attached for exact replay. */
public final class LiveLoadVectors {
  static Entity create(String kind,Level level) {
    return switch(kind){case "cow"->new Cow(EntityTypes.COW,level);case "item"->new ItemEntity(EntityTypes.ITEM,level);case "tnt"->new PrimedTnt(EntityTypes.TNT,level);case "falling_block"->new FallingBlockEntity(EntityTypes.FALLING_BLOCK,level);default->new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level);};
  }
  static CompoundTag input(int i,int kind)throws Exception {
    String[] inputs={
      "{}",
      "{Pos:[-17.25d,70.0d,31.5d],Motion:[0.25d,-0.5d,10.00001d],Rotation:[721.25f,-450.125f],CustomName:'loaded',OnGround:1b,Air:127,Fire:4s,fall_distance:3.5d,Invulnerable:1b,NoGravity:1b,Silent:1b,Tags:['a','b'],data:{value:7}}",
      "{Age:-17,ForcedAge:23,AgeLocked:1b,InLove:19,Health:7.5f,HurtTime:6s,DeathTime:2s,AbsorptionAmount:1.5f,NoAI:1b,LeftHanded:1b,CanPickUpLoot:1b,PersistenceRequired:1b,drop_chances:{mainhand:0.25f,head:2.0f},leash:[I;1,65,2],home_radius:4,home_pos:[I;3,70,4],DeathLootTable:'test:loot',DeathLootTableSeed:91L,variant:'minecraft:warm',sound_variant:'minecraft:moody',Item:{id:'minecraft:stone',count:7},PickupDelay:17s,Value:11s,Count:3,fuse:5s,block_state:{Name:'minecraft:stone'},explosion_power:9.5f,BlockState:{Name:'minecraft:anvil'},Time:37,DropItem:0b,CancelDrop:1b,FallHurtAmount:2.5f,FallHurtMax:12}",
      "{Health:3f,Age:5,LeftHanded:0b,NoAI:0b,drop_chances:{feet:0.75f,head:-1f,body:0.5f,unknown:1f},leash:{UUID:[I;0,1,0,2]},home_radius:-2,variant:'test:missing',sound_variant:'test:missing',Item:{id:'minecraft:dirt',count:2},Count:0,explosion_power:-20f,HurtEntities:0b,BlockState:{Name:'minecraft:sand'}}",
      "{Motion:[NaNd,8.0d,9.0d],Rotation:[-0.0f,-0.0f],Age:32768,Health:-1,Count:-2,Item:{id:'minecraft:stone',count:0},PickupDelay:32768,drop_chances:{mainhand:-0.0f,offhand:Infinityf,head:NaNf},leash:{bad:1},explosion_power:NaNf}",
      "{Pos:[I;2,66,3],Motion:[I;1,2,3],Rotation:[I;37,-450],Item:{id:'minecraft:stone',count:4},Team:'loads',CustomNameVisible:1b,Glowing:1b,TicksFrozen:81,HasVisualFire:1b}",
    };
    var tag=TagParser.parseCompoundFully(inputs[i]);
    tag.putIntArray("UUID",new int[]{0,kind+10,0,100+i});
    tag.putIntArray("LoveCause",new int[]{0,3,0,4});
    tag.putIntArray("Owner",new int[]{0,5,0,6});
    tag.putIntArray("Thrower",new int[]{0,7,0,8});
    tag.putIntArray("owner",new int[]{0,9,0,10});
    return tag;
  }
  static String base(Entity e) {
    var box=e.getBoundingBox();String head=e instanceof LivingEntity l?SoundVectors.f(l.yHeadRot)+","+SoundVectors.f(l.yBodyRot):"-";
    return TrackingVectors.vec(e.position())+"/"+TrackingVectors.vec(e.getDeltaMovement())+"/"+SoundVectors.f(e.getYRot())+","+SoundVectors.f(e.getXRot())+","+SoundVectors.f(e.yRotO)+","+SoundVectors.f(e.xRotO)+"/"+e.getUUID()+"/"+e.getId()+"/"+e.tickCount+"/"+e.isRemoved()+"/"+e.needsSync+"/"+head+"/"+DropVectors.vector(new net.minecraft.world.phys.Vec3(box.minX,box.minY,box.minZ))+"/"+DropVectors.vector(new net.minecraft.world.phys.Vec3(box.maxX,box.maxY,box.maxZ));
  }
  public static void run(Level level)throws Exception {
    level.getScoreboard().addPlayerTeam("loads");
    String[] kinds={"cow","item","tnt","falling_block","experience_orb"};
    String[][] fields={{"Age","ForcedAge","AgeLocked","InLove","LoveCause","Health","HurtTime","DeathTime","AbsorptionAmount","CanPickUpLoot","PersistenceRequired","LeftHanded","NoAI","drop_chances","leash","home_radius","home_pos","DeathLootTable","DeathLootTableSeed","variant","sound_variant","equipment"},{"Health","Age","PickupDelay","Owner","Thrower","Item"},{"fuse","block_state","explosion_power","owner"},{"BlockState","Time","DropItem","CancelDrop","HurtEntities","FallHurtAmount","FallHurtMax"},{"Health","Age","Value","Count"}};
    for(int kind=0;kind<kinds.length;kind++)for(int i=0;i<6;i++) {
      var e=create(kinds[kind],level);e.setId(99);e.setUUID(new UUID(0,1));e.setPos(8.5,64,8.5);e.setDeltaMovement(1,2,3);e.setYRot(11.25f);e.setXRot(-12.5f);e.tickCount=37;
      if(e instanceof ItemEntity item)item.setItem(new ItemStack(Items.DIAMOND,3));
      if(e instanceof Cow cow){cow.setAge(-100);cow.setAggressive(true);}
      var random=net.minecraft.util.RandomSource.create(12345);ExperienceVectors.field(Entity.class,"random").set(e,random);
      var tag=input(i,kind);e.load(TagValueInput.create(ProblemReporter.DISCARDING,level.registryAccess(),tag));
      var saved=NbtPredicate.getEntityTagToCompare(e);var projection=new CompoundTag();for(String field:fields[kind])if(saved.contains(field))projection.put(field,saved.get(field));
      System.out.println("entity-live-load\t"+kinds[kind]+"\t"+i+"\t"+tag+"\t"+base(e)+"\t"+projection+"\t"+Long.toHexString(random.nextLong()));
    }
    var cow=new Cow(EntityTypes.COW,level);
    for(String field:List.of("DATA_VARIANT_ID","DATA_SOUND_VARIANT_ID")) {
      var accessor=(net.minecraft.network.syncher.EntityDataAccessor<?>)ExperienceVectors.field(Cow.class,field).get(null);
      System.out.println("entity-cow-accessor\t"+field+"\t"+accessor.id()+"\t"+net.minecraft.network.syncher.EntityDataSerializers.getSerializedId(accessor.serializer()));
    }
    var lock=(net.minecraft.network.syncher.EntityDataAccessor<?>)ExperienceVectors.field(AgeableMob.class,"AGE_LOCKED").get(null);
    System.out.println("entity-cow-accessor\tAGE_LOCKED\t"+lock.id()+"\t"+net.minecraft.network.syncher.EntityDataSerializers.getSerializedId(lock.serializer()));
  }
}
