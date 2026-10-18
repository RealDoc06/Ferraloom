import java.lang.reflect.*;
import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.core.BlockPos;
import net.minecraft.network.syncher.*;

/** Real Player/Avatar/LivingEntity metadata and health setters. */
public final class LivingVectors extends Player {
  LivingVectors(Level level) { super(level,new GameProfile(new UUID(1,2),"metadata")); }
  @Override public GameType gameMode() { return GameType.SURVIVAL; }
  void leaveFirstTick() {this.firstTick=false;}
  void setFlyingFlag() {this.setSharedFlag(7,true);}
  static String value(Object value) {
    if(value instanceof Float number) return Integer.toHexString(Float.floatToRawIntBits(number));
    if(value instanceof Enum<?> entry) return Integer.toString(entry.ordinal());
    if(value instanceof Optional<?> optional) return optional.isEmpty()?"none":value.toString();
    if(value instanceof OptionalInt optional) return optional.isEmpty()?"none":Integer.toString(optional.getAsInt());
    return value.toString();
  }
  public static void run(Level level) throws Exception {
    var player=new LivingVectors(level);var data=player.getEntityData();
    Field field=SynchedEntityData.class.getDeclaredField("itemsById");field.setAccessible(true);
    var items=(SynchedEntityData.DataItem<?>[])field.get(data);
    for(var item:items) System.out.println("living-data\t"+item.getAccessor().id()+"\t"+EntityDataSerializers.getSerializedId(item.getAccessor().serializer())+"\t"+value(item.getValue())+"\t"+item.isDirty());
    data.packDirty();player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(30);
    float[] health={20,30,-0.0f,Float.NaN,Float.intBitsToFloat(0x7f800001),Float.POSITIVE_INFINITY,Float.NEGATIVE_INFINITY,7.5f};
    for(int i=0;i<health.length;i++) {player.setHealth(health[i]);System.out.println("living-health\t"+i+"\t"+Integer.toHexString(Float.floatToRawIntBits(player.getHealth()))+"\t"+(data.packDirty()!=null));}
    float[] rotations={721.25f,-721.25f,Float.NaN,37.25f};
    for(int i=0;i<rotations.length;i++) {
      var loaded=new LivingVectors(level);loaded.setYRot(11.25f);
      var tag=new net.minecraft.nbt.CompoundTag();tag.put("Rotation",PersistenceVectors.floats(rotations[i],0));
      loaded.load(net.minecraft.world.level.storage.TagValueInput.create(net.minecraft.util.ProblemReporter.DISCARDING,level.registryAccess(),tag));
      System.out.println("living-load-rotation\t"+i+"\t"+Integer.toHexString(Float.floatToRawIntBits(loaded.getYRot()))+","+Integer.toHexString(Float.floatToRawIntBits(loaded.yHeadRot))+","+Integer.toHexString(Float.floatToRawIntBits(loaded.yBodyRot))+","+Integer.toHexString(Float.floatToRawIntBits(loaded.yHeadRotO))+","+Integer.toHexString(Float.floatToRawIntBits(loaded.yBodyRotO)));
    }
    var lavaPos=new BlockPos(8,64,8);
    level.getChunkAt(lavaPos).getSection(level.getSectionIndex(64)).setBlockState(8,0,8,Blocks.LAVA.defaultBlockState());
    var lavaEntity=new net.minecraft.world.entity.item.PrimedTnt(net.minecraft.world.entity.EntityTypes.TNT,level);lavaEntity.setPos(8.5,64,8.5);lavaEntity.fallDistance=8;
    for(int i=0;i<3;i++) {
      boolean before=lavaEntity.isInLava();lavaEntity.baseTick();
      System.out.println("lava-base-tick\t"+i+"\t"+before+","+Long.toHexString(Double.doubleToRawLongBits(lavaEntity.fallDistance))+","+lavaEntity.isInLava());
    }
    level.getChunkAt(lavaPos).getSection(level.getSectionIndex(64)).setBlockState(8,0,8,Blocks.AIR.defaultBlockState());
    var pos=new BlockPos(8,63,8);
    level.getChunkAt(pos).getSection(level.getSectionIndex(pos.getY())).setBlockState(8,15,8,Blocks.SOUL_SAND.defaultBlockState());
    player.setPos(8.5,64,8.5);
    var attributes=List.of(Attributes.STEP_HEIGHT,Attributes.GRAVITY,Attributes.AIR_DRAG_MODIFIER,Attributes.BOUNCINESS,Attributes.MOVEMENT_EFFICIENCY,Attributes.BURNING_TIME);
    double[][] values={{0.6,0.08,1,0,0,1},{1.25,0.015,2,0.75,0.5,0.125},{0,-0.1,0,1.25,1,2},{0.6,0.08,100,0,0.25,1024}};
    for(int i=0;i<values.length;i++) {
      for(int j=0;j<attributes.size();j++) player.getAttribute(attributes.get(j)).setBaseValue(values[i][j]);
      player.setNoGravity(i==2);player.setRemainingFireTicks(-20);player.setTicksFrozen(100);
      player.igniteForTicks(i==3?Integer.MAX_VALUE:123);
      System.out.println("living-movement\t"+i+"\t"+Integer.toHexString(Float.floatToRawIntBits(player.maxUpStep()))+","+Long.toHexString(Double.doubleToRawLongBits(player.getEffectiveGravity()))+","+Integer.toHexString(Float.floatToRawIntBits(player.getAirDrag()))+","+Long.toHexString(Double.doubleToRawLongBits(player.getEntityBounciness()))+","+Integer.toHexString(Float.floatToRawIntBits(player.getBlockSpeedFactor()))+","+player.getRemainingFireTicks()+","+player.getTicksFrozen());
    }
  }
}
