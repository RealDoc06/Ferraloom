import java.util.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.level.ServerEntity;
import net.minecraft.server.level.ServerPlayer;

/** Executes vanilla's movement encoders and server interpolation tracker. */
public final class TrackingVectors {
  static String bits(double n) { return Long.toHexString(Double.doubleToRawLongBits(n)); }
  static String vec(Vec3 p) { return bits(p.x)+","+bits(p.y)+","+bits(p.z); }
  static String delta(VecDelta d) {
    if(d==null) return "null";
    if(d instanceof VecDelta.Linear e) return e.xa()+","+e.ya()+","+e.za();
    return String.join(";",((VecDelta.Stepped)d).steps().stream().map(e->e.ticks()+":"+e.xa()+","+e.ya()+","+e.za()).toList());
  }
  static String path(PositionPath p) {
    if(p instanceof PositionPath.Linear e) return "L:"+vec(e.endPosition());
    return "S:"+String.join(";",((PositionPath.Stepped)p).steps().stream().map(e->e.tickOffset()+":"+vec(e.position())).toList());
  }
  static String packet(Packet<?> packet) {
    if(packet instanceof ClientboundMoveEntityPacket p) return "move:"+p.hasPosition()+":"+p.hasRotation()+":"+delta(p.getPositionDelta())+":"+net.minecraft.util.Mth.packDegrees(p.getYRot())+","+net.minecraft.util.Mth.packDegrees(p.getXRot())+":"+p.isOnGround();
    if(packet instanceof ClientboundEntityPositionSyncPacket p) return "sync:"+path(p.position())+":"+Integer.toHexString(Float.floatToRawIntBits(p.yRot()))+","+Integer.toHexString(Float.floatToRawIntBits(p.xRot()))+":"+p.onGround();
    if(packet instanceof ClientboundSetEntityMotionPacket p) return "motion:"+vec(p.movement());
    if(packet instanceof ClientboundSetEntityDataPacket p) return "data:"+String.join(",",p.packedItems().stream().map(v->v.id()+":"+(v.value() instanceof net.minecraft.world.item.ItemStack stack?"item="+stack.getCount():v.value().toString())).toList());
    throw new UnsupportedOperationException(packet.getClass().getName());
  }
  static ServerEntity.Synchronizer synchronizer(List<String> output) {
    return new ServerEntity.Synchronizer() {
      public void sendToTrackingPlayers(Packet<? super ClientGamePacketListener> p) { output.add("T:"+packet(p)); }
      public void sendToTrackingPlayersAndSelf(Packet<? super ClientGamePacketListener> p) { output.add("S:"+packet(p)); }
      public void sendToTrackingPlayersFiltered(Packet<? super ClientGamePacketListener> p, java.util.function.Predicate<ServerPlayer> f) { throw new UnsupportedOperationException(); }
    };
  }
  static void serverEntity(Level level) {
    var item=new net.minecraft.world.entity.item.ItemEntity(EntityTypes.ITEM,level);item.setId(2);item.setYRot(0);item.setXRot(0);item.setPos(8.5,64,8.5);item.setDeltaMovement(Vec3.ZERO);item.setItem(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,3));
    var output=new ArrayList<String>();var tracker=new ServerEntity(null,item,UpdateInterval.periodic(3),true,synchronizer(output));
    var mount=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,level);
    for(int tick=0;tick<=12;tick++) {
      switch(tick) {
        case 1 -> item.setDeltaMovement(0.125,0,0);
        case 2 -> {item.setPos(8.625,64,8.5);item.setYRot(13);}
        case 4 -> {item.verticalCollision=true;item.setPos(8.7501,64,8.5);item.needsSync=true;}
        case 5 -> item.syncVelocity=true;
        case 6 -> item.setOnGround(true);
        case 7 -> {item.startRiding(mount,true,false);item.setYRot(20);item.needsSync=true;}
        case 8 -> item.stopRiding();
        case 10 -> {item.setPos(28.7501,64,8.5);item.needsSync=true;}
        case 11 -> {item.setRequiresPrecisePosition(true);item.needsSync=true;}
        case 12 -> {item.setRequiresPrecisePosition(false);item.setItem(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE,5));}
      }
      output.clear();tracker.sendChanges();System.out.println("server-entity\t"+tick+"\t"+(output.isEmpty()?"-":String.join("|",output)));
    }
    var tnt=new net.minecraft.world.entity.item.PrimedTnt(EntityTypes.TNT,level);tnt.setId(3);tnt.setPos(0,64,0);
    var stationary=new ServerEntity(null,tnt,UpdateInterval.periodic(1),true,synchronizer(output));
    for(int tick=0;tick<=401;tick++) {
      output.clear();stationary.sendChanges();
      if(!output.isEmpty()) System.out.println("server-teleport\t"+tick+"\t"+String.join("|",output));
    }
  }
  public static void run(Level level) {
    serverEntity(level);
    for(String kind:List.of("item","orb")) {
      Entity entity=kind.equals("item")?new net.minecraft.world.entity.item.ItemEntity(EntityTypes.ITEM,level):new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level);
      entity.setId(7);
      if(entity instanceof net.minecraft.world.entity.item.ItemEntity item)item.setItem(new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.STONE));
      var output=new ArrayList<String>();var tracker=new ServerEntity(null,entity,UpdateInterval.NEVER,true,synchronizer(output));tracker.sendChanges();output.clear();
      entity.setDeltaMovement(0.125,0.25,0.375);
      try {var mark=Entity.class.getDeclaredMethod("markHurt");mark.setAccessible(true);mark.invoke(entity);}catch(ReflectiveOperationException error){throw new AssertionError(error);}
      String before=entity.needsSync+","+entity.syncVelocity;tracker.sendChanges();
      System.out.println("hurt-sync\t"+kind+"\t"+before+"\t"+String.join("|",output)+"\t"+entity.needsSync+","+entity.syncVelocity);
    }
    for(Vec3 base:List.of(Vec3.ZERO,new Vec3(1.0/8192,-1.0/8192,4.00001),new Vec3(-30000000.125,0,29999999.9))) {
      var codec=new VecDeltaCodec();codec.setBase(base);
      for(Vec3 offset:List.of(Vec3.ZERO,new Vec3(1.0/8192,-1.0/8192,0),new Vec3(7.999755859375,-8,0),new Vec3(8,-8.000244140625,0),new Vec3(0.00000001,1,-0.0001))) {
        Vec3 pos=base.add(offset);var encoded=codec.tryEncode(pos);
        Vec3 decoded=encoded==null?base:encoded.decode(codec).endPosition();
        System.out.println("delta-codec\t"+vec(base)+"\t"+vec(pos)+"\t"+delta(encoded)+"\t"+vec(decoded)+"\t"+vec(new Vec3(VecDeltaCodec.encodingPrecisionLoss(pos.x),VecDeltaCodec.encodingPrecisionLoss(pos.y),VecDeltaCodec.encodingPrecisionLoss(pos.z))));
      }
      var steps=List.of(new PositionStep(base.add(1,0,0),2),new PositionStep(base.add(2,0.125,-1),3));
      System.out.println("delta-steps\t"+vec(base)+"\t"+path(PositionPath.stepped(steps))+"\t"+delta(codec.tryEncode(PositionPath.stepped(steps)))+"\t"+vec(codec.getBase()));
    }
    var entity=new Cow(EntityTypes.COW,level);var tracker=new SteppedInterpolationTracker(entity);
    for(int tick=0;tick<10;tick++) {
      Vec3 pos=new Vec3(tick*0.125,64+tick*0.001,-tick*0.2);entity.setPos(pos);
      entity.syncPosition=tick==1||tick==3||tick==4||tick==7;
      if(tick==4) tracker.applyPredictedMovement(new Vec3(0.1,0.0,0.0));
      tracker.updateTracking(pos);
      String result="-";
      if(tick%3==0) { result=path(tracker.getPositionPath(pos));tracker.clear(); }
      System.out.println("tracked-steps\t"+tick+"\t"+vec(pos)+"\t"+entity.syncPosition+"\t"+result);
    }
  }
}
