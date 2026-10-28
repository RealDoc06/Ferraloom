import java.nio.file.*;
import java.util.*;
import java.util.stream.Collectors;
import net.minecraft.world.entity.ai.goal.*;

/** Executes vanilla's selectors with observable goals; no selector algorithm is reproduced. */
public final class GoalSelectorVectors {
    static final class Probe extends Goal {
        final String id; final Fixture fixture;
        boolean usable=true, continuing=true, interruptable=true, everyTick;
        Probe(String id, Fixture fixture) {this.id=id;this.fixture=fixture;}
        public boolean canUse(){fixture.calls.add(id+".use");return usable;}
        public boolean canContinueToUse(){fixture.calls.add(id+".continue");return continuing;}
        public boolean isInterruptable(){fixture.calls.add(id+".interrupt");return interruptable;}
        public boolean requiresUpdateEveryTick(){fixture.calls.add(id+".every");return everyTick;}
        public void start(){fixture.calls.add(id+".start");}
        public void stop(){fixture.calls.add(id+".stop");}
        public void tick(){fixture.calls.add(id+".tick");}
        int delay(int n){return adjustedTickDelay(n);}
    }
    static final class Fixture {
        final GoalSelector selector=new GoalSelector();
        final Map<String,Probe> goals=new LinkedHashMap<>();
        final ArrayList<String> calls=new ArrayList<>();
        Probe get(String id){return goals.computeIfAbsent(id,k->new Probe(k,this));}
        void apply(String input) {
            String[] a=input.split(",");
            switch(a[0]) {
                case "add" -> {var goal=get(a[1]);goal.setFlags(flags(a[3]));selector.addGoal(Integer.parseInt(a[2]),goal);}
                case "use" -> get(a[1]).usable=Boolean.parseBoolean(a[2]);
                case "continue" -> get(a[1]).continuing=Boolean.parseBoolean(a[2]);
                case "interrupt" -> get(a[1]).interruptable=Boolean.parseBoolean(a[2]);
                case "every" -> get(a[1]).everyTick=Boolean.parseBoolean(a[2]);
                case "flags" -> get(a[1]).setFlags(flags(a[2]));
                case "remove" -> selector.removeGoal(get(a[1]));
                case "removeAll" -> selector.removeAllGoals(goal->{calls.add(((Probe)goal).id+".predicate");return a[1].contains(((Probe)goal).id);});
                case "disable" -> selector.disableControlFlag(Goal.Flag.valueOf(a[1]));
                case "enable" -> selector.enableControlFlag(Goal.Flag.valueOf(a[1]));
                case "tick" -> selector.tick();
                case "fast" -> selector.tickRunningGoals(false);
                case "force" -> selector.tickRunningGoals(true);
                default -> throw new AssertionError(input);
            }
        }
        String snapshot() throws Exception {
            var field=GoalSelector.class.getDeclaredField("lockedFlags");field.setAccessible(true);
            @SuppressWarnings("unchecked") var locks=(Map<Goal.Flag,WrappedGoal>)field.get(selector);
            return selector.getAvailableGoals().stream().map(g->((Probe)g.getGoal()).id+":"+g.getPriority()+":"+g.isRunning()).collect(Collectors.joining(","))+"/"+
                locks.entrySet().stream().map(e->e.getKey()+":"+((Probe)e.getValue().getGoal()).id).collect(Collectors.joining(","));
        }
    }
    static EnumSet<Goal.Flag> flags(String value) {
        var result=EnumSet.noneOf(Goal.Flag.class);
        if(!value.equals("-"))for(var flag:value.split("\\+"))result.add(Goal.Flag.valueOf(flag));
        return result;
    }
    static void sequence(List<String> rows,String name,String... inputs)throws Exception {
        var f=new Fixture();
        for(String input:inputs){f.calls.clear();f.apply(input);rows.add("selector\t"+name+"\t"+input+"\t"+(f.calls.isEmpty()?"-":String.join("|",f.calls))+"\t"+f.snapshot());}
    }
    public static void main(String[] args)throws Exception {
        var rows=new ArrayList<String>();
        sequence(rows,"priority","add,a,5,MOVE+LOOK","add,b,5,MOVE","add,c,1,LOOK","add,d,6,MOVE","tick","tick","continue,c,false","tick","disable,LOOK","tick","enable,LOOK","tick");
        sequence(rows,"interrupt","add,a,5,MOVE","interrupt,a,false","tick","add,b,1,MOVE","tick","interrupt,a,true","tick","continue,b,false","use,b,false","tick");
        sequence(rows,"cadence","add,a,1,MOVE","add,b,1,LOOK","every,b,true","tick","disable,MOVE","continue,b,false","fast","force","tick","enable,MOVE","tick");
        sequence(rows,"identity","add,a,5,MOVE","add,a,1,MOVE","add,b,2,LOOK","tick","remove,a","force","tick","add,a,3,MOVE","tick","removeAll,ab","tick");
        sequence(rows,"flags","add,a,2,MOVE+JUMP","add,b,1,LOOK+TARGET","tick","flags,a,LOOK","tick","flags,b,-","disable,LOOK","tick","enable,LOOK","tick");
        sequence(rows,"no-goal","add,a,2147483647,MOVE","add,b,2147483647,-","tick","remove,b","add,a,-2147483648,MOVE","tick","remove,a","add,a,-2147483648,MOVE","tick");
        var f=new Fixture();var p=f.get("delay");
        for(boolean every:List.of(false,true))for(int n:new int[]{Integer.MIN_VALUE,-3,-2,-1,0,1,2,3,7,Integer.MAX_VALUE}){
            p.everyTick=every;rows.add("delay\t"+every+"\t"+n+"\t"+p.delay(n));
        }
        Files.write(Path.of(args[0]),rows);System.out.println(rows.size()+" native goal observations");
    }
}
