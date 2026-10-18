import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.scores.*;

/** Real Entity pushes and the captured-rule lifetime of EntitySelector. */
public final class PushingVectors {
  static final class Receiver extends PrimedTnt {
    boolean pushable=true;PlayerTeam team;int teamReads;
    Receiver(Level level) {super(EntityTypes.TNT,level);}
    @Override public boolean isPushable() {return pushable;}
    @Override public PlayerTeam getTeam() {teamReads++;return team;}
  }
  public static void run(Level level) {
    double edge=(double)0.01f;
    double[][] deltas={{0,0},{0.001,0},{Math.nextDown(edge),0},{edge,0},{0.5,0.25},{2,-4},{-8,0.5}};
    for(int i=0;i<16;i++) {
      var a=new Receiver(level);var b=new Receiver(level);var mount=new PrimedTnt(EntityTypes.TNT,level);
      var delta=deltas[Math.min(i,deltas.length-1)];a.setPos(0,64,0);b.setPos(delta[0],64,delta[1]);
      a.noPhysics=i==7;b.noPhysics=i==8;a.pushable=i!=9&&i!=14;b.pushable=i!=10&&i!=14;
      int link=i>=11&&i<=13?i-10:0;
      if(link==1){a.startRiding(mount,true,false);b.startRiding(mount,true,false);}
      if(link==2)mount.startRiding(a,true,false);if(link==3)mount.startRiding(b,true,false);
      var va=new Vec3(0.125,-0.25,-0.0);var vb=new Vec3(-0.5,0.75,1);
      a.setDeltaMovement(va);b.setDeltaMovement(vb);a.needsSync=false;b.needsSync=false;
      a.push(i==15?a:b);
      System.out.println("entity-push\t"+i+"\t"+TrackingVectors.vec(new Vec3(delta[0],0,delta[1]))+"\t"+a.noPhysics+"\t"+b.noPhysics+"\t"+
        a.pushable+"\t"+b.pushable+"\t"+link+"\t"+(i==15)+"\t"+TrackingVectors.vec(va)+"\t"+TrackingVectors.vec(vb)+"\t"+
        TrackingVectors.vec(a.getDeltaMovement())+"\t"+TrackingVectors.vec(b.getDeltaMovement())+"\t"+a.needsSync+"\t"+b.needsSync);
    }
    Vec3[] impulses={Vec3.ZERO,new Vec3(0.25,-0.5,0.125),new Vec3(Double.NaN,0,0),new Vec3(0,Double.POSITIVE_INFINITY,0)};
    for(int i=0;i<impulses.length;i++) {
      var a=new Receiver(level);var before=new Vec3(-0.0,0.125,-0.5);a.setDeltaMovement(before);a.needsSync=false;a.push(impulses[i]);
      System.out.println("entity-impulse\t"+i+"\t"+TrackingVectors.vec(impulses[i])+"\t"+TrackingVectors.vec(before)+"\t"+TrackingVectors.vec(a.getDeltaMovement())+"\t"+a.needsSync);
    }
    for(boolean never:new boolean[]{false,true}) {
      var scoreboard=new Scoreboard();var own=scoreboard.addPlayerTeam("own");var other=scoreboard.addPlayerTeam("other");
      own.setCollisionRule(never?Team.CollisionRule.NEVER:Team.CollisionRule.PUSH_OTHER_TEAMS);
      var a=new Receiver(level);var b=new Receiver(level);a.team=own;b.team=other;
      var predicate=EntitySelector.pushableBy(a);own.setCollisionRule(Team.CollisionRule.ALWAYS);
      System.out.println("push-team-capture\t"+never+"\t"+predicate.test(b)+"\t"+a.teamReads+","+b.teamReads);
    }
  }
}
