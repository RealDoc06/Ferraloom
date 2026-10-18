import net.minecraft.world.TickRateManager;
import java.util.*;

/** Real TickRateManager and JDK Formatter vectors; clocks supplied explicitly. */
public final class TickRateVectors {
    static void print(String name,TickRateManager manager){
        System.out.println(name+"\t"+manager.runsNormally()+":"+manager.isFrozen()+":"+manager.isSteppingForward()+":"+manager.frozenTicksToRun());
    }
    public static void main(String[] args){
        for(float rate:new float[]{0.5f,20f,30.5f,10000f,Float.NaN,Float.POSITIVE_INFINITY}){
            var manager=new TickRateManager();manager.setTickRate(rate);
            System.out.println("rate:"+Float.floatToRawIntBits(rate)+"\t"+Float.floatToRawIntBits(manager.tickrate())+":"+manager.nanosecondsPerTick()+":"+Float.floatToRawIntBits(manager.millisecondsPerTick()));
        }
        var manager=new TickRateManager();print("initial",manager);
        manager.setFrozen(true);print("freeze-before-tick",manager);manager.tick();print("frozen-tick",manager);
        manager.setFrozenTicksToRun(2);print("step-request",manager);manager.tick();print("step-one",manager);manager.tick();print("step-two",manager);manager.tick();print("step-done",manager);
        manager.setFrozenTicksToRun(3);manager.setFrozen(false);manager.tick();print("unfreeze-with-steps",manager);
        for(long completed:new long[]{0,1,7,30,1000})for(long nanos:new long[]{0,1,125000,1085000,5015000,999999999}){
            double milliseconds=Math.max(1.0,nanos)/1000000;
            int tps=(int)(1000*completed/milliseconds);
            double mspt=completed==0?50.0:milliseconds/completed;
            System.out.println("sprint:"+completed+":"+nanos+"\t"+tps+":"+String.format(Locale.ROOT,"%.2f",mspt));
        }
    }
}
