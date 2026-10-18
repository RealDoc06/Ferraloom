import com.google.gson.*;
import io.netty.buffer.*;
import java.nio.file.Files;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.PathNavigationRegion;
import net.minecraft.world.level.pathfinder.*;

/** Calls the real 26.3 search/heap/path classes; only neighbor generation is synthetic. */
public class CoreVectors {
  static final Gson G = new GsonBuilder().setPrettyPrinting().create();
  static JsonArray rows = new JsonArray();
  static JsonArray arr(int... values) {JsonArray a=new JsonArray();for(int v:values)a.add(v);return a;}
  static JsonArray pos(Node n) {return arr(n.x,n.y,n.z);}
  static JsonObject node(Node n) {
    JsonObject o=new JsonObject();o.add("pos",pos(n));o.addProperty("hash",n.hashCode());o.addProperty("heap",n.heapIdx);
    o.addProperty("g",Float.floatToRawIntBits(n.g));o.addProperty("h",Float.floatToRawIntBits(n.h));o.addProperty("f",Float.floatToRawIntBits(n.f));o.addProperty("walked",Float.floatToRawIntBits(n.walkedDistance));o.addProperty("malus",Float.floatToRawIntBits(n.costMalus));o.addProperty("closed",n.closed);o.addProperty("type",n.type.ordinal());
    o.add("from",n.cameFrom==null?JsonNull.INSTANCE:pos(n.cameFrom));return o;
  }
  static String bytes(ByteBuf buffer) {byte[] b=new byte[buffer.readableBytes()];buffer.getBytes(buffer.readerIndex(),b);return HexFormat.of().formatHex(b);}
  static void primitives() {
    int[][] xyz={{0,0,0},{1,2,3},{-1,-1,-1},{32768,256,256},{0,0,256},{1,-64,2},{Integer.MIN_VALUE,0,Integer.MAX_VALUE},{Integer.MAX_VALUE,-2048,Integer.MIN_VALUE}};
    for(int[] p:xyz) {
      Node n=new Node(p[0],p[1],p[2]);Node to=new Node(5,7,-9);JsonObject o=new JsonObject();o.addProperty("kind","node");o.add("pos",pos(n));o.addProperty("hash",n.hashCode());
      o.addProperty("distance",Float.floatToRawIntBits(n.distanceTo(to)));o.addProperty("squared",Float.floatToRawIntBits(n.distanceToSqr(to)));o.addProperty("xz",Float.floatToRawIntBits(n.distanceToXZ(to)));o.addProperty("manhattan",Float.floatToRawIntBits(n.distanceManhattan(to)));
      n.heapIdx=3;n.g=1.25f;n.h=-0.0f;n.f=9.5f;n.walkedDistance=6.75f;n.costMalus=-2;n.type=PathType.WATER;n.closed=true;n.cameFrom=to;
      o.add("clone",node(n.cloneAndMove(-5,200,8)));ByteBuf b=Unpooled.buffer();Node.DEBUG_STREAM_CODEC.encode(b,n);o.addProperty("bytes",bytes(b));rows.add(o);
    }
    for(PathType t:PathType.values()) {JsonObject o=new JsonObject();o.addProperty("kind","type");o.addProperty("id",t.ordinal());o.addProperty("name",t.name());o.addProperty("malus",Float.floatToRawIntBits(t.getMalus()));rows.add(o);}
  }
  static JsonObject heapState(BinaryHeap heap,Node[] nodes) {JsonObject s=new JsonObject();JsonArray h=new JsonArray(),idx=new JsonArray(),cost=new JsonArray();for(Node n:heap.getHeap())h.add(n.x);for(Node n:nodes){idx.add(n.heapIdx);cost.add(Float.floatToRawIntBits(n.f));}s.add("heap",h);s.add("indices",idx);s.add("costs",cost);return s;}
  static void heap(String name,float[] costs,String... operations) {
    JsonObject row=new JsonObject();row.addProperty("kind","heap");row.addProperty("name",name);JsonArray input=new JsonArray();Node[] nodes=new Node[costs.length];for(int i=0;i<costs.length;i++){nodes[i]=new Node(i,0,0);nodes[i].f=costs[i];input.add(Float.floatToRawIntBits(costs[i]));}row.add("costs",input);
    BinaryHeap heap=new BinaryHeap();JsonArray steps=new JsonArray();for(String op:operations){String[] a=op.split(":");JsonObject step=new JsonObject();step.addProperty("op",op);try {switch(a[0]){case"insert"->heap.insert(nodes[Integer.parseInt(a[1])]);case"pop"->step.addProperty("popped",heap.pop().x);case"peek"->step.addProperty("peek",heap.peek().x);case"change"->heap.changeCost(nodes[Integer.parseInt(a[1])],Float.parseFloat(a[2]));case"remove"->heap.remove(nodes[Integer.parseInt(a[1])]);case"clear"->heap.clear();default->throw new AssertionError(op);}}catch(IllegalStateException e){step.addProperty("error",e.getMessage());}step.add("state",heapState(heap,nodes));steps.add(step);}row.add("steps",steps);rows.add(row);
  }
  static void heaps() {
    heap("equal",new float[]{1,1,1,1,1},"insert:0","insert:1","insert:2","insert:3","insert:4","peek","pop","pop","pop","pop","pop");
    heap("change-remove-clear",new float[]{4,2,3,1,6,5},"insert:0","insert:1","insert:2","insert:3","insert:4","insert:5","change:4:0.0","change:4:9.0","remove:2","remove:3","pop","clear","insert:0","insert:2","pop");
    heap("special-floats",new float[]{Float.NaN,-0.0f,0.0f,Float.POSITIVE_INFINITY,Float.NEGATIVE_INFINITY,3},"insert:0","insert:1","insert:2","insert:3","insert:4","insert:5","change:0:2.0","change:1:NaN","pop","pop","pop","pop","pop","pop");
    BinaryHeap grown=new BinaryHeap();for(int i=0;i<140;i++){Node n=new Node(i,0,0);n.f=(i*31)%17;grown.insert(n);}JsonObject growth=new JsonObject();growth.addProperty("kind","heap-growth");JsonArray popped=new JsonArray();while(!grown.isEmpty())popped.add(grown.pop().x);growth.add("popped",popped);rows.add(growth);
  }
  static JsonObject pathState(Path p) {JsonObject o=new JsonObject();JsonArray nodes=new JsonArray();for(int i=0;i<p.getNodeCount();i++)nodes.add(node(p.getNode(i)));o.add("nodes",nodes);o.addProperty("cursor",p.getNextNodeIndex());o.addProperty("not_started",p.notStarted());o.addProperty("done",p.isDone());o.addProperty("dist",Float.floatToRawIntBits(p.getDistToTarget()));o.addProperty("hash",p.hashCode());return o;}
  static void targets() {
    Node first=new Node(1,2,3),second=new Node(4,5,6);Target target=new Target(first);JsonObject row=new JsonObject();row.addProperty("kind","target");JsonArray states=new JsonArray();
    for(float h:new float[]{10,10,9,Float.NaN,8}){target.updateBest(h,h==10? (states.isEmpty()?first:second):(Float.isNaN(h)||h==8?first:second));JsonObject state=new JsonObject();state.addProperty("h",Float.floatToRawIntBits(h));state.add("best",pos(target.getBestNode()));state.addProperty("reached",target.isReached());states.add(state);}target.setReached();row.add("states",states);row.addProperty("reached",target.isReached());rows.add(row);
    JsonObject ids=new JsonObject();ids.addProperty("kind","type-fallback");JsonArray decoded=new JsonArray();for(int id:new int[]{-1,27,255,Integer.MAX_VALUE,Integer.MIN_VALUE}){ByteBuf b=Unpooled.buffer();new net.minecraft.network.FriendlyByteBuf(b).writeVarInt(id);JsonObject out=new JsonObject();out.addProperty("id",id);out.addProperty("result",PathType.STREAM_CODEC.decode(b).ordinal());decoded.add(out);}ids.add("values",decoded);rows.add(ids);
    JsonObject empty=new JsonObject();empty.addProperty("kind","path-empty");empty.add("state",pathState(new Path(new ArrayList<>(),BlockPos.ZERO,false)));rows.add(empty);
  }
  static void paths() {
    Node a=new Node(1,2,3),b=new Node(2,3,4),c=new Node(4,5,6);Path p=new Path(new ArrayList<>(List.of(a,b,c)),new BlockPos(8,9,10),true);Path copy=p.copy();JsonObject row=new JsonObject();row.addProperty("kind","path-alias");JsonArray states=new JsonArray();
    states.add(pathPair(p,copy));p.advance();states.add(pathPair(p,copy));b.f=9.5f;states.add(pathPair(p,copy));p.replaceNode(1,new Node(7,8,9));states.add(pathPair(p,copy));copy.truncateNodes(2);states.add(pathPair(p,copy));copy.setNextNodeIndex(-2);states.add(pathPair(p,copy));p.setNextNodeIndex(Integer.MAX_VALUE);p.advance();states.add(pathPair(p,copy));row.add("states",states);rows.add(row);
  }
  static JsonObject pathPair(Path a,Path b) {JsonObject o=new JsonObject();o.add("a",pathState(a));o.add("b",pathState(b));o.addProperty("same",a.sameAs(b));o.addProperty("equals",a.equals(b));return o;}
  static class Graph extends NodeEvaluator {
    final int[][] positions;final int[][] edges;final float[] costs;final boolean nullStart;final List<String> calls=new ArrayList<>();
    Graph(int[][] p,int[][]e,float[]c,boolean ns){positions=p;edges=e;costs=c;nullStart=ns;}
    public void prepare(PathNavigationRegion r,Mob m){nodes.clear();calls.add("prepare");}
    public void done(){calls.add("done");}
    Node at(int i){int[] p=positions[i];Node n=getNode(p[0],p[1],p[2]);n.costMalus=costs[i];return n;}
    public Node getStart(){calls.add("start");return nullStart?null:at(0);}
    public Target getTarget(double x,double y,double z){calls.add("target:"+(int)x+","+(int)y+","+(int)z);return getTargetNodeAt(x,y,z);}
    public int getNeighbors(Node[] out,Node n){calls.add("neighbors:"+n.x+","+n.y+","+n.z);for(int i=0;i<positions.length;i++){int[]p=positions[i];if(n.x==p[0]&&n.y==p[1]&&n.z==p[2]){int count=0;for(int j:edges[i]){Node next=at(j);if(!next.closed)out[count++]=next;}return count;}}return 0;}
    public PathType getPathTypeOfMob(PathfindingContext c,int x,int y,int z,Mob m){throw new AssertionError();}
    public PathType getPathType(PathfindingContext c,int x,int y,int z){throw new AssertionError();}
  }
  static void search(String name,int[][]p,int[][]edges,float[]cost,int[][]target,int budget,float length,int reach,float multiplier,boolean nullStart,boolean horizontal) {
    Graph graph=new Graph(p,edges,cost,nullStart);PathFinder finder=horizontal?new PathFinder(graph,budget){protected float distance(Node a,Node b){return a.distanceToXZ(b);}}:new PathFinder(graph,budget);finder.setCaptureDebug(()->true);Set<BlockPos> targets=new HashSet<>();for(int[]t:target)targets.add(new BlockPos(t[0],t[1],t[2]));
    JsonObject row=new JsonObject();row.addProperty("kind","search");row.addProperty("name",name);row.add("positions",G.toJsonTree(p));row.add("edges",G.toJsonTree(edges));JsonArray cs=new JsonArray();for(float c:cost)cs.add(Float.floatToRawIntBits(c));row.add("costs",cs);row.add("targets",G.toJsonTree(target));row.addProperty("budget",budget);row.addProperty("length",length);row.addProperty("reach",reach);row.addProperty("multiplier",multiplier);row.addProperty("null_start",nullStart);row.addProperty("horizontal",horizontal);
    try {Path path=finder.findPath(null,null,targets,length,reach,multiplier);JsonObject result=new JsonObject();result.add("calls",G.toJsonTree(graph.calls));if(path==null)result.add("path",JsonNull.INSTANCE);else{result.add("path",pathState(path));result.add("target",arr(path.getTarget().getX(),path.getTarget().getY(),path.getTarget().getZ()));result.addProperty("reached",path.canReach());ByteBuf buffer=Unpooled.buffer();try{Path.DEBUG_STREAM_CODEC.encode(buffer,path);result.addProperty("bytes",bytes(buffer));}catch(IllegalStateException e){result.addProperty("debug_error",e.getMessage());}}
      JsonArray ns=new JsonArray();for(int i=0;i<p.length;i++)ns.add(node(graph.at(i)));result.add("nodes",ns);row.add("result",result);
    } catch(IllegalStateException e){row.addProperty("error","duplicate-target");row.add("calls",G.toJsonTree(graph.calls));}
    rows.add(row);
  }
  static void searches() {
    int[][] line={{0,0,0},{1,0,0},{2,0,0},{3,0,0}},edges={{1},{0,2},{1,3},{2}},target={{3,0,0}};float[] costs={0,0,0,0};
    for(int budget:new int[]{0,1,2,3,4,5,30})search("budget"+budget,line,edges,costs,target,budget,10,0,1,false,false);
    for(float length:new float[]{0,1,2,3,3.001f})search("length"+length,line,edges,costs,target,30,length,0,1,false,false);
    for(int reach:new int[]{-1,1,2,3})search("reach"+reach,line,edges,costs,target,30,10,reach,1,false,false);
    search("multiplier",line,edges,costs,target,6,10,0,0.5f,false,false);
    search("null-start",line,edges,costs,target,30,10,0,1,true,false);
    search("empty-targets",line,edges,costs,new int[0][],30,10,0,1,false,false);
    search("start-target",line,edges,costs,new int[][]{{0,0,0}},30,10,0,1,false,false);
    search("unreachable",line,new int[][]{{1},{0},{3},{2}},costs,target,30,10,0,1,false,false);
    search("multi-target",line,edges,costs,new int[][]{{-3,0,0},{3,0,0},{0,0,4}},30,10,1,1,false,false);
    search("target-hash-alias",line,edges,costs,new int[][]{{0,0,0},{0,256,0}},30,10,0,1,false,false);
    int[][] diamond={{0,0,0},{1,0,0},{0,0,1},{2,0,0},{1,0,1},{2,0,1}},de={{1,2},{3,4},{4},{5},{5},{}};
    search("malus",diamond,de,new float[]{0,8,0,0,0,0},new int[][]{{2,0,1}},30,20,0,1,false,false);
    search("negative-malus",diamond,de,new float[]{0,2,0,0,-2,0},new int[][]{{2,0,1}},30,20,0,1,false,false);
    search("equal-alternatives",diamond,de,new float[6],new int[][]{{2,0,1}},30,20,0,1,false,false);
    int[][] steps={{0,0,0},{1,5,0},{0,0,2},{2,5,0}},se={{1,2},{3},{3},{}};
    search("euclidean",steps,se,new float[4],new int[][]{{2,5,0}},30,20,0,1,false,false);
    search("warden-horizontal",steps,se,new float[4],new int[][]{{2,5,0}},30,20,0,1,false,true);
  }
  public static void main(String[] args)throws Exception{primitives();heaps();targets();paths();searches();Files.writeString(java.nio.file.Path.of(args[0]),G.toJson(rows)+"\n");System.out.println(rows.size()+" native pathfinder cases");}
}
