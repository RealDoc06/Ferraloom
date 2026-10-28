import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.ai.control.*;
import net.minecraft.world.entity.ai.navigation.PathNavigation;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.pathfinder.*;
import static org.mockito.Mockito.*;

/** Actual vanilla control state machines against explicit observable Mob services. */
public final class ControlVectors {
    static String f(float n){return Integer.toHexString(Float.floatToRawIntBits(n));}
    static Field field(Class<?> owner,String name)throws Exception {var f=owner.getDeclaredField(name);f.setAccessible(true);return f;}
    static final class Move extends MoveControl<Mob> {
        Move(Mob mob){super(mob);}String operation(){return operation.name();}
        float turn(float a,float b,float max){return rotlerp(a,b,max);}
    }
    static final class Look extends LookControl {
        Look(Mob mob){super(mob);}int cooldown(){return lookAtCooldown;}
        String angles(){return getYRotD().map(ControlVectors::f).orElse("-")+","+getXRotD().map(ControlVectors::f).orElse("-");}
    }
    static final class Fixture {
        final Mob mob=mock(Mob.class);
        final Level level=mock(Level.class);
        final PathNavigation navigation=mock(PathNavigation.class);
        final NodeEvaluator evaluator=mock(NodeEvaluator.class);
        final Move move=new Move(mob);final Look look=new Look(mob);
        final JumpControl jump=new JumpControl(mob);final BodyRotationControl body=new BodyRotationControl(mob);
        boolean walkable=true,done=true,onGround,liquid,affected=true;
        double x=8.5,y=64,z=8.5;BlockState state=Blocks.AIR.defaultBlockState();
        Fixture() {
            doReturn(level).when(mob).level();doReturn(navigation).when(mob).getNavigation();doReturn(evaluator).when(navigation).getNodeEvaluator();
            doReturn(jump).when(mob).getJumpControl();
            doAnswer(c->x).when(mob).getX();doAnswer(c->y).when(mob).getY();doAnswer(c->z).when(mob).getZ();
            doAnswer(c->y+(double)1.3f).when(mob).getEyeY();doAnswer(c->BlockPos.containing(x,y,z)).when(mob).blockPosition();
            doAnswer(c->(int)Math.floor(y)).when(mob).getBlockY();
            doAnswer(c->state).when(level).getBlockState(any());
            doAnswer(c->walkable?PathType.WALKABLE:PathType.BLOCKED).when(evaluator).getPathType(any(Mob.class),any(BlockPos.class));
            doAnswer(c->done).when(navigation).isDone();doAnswer(c->onGround).when(mob).onGround();doAnswer(c->liquid).when(mob).isInLiquid();
            doAnswer(c->affected).when(mob).isAffectedByFluids();
            doReturn(0.2).when(mob).getAttributeValue(Attributes.MOVEMENT_SPEED);doReturn(0.6f).when(mob).maxUpStep();doReturn(0.9f).when(mob).getBbWidth();
            doReturn(10).when(mob).getHeadRotSpeed();doReturn(40).when(mob).getMaxHeadXRot();doReturn(75).when(mob).getMaxHeadYRot();
            doCallRealMethod().when(mob).setYRot(anyFloat());doCallRealMethod().when(mob).setXRot(anyFloat());
            doCallRealMethod().when(mob).getYRot();doCallRealMethod().when(mob).getXRot();
            doCallRealMethod().when(mob).setSpeed(anyFloat());doCallRealMethod().when(mob).getSpeed();
            doCallRealMethod().when(mob).setZza(anyFloat());doCallRealMethod().when(mob).setXxa(anyFloat());doCallRealMethod().when(mob).setJumping(anyBoolean());
            mob.xo=x;mob.zo=z;
        }
        void apply(String input) {
            var a=input.split(",");
            switch(a[0]) {
                case "strafe"->move.strafe(Float.parseFloat(a[1]),Float.parseFloat(a[2]));
                case "want"->move.setWantedPosition(x+Double.parseDouble(a[1]),y+Double.parseDouble(a[2]),z+Double.parseDouble(a[3]),Double.parseDouble(a[4]));
                case "move"->move.tick();case "wait"->move.setWait();case "jump"->jump.tick();
                case "ground"->onGround=Boolean.parseBoolean(a[1]);case "liquid"->liquid=Boolean.parseBoolean(a[1]);case "affected"->affected=Boolean.parseBoolean(a[1]);
                case "walkable"->walkable=Boolean.parseBoolean(a[1]);case "done"->done=Boolean.parseBoolean(a[1]);
                case "yaw"->mob.setYRot(Float.parseFloat(a[1]));case "pitch"->mob.setXRot(Float.parseFloat(a[1]));
                case "head"->mob.yHeadRot=Float.parseFloat(a[1]);case "body"->mob.yBodyRot=Float.parseFloat(a[1]);
                case "position"->{x=Double.parseDouble(a[1]);y=Double.parseDouble(a[2]);z=Double.parseDouble(a[3]);}
                case "old"->{mob.xo=Double.parseDouble(a[1]);mob.zo=Double.parseDouble(a[2]);}
                case "look"->look.setLookAt(x+Double.parseDouble(a[1]),y+(double)1.3f+Double.parseDouble(a[2]),z+Double.parseDouble(a[3]),Float.parseFloat(a[4]),Float.parseFloat(a[5]));
                case "lookTick"->look.tick();case "bodyTick"->body.clientTick();
                case "block"->state=switch(a[1]){case "air"->Blocks.AIR.defaultBlockState();case "slab"->Blocks.STONE_SLAB.defaultBlockState();case "door"->Blocks.OAK_DOOR.defaultBlockState();case "fence"->Blocks.OAK_FENCE.defaultBlockState();default->throw new AssertionError(input);};
                default->throw new AssertionError(input);
            }
        }
        String snapshot()throws Exception {
            return move.operation()+":"+f(mob.getSpeed())+","+f(field(LivingEntity.class,"zza").getFloat(mob))+","+f(field(LivingEntity.class,"xxa").getFloat(mob))+":"+
                f(mob.getYRot())+","+f(mob.getXRot())+","+f(mob.yHeadRot)+","+f(mob.yBodyRot)+":"+
                field(LivingEntity.class,"jumping").getBoolean(mob)+":"+look.cooldown()+":"+look.angles()+":"+
                field(BodyRotationControl.class,"headStableTime").getInt(body)+","+f(field(BodyRotationControl.class,"lastStableYHeadRot").getFloat(body));
        }
    }
    static void sequence(List<String> rows,String name,String... inputs)throws Exception {
        var f=new Fixture();for(var input:inputs){f.apply(input);rows.add("control\t"+name+"\t"+input+"\t"+f.snapshot());}
    }
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var rows=new ArrayList<String>();
        sequence(rows,"strafe","move","strafe,0.2,0.4","move","yaw,90","strafe,2,-2","move","walkable,false","strafe,-1,1","move","move");
        sequence(rows,"move","want,0.0005,0,0,1","move","want,1,0,1,1.25","move","want,0.5,1,0,2","move","jump","move","liquid,true","affected,false","move","affected,true","move","want,0,2,0,1","move","want,3,0,0,0.5","move","ground,true","move","jump","wait","move");
        sequence(rows,"collision","block,slab","want,2,0,0,1","move","jump","ground,true","move","block,door","want,2,0,0,1","move","block,fence","want,2,0,0,1","move");
        sequence(rows,"look","pitch,35","head,170","body,-170","look,0,0,0,10,40","lookTick","lookTick","lookTick","done,false","look,-1,1,1,30,15","lookTick","lookTick","lookTick","look,0.000009,0.000009,0,90,90","lookTick","look,0.000011,0.000011,0,90,90","lookTick");
        sequence(rows,"body","head,80","bodyTick","head,85","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","bodyTick","position,9,64,8.5","yaw,-170","bodyTick");
        var control=new Move(null);
        for(float a:new float[]{-721.25f,-180f,-0f,0f,180f,360f,721.25f})for(float b:new float[]{-180f,0f,180f})for(float limit:new float[]{0f,10f,90f}){
            rows.add("rotation\t"+f(a)+"\t"+f(b)+"\t"+f(limit)+"\t"+f(control.turn(a,b,limit))+"\t"+f(control.rotateTowards(a,b,limit)));
        }
        Files.write(java.nio.file.Path.of(args[0]),rows);System.out.println(rows.size()+" native control observations");
    }
}
