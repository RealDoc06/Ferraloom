// Test code only. Run against vanilla's fastutil 8.5.18 and JDK 25.
// Produces operation-by-operation expected traversal, not a snapshot of Rust behavior.
import java.io.*;
import java.lang.reflect.*;
import java.util.*;
import it.unimi.dsi.fastutil.HashCommon;

public class CollectionsOracle {
    static long state;
    static int random(int bound) {
        state ^= state << 13; state ^= state >>> 7; state ^= state << 17;
        return (int) Long.remainderUnsigned(state, bound);
    }
    static final long[] LONGS = new long[160];
    static final int[] INTS = new int[160];
    static {
        LONGS[0]=0; LONGS[1]=-1; LONGS[2]=Long.MIN_VALUE; LONGS[3]=Long.MAX_VALUE;
        INTS[0]=0; INTS[1]=-1; INTS[2]=Integer.MIN_VALUE; INTS[3]=Integer.MAX_VALUE;
        int l=4,i=4;
        for (long x=-100000; l<LONGS.length || i<INTS.length; x++) {
            if (l<LONGS.length && (HashCommon.mix(x)&255)==255) LONGS[l++]=x;
            if (i<INTS.length && (HashCommon.mix((int)x)&255)==255) INTS[i++]=(int)x;
        }
    }
    // All 128 strings collide, but compare in Java's UTF-16 order. This forces
    // HashMap treeification, root changes, split bins and untreeification.
    static String string(int id) {
        if (id==0) return null;
        if (id==1) return "😀\uD800";
        StringBuilder s=new StringBuilder();
        for(int bit=0;bit<8;bit++) s.append((id & (1<<bit))==0 ? "Aa" : "BB");
        return s.toString();
    }
    static Object key(String kind,int id) {
        return switch(kind) {
            case "long" -> LONGS[id];
            case "int" -> INTS[id];
            case "short" -> (short)INTS[id];
            case "char" -> (char)INTS[id];
            case "string" -> string(id);
            default -> throw new AssertionError(kind);
        };
    }
    static int keyId(String kind,Object key) {
        for(int id=0;id<160;id++) if(Objects.equals(key(kind,id),key)) return id;
        throw new AssertionError(key);
    }
    static Object value(String kind,int id) {
        return switch(kind) {
            case "object" -> "v"+id;
            case "long" -> (long)id;
            case "int" -> id;
            case "byte" -> (byte)id;
            case "bool" -> (id&1)!=0;
            case "float" -> (float)id;
            case "double" -> (double)id;
            default -> throw new AssertionError(kind);
        };
    }
    static int valueId(Object value) {
        if(value instanceof String s) return Integer.parseInt(s.substring(1));
        if(value instanceof Boolean b) return b ? 1:0;
        return ((Number)value).intValue();
    }
    static int capacity(Object collection) throws Exception {
        Class<?> cls=collection.getClass();
        while(cls!=null) {
            try {
                Field field=cls.getDeclaredField("n"); field.setAccessible(true);return field.getInt(collection);
            } catch(NoSuchFieldException e) { cls=cls.getSuperclass(); }
        }
        Object map=collection;
        if(collection instanceof Set<?>) {
            Field f=HashSet.class.getDeclaredField("map");f.setAccessible(true);map=f.get(collection);
        }
        Field f=HashMap.class.getDeclaredField("table");f.setAccessible(true);Object[] table=(Object[])f.get(map);
        return table==null?0:table.length;
    }
    static void invoke(Object o,String name) throws Exception {
        for(Method m:o.getClass().getMethods()) {
            if(m.getName().startsWith(name) && m.getParameterCount()==0) {m.invoke(o);return;}
        }
        throw new AssertionError(name);
    }
    static void move(Object o,String name,Object key,Object value,boolean set) throws Exception {
        for(Method m:o.getClass().getMethods()) {
            if(!m.getName().equals(name) || m.isBridge() || m.getParameterCount()!=(set?1:2))continue;
            try { if(set)m.invoke(o,key);else m.invoke(o,key,value);return; }
            catch(IllegalArgumentException ignored) {}
        }
        throw new AssertionError(name);
    }
    @SuppressWarnings({"rawtypes","unchecked"})
    static void scenario(PrintWriter out,String type,String keys,String values,boolean set,boolean linked,float load) throws Exception {
        boolean jdk=type.startsWith("Java");
        Object collection;
        if(jdk) {
            collection=switch(type) {
                case "JavaHashMap" -> new HashMap<>(0,load);
                case "JavaHashSet" -> new HashSet<>(0,load);
                case "JavaLinkedHashMap" -> new LinkedHashMap<>(0,load);
                case "JavaAccessHashMap" -> new LinkedHashMap<>(0,load,true);
                case "JavaLinkedHashSet" -> new LinkedHashSet<>(0,load);
                default -> throw new AssertionError(type);
            };
        } else {
            String pkg=keys.equals("string")?"objects":keys+"s";
            collection=Class.forName("it.unimi.dsi.fastutil."+pkg+"."+type).getConstructor(int.class,float.class).newInstance(0,load);
        }
        Map map=set?null:(Map)collection;
        Set s=set?(Set)collection:null;
        state=0x12345678abcdefL;
        out.println("CASE "+type+" "+keys+" "+values+" "+load);
        for(int step=0;step<720;step++) {
            int op,id;
            if(step<160) {op=0;id=step;}
            else if(step<300) {op=1;id=step-160;}
            else if(step==300) {op=2;id=0;}
            else if(step==301) {op=3;id=0;}
            else {op=random(16);id=random(160);}
            // Keep clear infrequent while forcing a fresh growth cycle.
            if(op==3 && step!=301 && step!=600)op=0;
            if(op==2 && jdk)op=0;
            if(op>=4 && op<=9 && !linked)op=0;
            Object k=key(keys,id),v=set?null:value(values,step%101);
            String action;
            switch(op) {
                case 0,10,11,12,13 -> { if(set)s.add(k);else map.put(k,v); action="put"; }
                case 1 -> {if(set)s.remove(k);else map.remove(k);action="remove";}
                case 2 -> {invoke(collection,"trim");action="trim";}
                case 3 -> {if(set)s.clear();else map.clear();action="clear";}
                case 4,5 -> {
                    boolean first=op==4;
                    if(jdk) {
                        if(set) {if(first)((SequencedSet)s).addFirst(k);else ((SequencedSet)s).addLast(k);}
                        else {if(first)((SequencedMap)map).putFirst(k,v);else ((SequencedMap)map).putLast(k,v);}
                    } else move(collection,set?(first?"addAndMoveToFirst":"addAndMoveToLast"):(first?"putAndMoveToFirst":"putAndMoveToLast"),k,v,set);
                    action=first?"first":"last";
                }
                case 6,7 -> {
                    if(!set) {
                        if(jdk)map.get(k);
                        else move(collection,op==6?"getAndMoveToFirst":"getAndMoveToLast",k,null,true);
                    }
                    action=op==6?"getfirst":"getlast";
                }
                case 8,9 -> {
                    boolean first=op==8;
                    if(!(set?s.isEmpty():map.isEmpty())) {
                        if(jdk) {
                            if(set) {if(first)((SequencedSet)s).removeFirst();else ((SequencedSet)s).removeLast();}
                            else {if(first)((SequencedMap)map).pollFirstEntry();else ((SequencedMap)map).pollLastEntry();}
                        } else invoke(collection,set?(first?"removeFirst":"removeLast"):(first?"removeFirst":"removeLast"));
                    }
                    action=first?"popfirst":"poplast";
                }
                case 14 -> {
                    Iterator it=set?s.iterator():map.keySet().iterator();
                    int count=0;
                    while(it.hasNext()) {it.next();if((count++%3)==id%3)it.remove();}
                    action="iterremove";
                }
                case 15 -> {
                    if(!jdk) collection.getClass().getMethod("trim",int.class).invoke(collection,id);
                    else if(!set)map.get(k);
                    action=jdk?"get":"trimto";
                }
                default -> throw new AssertionError(op);
            }
            out.print(action+" "+id+" "+(step%101)+" "+capacity(collection)+" ");
            if(set) { for(Object item:s)out.print(keyId(keys,item)+","); }
            else { for(Object raw:map.entrySet()) { Map.Entry e=(Map.Entry)raw;out.print(keyId(keys,e.getKey())+":"+valueId(e.getValue())+","); } }
            out.println();
        }
    }
    public static void main(String[] args) throws Exception {
        try(PrintWriter out=new PrintWriter(args[0])) {
            for(float f:new float[]{0.75f,0.5f,0.9f}) {
                for(String spec:new String[]{
                    "LongOpenHashSet long set", "LongLinkedOpenHashSet long set",
                    "IntOpenHashSet int set", "ObjectOpenHashSet string set", "ObjectLinkedOpenHashSet string set",
                    "Long2ObjectOpenHashMap long object", "Long2ObjectLinkedOpenHashMap long object",
                    "Long2IntOpenHashMap long int", "Long2LongOpenHashMap long long", "Long2ByteOpenHashMap long byte",
                    "Int2ObjectOpenHashMap int object", "Object2ObjectOpenHashMap string object", "Object2IntOpenHashMap string int", "Object2ObjectLinkedOpenHashMap string object",
                    "Object2BooleanOpenHashMap string bool", "Object2ByteLinkedOpenHashMap string byte", "Int2ObjectLinkedOpenHashMap int object",
                    "Object2LongOpenHashMap string long", "Object2IntLinkedOpenHashMap string int", "Int2LongOpenHashMap int long",
                    "Short2ObjectOpenHashMap short object", "Int2IntOpenHashMap int int", "Long2FloatLinkedOpenHashMap long float",
                    "Short2BooleanOpenHashMap short bool", "Long2LongLinkedOpenHashMap long long", "Long2BooleanOpenHashMap long bool",
                    "CharOpenHashSet char set", "Int2IntLinkedOpenHashMap int int", "Int2DoubleOpenHashMap int double", "ShortOpenHashSet short set",
                    "JavaHashMap string object", "JavaHashSet string set", "JavaLinkedHashMap string object", "JavaLinkedHashSet string set", "JavaAccessHashMap string object"
                }) {
                    String[] p=spec.split(" "); scenario(out,p[0],p[1],p[2],p[2].equals("set"),p[0].contains("Linked")||p[0].contains("Access"),f);
                }
            }
        }
    }
}
