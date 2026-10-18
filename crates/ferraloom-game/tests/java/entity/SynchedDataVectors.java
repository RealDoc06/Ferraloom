import net.minecraft.network.syncher.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;

/** Runs boxed float equality and ItemStack identity through real SynchedEntityData. */
public final class SynchedDataVectors extends net.minecraft.world.entity.item.PrimedTnt {
  static final EntityDataAccessor<Float> FLOAT=SynchedEntityData.defineId(SynchedDataVectors.class,EntityDataSerializers.FLOAT);
  static final EntityDataAccessor<ItemStack> STACK=SynchedEntityData.defineId(SynchedDataVectors.class,EntityDataSerializers.ITEM_STACK);
  static final EntityDataAccessor<ItemStack> PROTOTYPE=SynchedEntityData.defineId(SynchedDataVectors.class,EntityDataSerializers.ITEM_STACK);
  SynchedDataVectors(Level level) {super(EntityTypes.TNT,level);}
  @Override protected void defineSynchedData(SynchedEntityData.Builder builder) {super.defineSynchedData(builder);builder.define(FLOAT,0.0f);builder.define(STACK,ItemStack.EMPTY);builder.define(PROTOTYPE,new ItemStack(Items.STONE));}
  static boolean changed(SynchedEntityData data) {return data.packDirty()!=null;}
  public static void run(Level level) {
    var entity=new SynchedDataVectors(level);var data=entity.getEntityData();
    float[] values={0.0f,-0.0f,-0.0f,Float.NaN,Float.intBitsToFloat(0x7f800001),1,1};
    for(int index=0;index<values.length;index++) {
      data.set(FLOAT,values[index]);boolean dirty=changed(data);
      System.out.println("synched-float\t"+index+"\t"+dirty+"\t"+Integer.toHexString(Float.floatToRawIntBits(data.get(FLOAT)))+"\t"+(data.getNonDefaultValues()!=null));
    }
    data.set(FLOAT,0.0f);changed(data);
    var states=new java.util.ArrayList<Boolean>();
    data.set(STACK,new ItemStack(Items.STONE,3));states.add(changed(data));
    data.set(STACK,new ItemStack(Items.STONE,3));states.add(changed(data));
    var held=data.get(STACK);data.set(STACK,held);states.add(changed(data));
    held.setCount(4);states.add(changed(data));
    data.set(STACK,held,true);states.add(changed(data));
    data.set(STACK,ItemStack.EMPTY);states.add(changed(data));
    data.set(STACK,ItemStack.EMPTY);states.add(changed(data));
    System.out.println("synched-stack\t"+String.join(",",states.stream().map(Object::toString).toList())+"\t"+(data.getNonDefaultValues()!=null));
    data.get(PROTOTYPE).setCount(2);boolean mutated=data.getNonDefaultValues()!=null;
    data.set(PROTOTYPE,data.get(PROTOTYPE).copy());
    System.out.println("synched-prototype\t"+mutated+","+(data.getNonDefaultValues()!=null));
  }
}
