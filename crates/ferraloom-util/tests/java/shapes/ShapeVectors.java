import java.io.*;
import java.lang.reflect.Field;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.phys.*;
import net.minecraft.world.phys.shapes.*;
import com.mojang.math.OctahedralGroup;

/** Numeric test vectors obtained by executing the unmodified vanilla 26.3 jar. */
public final class ShapeVectors {
    static DataOutputStream out;
    static final List<VoxelShape> shapes = new ArrayList<>();
    static final Map<String, Integer> registryShapes = new HashMap<>();
    static final Field DISCRETE;
    static { try { DISCRETE = VoxelShape.class.getDeclaredField("shape"); DISCRETE.setAccessible(true); }
        catch (ReflectiveOperationException e) { throw new ExceptionInInitializerError(e); } }
    static final BooleanOp[] OPS = {BooleanOp.FALSE,BooleanOp.NOT_OR,BooleanOp.ONLY_SECOND,BooleanOp.NOT_FIRST,BooleanOp.ONLY_FIRST,BooleanOp.NOT_SECOND,BooleanOp.NOT_SAME,BooleanOp.NOT_AND,BooleanOp.AND,BooleanOp.SAME,BooleanOp.SECOND,BooleanOp.CAUSES,BooleanOp.FIRST,BooleanOp.CAUSED_BY,BooleanOp.OR,BooleanOp.TRUE};
    static void doubles(DataOutputStream stream, double... values) throws IOException {
        for (double value : values) stream.writeLong(Double.doubleToRawLongBits(value));
    }
    static int kind(VoxelShape s) { return s instanceof CubeVoxelShape ? 0 : s instanceof SliceShape ? 2 : 1; }
    static void lines(DataOutputStream stream, VoxelShape s, boolean edges) throws IOException {
        var lines = new ArrayList<double[]>();
        Shapes.DoubleLineConsumer consumer = (x,y,z,u,v,w) -> lines.add(new double[]{x,y,z,u,v,w});
        if (edges) s.forAllEdges(consumer); else s.forAllBoxes(consumer);
        stream.writeInt(lines.size());
        for (var line : lines) doubles(stream, line);
    }
    static void snapshot(DataOutputStream stream, VoxelShape s) throws Exception {
        stream.writeByte(kind(s));
        stream.writeBoolean(s == Shapes.block());
        stream.writeBoolean(s == Shapes.empty());
        var discrete = (DiscreteVoxelShape) DISCRETE.get(s);
        stream.writeBoolean(s.isEmpty());
        for (var axis : Direction.Axis.values()) {
            stream.writeInt(discrete.getSize(axis));
            stream.writeInt(discrete.firstFull(axis));
            stream.writeInt(discrete.lastFull(axis));
            var coords = s.getCoords(axis);
            stream.writeBoolean(coords.getClass().getSimpleName().equals("CubePointRange"));
            for (double value : coords) doubles(stream, value);
            doubles(stream, s.min(axis), s.max(axis));
        }
        for (int x=0;x<discrete.getXSize();x++) for (int y=0;y<discrete.getYSize();y++) for (int z=0;z<discrete.getZSize();z++) stream.writeBoolean(discrete.isFull(x,y,z));
        lines(stream,s,false);
        lines(stream,s,true);
    }
    static int result(VoxelShape s) throws Exception {
        int id = shapes.size(); shapes.add(s); snapshot(out,s); return id;
    }
    static int box(double... bounds) throws Exception {
        out.writeByte(1); doubles(out,bounds);
        return result(Shapes.box(bounds[0],bounds[1],bounds[2],bounds[3],bounds[4],bounds[5]));
    }
    static int join(int a,int b,int op,boolean optimize) throws Exception {
        out.writeByte(2); out.writeInt(a);out.writeInt(b);out.writeByte(op);out.writeBoolean(optimize);
        boolean valid = !OPS[op].apply(false,false);out.writeBoolean(valid);
        if (!valid) {
            boolean threwJoin=false,threwQuery=false;
            try { Shapes.joinUnoptimized(shapes.get(a),shapes.get(b),OPS[op]); } catch(IllegalArgumentException e) { threwJoin=true; }
            try { Shapes.joinIsNotEmpty(shapes.get(a),shapes.get(b),OPS[op]); } catch(IllegalArgumentException e) { threwQuery=true; }
            if (!threwJoin || !threwQuery) throw new AssertionError("invalid op accepted");
            return -1;
        }
        out.writeBoolean(Shapes.joinIsNotEmpty(shapes.get(a),shapes.get(b),OPS[op]));
        return result(optimize ? Shapes.join(shapes.get(a),shapes.get(b),OPS[op]) : Shapes.joinUnoptimized(shapes.get(a),shapes.get(b),OPS[op]));
    }
    static int optimize(int id) throws Exception {out.writeByte(3);out.writeInt(id);return result(shapes.get(id).optimize());}
    static int move(int id,double x,double y,double z) throws Exception {out.writeByte(4);out.writeInt(id);doubles(out,x,y,z);return result(shapes.get(id).move(x,y,z));}
    static int face(int id, Direction d) throws Exception {out.writeByte(5);out.writeInt(id);out.writeByte(d.ordinal());return result(shapes.get(id).getFaceShape(d));}
    static void faces(int a,int b) throws Exception {
        for(var d:Direction.values()) {
            face(a,d);
            out.writeByte(8);out.writeInt(a);out.writeInt(b);out.writeByte(d.ordinal());
            out.writeBoolean(Shapes.blockOccludes(shapes.get(a),shapes.get(b),d));
            out.writeBoolean(Shapes.mergedFaceOccludes(shapes.get(a),shapes.get(b),d));
            out.writeBoolean(Shapes.faceShapeOccludes(shapes.get(a),shapes.get(b)));
            out.writeBoolean(Block.isFaceFull(shapes.get(a),d));
        }
    }
    static void collide(int id,Direction.Axis axis,AABB moving,double distance) throws Exception {
        out.writeByte(6);out.writeInt(id);out.writeByte(axis.ordinal());
        doubles(out,moving.minX,moving.minY,moving.minZ,moving.maxX,moving.maxY,moving.maxZ,distance,shapes.get(id).collide(axis,moving,distance));
    }
    static void clip(int id,Vec3 from,Vec3 to,BlockPos pos) throws Exception {
        out.writeByte(7);out.writeInt(id);doubles(out,from.x,from.y,from.z,to.x,to.y,to.z);
        out.writeInt(pos.getX());out.writeInt(pos.getY());out.writeInt(pos.getZ());
        BlockHitResult hit=shapes.get(id).clip(from,to,pos);out.writeBoolean(hit!=null);
        if(hit!=null) {doubles(out,hit.getLocation().x,hit.getLocation().y,hit.getLocation().z);out.writeByte(hit.getDirection().ordinal());out.writeBoolean(hit.isInside());}
    }
    static void rotate(int id,OctahedralGroup rotation,Vec3 point) throws Exception {
        out.writeByte(9);out.writeInt(id);
        for(var a:Direction.Axis.values())out.writeByte(rotation.permutation().permuteAxis(a).ordinal());
        for(var a:Direction.Axis.values())out.writeBoolean(rotation.inverts(a));
        doubles(out,point.x,point.y,point.z);result(Shapes.rotate(shapes.get(id),rotation,point));
    }
    static int imported(VoxelShape shape) throws Exception {
        var bytes=new ByteArrayOutputStream();snapshot(new DataOutputStream(bytes),shape);
        var key=Base64.getEncoder().encodeToString(bytes.toByteArray());
        Integer old=registryShapes.get(key);if(old!=null)return old;
        out.writeByte(0);int id=result(shape);registryShapes.put(key,id);return id;
    }
    static void probes(int id,Random random) throws Exception {
        for(var axis:Direction.Axis.values()) {
            for(double distance:new double[]{-2,-1.0000001,-1e-7,Math.nextUp(-1e-7),-0.0,Math.nextDown(1e-7),1e-7,Math.nextUp(1e-7),1.0000001,2}) {
                double shift=random.nextInt(9)/8.0;
                collide(id,axis,new AABB(-0.75+shift,-0.25,-0.375,0.25+shift,0.75,0.625),distance);
            }
        }
        for(double edge:new double[]{-1e-7,Math.nextUp(-1e-7),0,Math.nextDown(1e-7),1e-7,0.5,Math.nextDown(1),1,1+1e-7}) {
            clip(id,new Vec3(-1,edge,edge),new Vec3(2,edge,edge),BlockPos.ZERO);
            clip(id,new Vec3(edge,-1,edge),new Vec3(edge,2,edge),BlockPos.ZERO);
            clip(id,new Vec3(edge,edge,-1),new Vec3(edge,edge,2),BlockPos.ZERO);
        }
        clip(id,new Vec3(0.5,0.5,0.5),new Vec3(0.50001,0.5,0.5),BlockPos.ZERO);
        clip(id,new Vec3(0.5,0.5,0.5),new Vec3(1,1,1),BlockPos.ZERO);
        clip(id,new Vec3(13,5,-7),new Vec3(16,7,-5),new BlockPos(14,5,-7));
        out.writeByte(11);out.writeInt(id);
        for(var axis:Direction.Axis.values())for(double b:new double[]{-1e-7,0,0.5,1,Double.NaN})for(double c:new double[]{-1e-7,0,0.5,1,Double.NaN}) {
            doubles(out,b,c,shapes.get(id).min(axis,b,c),shapes.get(id).max(axis,b,c));
            out.writeInt(findIndex(shapes.get(id),axis,b));
        }
    }
    static int findIndex(VoxelShape s,Direction.Axis a,double p)throws Exception {
        var method=(s instanceof CubeVoxelShape?CubeVoxelShape.class:VoxelShape.class).getDeclaredMethod("findIndex",Direction.Axis.class,double.class);
        method.setAccessible(true);return (Integer)method.invoke(s,a,p);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        try(var stream=new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(Path.of(args[0]))))) {
            out=stream;out.writeInt(0x53485031);
            int empty=imported(Shapes.empty()),block=imported(Shapes.block()),infinity=imported(Shapes.INFINITY);
            var random=new Random(0x26_3_777);
            var candidates=new ArrayList<Integer>(List.of(empty,block,infinity));
            for(double edge:new double[]{-1e-7,Math.nextUp(-1e-7),-0.0,0.0,Math.nextDown(1e-7),1e-7,Math.nextUp(1e-7),0.125,0.125+Math.nextDown(1e-7),0.125+1e-7,0.499999999999,0.5,0.9999999,1.0,1.0000001,Math.nextUp(1.0000001)}) {
                int id=box(edge,-0.0,0.125,edge+0.25,1,0.875);candidates.add(id);optimize(id);faces(id,block);probes(id,random);
            }
            for(int i=0;i<96;i++) {
                int a=empty,b=empty;
                for(int j=0;j<4;j++) {
                    double[] low=new double[3],high=new double[3];
                    for(int k=0;k<3;k++){low[k]=(random.nextInt(25)-8)/8.0;high[k]=low[k]+(random.nextInt(8)+1)/8.0;if(i%3==0){low[k]+=random.nextDouble()*1e-6;high[k]+=random.nextDouble()*1e-6;}}
                    int id=box(low[0],low[1],low[2],high[0],high[1],high[2]);
                    if(j%2==0)a=join(a,id,14,false);else b=join(b,id,14,false);
                }
                for(int op=0;op<16;op++) {int id=join(a,b,op,false);if(id>=0){optimize(id);join(a,b,op,true);}}
                faces(a,b);probes(a,random);
                int shifted=move(move(a,0.1,-0.1,1e-7),-0.1,0.1,-1e-7);optimize(shifted);
                if(i<12)for(var rotation:OctahedralGroup.values()) {rotate(a,rotation,new Vec3(0.5,0.5,0.5));rotate(a,rotation,new Vec3(0.1,-0.0,2.75));}
                candidates.add(a);
            }
            for(int a:candidates)for(int op=0;op<16;op++){join(a,a,op,false);join(a,empty,op,false);join(empty,a,op,false);}
            var unique=new HashSet<Integer>();int states=0;
            for(var blockType:BuiltInRegistries.BLOCK)for(var state:blockType.getStateDefinition().getPossibleStates()) {
                int collision=imported(state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO));
                int outline=imported(state.getShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO));
                int occlusion=imported(state.getOcclusionShape());
                unique.add(collision);unique.add(outline);unique.add(occlusion);
                out.writeByte(10);out.writeInt(Block.getId(state));out.writeInt(collision);out.writeInt(outline);out.writeInt(occlusion);states++;
            }
            for(int id:unique) {optimize(id);faces(id,block);probes(id,random);}
            // Touching faces on both sides of the tolerance and subshape slices.
            for(double gap:new double[]{Math.nextDown(1e-7),1e-7,Math.nextUp(1e-7),2e-7}) {
                int a=box(0,0,0,0.5,1,1), b=box(0.5+gap,0,0,1,1,1);
                for(int op=0;op<16;op++)join(a,b,op,false);
            }
            out.writeByte(255);
            System.out.printf("states=%d unique_registry_shapes=%d shape_results=%d%n",states,unique.size(),shapes.size());
        }
    }
}
