import java.io.*;
import java.lang.reflect.*;
import java.util.*;

public class PriorityQueueOracle {
    record Item(int id,int priority) implements Comparable<Item> {
        @Override public int compareTo(Item other) {return Integer.compare(priority,other.priority);}
    }
    static long state;
    static int random(int bound) {state^=state<<13;state^=state>>>7;state^=state<<17;return (int)Long.remainderUnsigned(state,bound);}
    static final Field ARRAY;
    static final Method REMOVE_AT;
    static {try {ARRAY=PriorityQueue.class.getDeclaredField("queue");ARRAY.setAccessible(true);REMOVE_AT=PriorityQueue.class.getDeclaredMethod("removeAt",int.class);REMOVE_AT.setAccessible(true);}catch(Exception e){throw new RuntimeException(e);}}
    static int capacity(PriorityQueue<Item> q)throws Exception {return ((Object[])ARRAY.get(q)).length;}
    static void printIds(PrintWriter out,Collection<Item> entries) {if(entries.isEmpty())out.print("-");else for(Item entry:entries)out.print(entry.id()+",");out.print(" ");}
    static void scenario(PrintWriter out,int initial,boolean descending,boolean fromCollection,boolean shared)throws Exception {
        Item[] pool=new Item[192];for(int i=0;i<pool.length;i++)pool[i]=new Item(i,(i*37)%7-3);
        Comparator<Item> compare=Comparator.comparingInt(Item::priority);if(descending)compare=compare.reversed();
        PriorityQueue<Item> q=new PriorityQueue<>(Math.max(1,initial),compare);
        state=0x12345678abcdefL;
        out.println("CASE "+initial+" "+descending+" "+fromCollection+" "+shared);
        ArrayList<Item> items=new ArrayList<>();
        if(fromCollection) {
            // The collection constructor uses natural ordering; also test heapify with empty input.
            for(int i=0;i<initial;i++) {Item item=pool[random(pool.length)];items.add(shared?item:new Item(item.id(),item.priority()));}
            q=new PriorityQueue<>(items);
        }
        out.print("initial 0 0 "+capacity(q)+" -1 ");printIds(out,q);if(items.isEmpty())out.print("-");else printIds(out,items);out.println();
        for(int step=0;step<1600;step++) {
            int op,id;
            if(step<192) {op=0;id=step;}else{op=random(16);id=random(pool.length);}
            if(op==3 && step%71!=0)op=0;
            Item item=shared?pool[id]:new Item(id,pool[id].priority());
            String action;Item result=null;ArrayList<Item> visited=new ArrayList<>();
            switch(op) {
                case 0,6,7,8,9,10,11,12 -> {q.offer(item);action="offer";}
                case 1,13 -> {result=q.poll();action="poll";}
                case 2 -> {boolean removed=q.remove(item);action="remove";if(removed)result=item;}
                case 3 -> {q.clear();action="clear";}
                case 4,14 -> {
                    Iterator<Item> it=q.iterator();int count=0;
                    while(it.hasNext()) {Item next=it.next();visited.add(next);if((count++%3)==id%3)it.remove();}
                    action="iterremove";
                }
                case 5 -> {
                    // Capture the removed value separately; removeAt returns a moved value.
                    if(!q.isEmpty()){int index=id%q.size();result=(Item)((Object[])ARRAY.get(q))[index];REMOVE_AT.invoke(q,index);}
                    action="removeat";
                }
                case 15 -> {result=q.peek();action="peek";}
                default -> throw new AssertionError(op);
            }
            out.print(action+" "+id+" "+step+" "+capacity(q)+" "+(result==null?-1:result.id())+" ");
            printIds(out,q);if(visited.isEmpty())out.print("-");else printIds(out,visited);out.println();
        }
    }
    public static void main(String[] args)throws Exception {
        try(PrintWriter out=new PrintWriter(args[0])) {
            for(int n:new int[]{1,11,63,64})for(boolean descending:new boolean[]{false,true})for(boolean shared:new boolean[]{false,true})scenario(out,n,descending,false,shared);
            for(int n:new int[]{0,1,63,160})for(boolean shared:new boolean[]{false,true})scenario(out,n,false,true,shared);
        }
    }
}
