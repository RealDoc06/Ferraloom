import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Native dimensions, pose scaling and attachment transform arithmetic. */
public final class DimensionVectors {
  static final class ResizingEntity extends net.minecraft.world.entity.item.PrimedTnt {
    EntityDimensions current;
    ResizingEntity(Level level) {super(EntityTypes.TNT,level);}
    void leaveFirstTick() {this.firstTick=false;}
    @Override public EntityDimensions getDimensions(Pose pose) {return current==null?super.getDimensions(pose):current;}
  }
  static String vec(Vec3 v) {return Long.toHexString(Double.doubleToRawLongBits(v.x))+","+Long.toHexString(Double.doubleToRawLongBits(v.y))+","+Long.toHexString(Double.doubleToRawLongBits(v.z));}
  static void emit(String name,EntityDimensions dimensions) {
    var output=new StringBuilder("dimensions\t"+name+"\t"+Integer.toHexString(Float.floatToRawIntBits(dimensions.width()))+","+Integer.toHexString(Float.floatToRawIntBits(dimensions.height()))+","+Integer.toHexString(Float.floatToRawIntBits(dimensions.eyeHeight()))+","+dimensions.fixed());
    for(var attachment:EntityAttachment.values()) {
      output.append('\t');boolean first=true;
      for(int index=0;;index++) {var point=dimensions.attachments().getNullable(attachment,index,0);if(point==null)break;if(!first)output.append(';');output.append(vec(point));first=false;}
      output.append(':').append(vec(dimensions.attachments().getAverage(attachment)));
    }
    System.out.println(output);
  }
  public static void run(Level level) {
    emit("cow",EntityTypes.COW.getDimensions());
    emit("baby-cow",EntityTypes.COW.getDimensions().scale(0.5f).withEyeHeight(0.665f).scale(1.7f));
    emit("happy-ghast",EntityTypes.HAPPY_GHAST.getDimensions());
    var player=new LivingVectors(level);player.getAttribute(Attributes.SCALE).setBaseValue(1.7);
    for(var pose:Pose.values())emit("player-"+pose.ordinal(),player.getDimensions(pose));
    var points=EntityTypes.HAPPY_GHAST.getDimensions().attachments().scale(0.7f,1.3f,0.7f);
    for(float rotation:new float[]{-180,-90,0,37.3f,90,180})for(int index:new int[]{-1,0,3,5})
      System.out.println("attachment\t"+Integer.toHexString(Float.floatToRawIntBits(rotation))+"\t"+index+"\t"+vec(points.getClamped(EntityAttachment.PASSENGER,index,rotation)));
    for(int x=5;x<=12;x++)for(int z=5;z<=12;z++)put(level,new net.minecraft.core.BlockPos(x,63,z));
    for(int y=64;y<=66;y++)put(level,new net.minecraft.core.BlockPos(9,y,8));
    var entity=new ResizingEntity(level);entity.setPos(8.7,64,8.5);entity.current=EntityDimensions.scalable(0.5f,0.5f);entity.refreshDimensions();entity.leaveFirstTick();
    entity.current=EntityDimensions.scalable(1,2);entity.refreshDimensions();
    emitPosition("growth",entity);
    entity.current=EntityDimensions.scalable(0.5f,0.5f);entity.refreshDimensions();entity.setPos(8.7,64,8.5);
    for(int x=5;x<=12;x++)for(int z=5;z<=12;z++)put(level,new net.minecraft.core.BlockPos(x,65,z));
    entity.current=EntityDimensions.scalable(1,2);entity.refreshDimensions();
    emitPosition("growth-low-ceiling",entity);
    player.setPos(8.7,64,8.5);player.leaveFirstTick();player.setPose(Pose.CROUCHING);player.setPose(Pose.STANDING);
    emitPosition("player-growth",player);
  }
  static void put(Level level,net.minecraft.core.BlockPos pos) {level.getChunkAt(pos).getSection(level.getSectionIndex(pos.getY())).setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,net.minecraft.world.level.block.Blocks.STONE.defaultBlockState());}
  static void emitPosition(String label,Entity entity) {var box=entity.getBoundingBox();System.out.println("size-refresh\t"+label+"\t"+vec(entity.position())+"\t"+vec(new Vec3(box.minX,box.minY,box.minZ))+"\t"+vec(new Vec3(box.maxX,box.maxY,box.maxZ)));}
}
