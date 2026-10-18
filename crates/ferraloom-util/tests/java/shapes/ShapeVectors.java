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
    static final Map<String, Integer> snapshots = new HashMap<>();
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
        int id = shapes.size(); shapes.add(s);
        var bytes = new ByteArrayOutputStream(); snapshot(new DataOutputStream(bytes),s);
        byte[] data=bytes.toByteArray();String key=Base64.getEncoder().encodeToString(data);
        Integer index=snapshots.get(key);
        if(index==null){index=snapshots.size();snapshots.put(key,index);out.writeInt(index);out.writeInt(data.length);out.write(data);}
        else out.writeInt(index);
        return id;
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
    static void merger(int a,int b,Direction.Axis axis,int cost,boolean firstOnly,boolean secondOnly) throws Exception {
        var method=Shapes.class.getDeclaredMethod("createIndexMerger",int.class,it.unimi.dsi.fastutil.doubles.DoubleList.class,it.unimi.dsi.fastutil.doubles.DoubleList.class,boolean.class,boolean.class);
        method.setAccessible(true);
        var merger=(IndexMerger)method.invoke(null,cost,shapes.get(a).getCoords(axis),shapes.get(b).getCoords(axis),firstOnly,secondOnly);
        out.writeByte(12);out.writeInt(a);out.writeInt(b);out.writeByte(axis.ordinal());out.writeInt(cost);out.writeBoolean(firstOnly);out.writeBoolean(secondOnly);
        writeMerger(merger);
    }
    static void writeMerger(IndexMerger merger)throws Exception {
        out.writeByte(switch(merger.getClass().getSimpleName()){case "DiscreteCubeMerger"->0;case "IdenticalMerger"->1;case "NonOverlappingMerger"->2;case "IndirectMerger"->3;default->throw new AssertionError();});
        out.writeInt(merger.size());out.writeBoolean(merger.getList().getClass().getSimpleName().equals("CubePointRange"));
        for(double v:merger.getList())doubles(out,v);
        var indexes=new ArrayList<int[]>();merger.forMergedIndexes((x,y,z)->{indexes.add(new int[]{x,y,z});return true;});
        out.writeInt(indexes.size());for(var index:indexes)for(int i:index)out.writeInt(i);
    }
    static void literalMerger(double[] a,double[] b,boolean identity,boolean firstOnly,boolean secondOnly)throws Exception {
        var first=it.unimi.dsi.fastutil.doubles.DoubleArrayList.wrap(a);
        var second=identity?first:it.unimi.dsi.fastutil.doubles.DoubleArrayList.wrap(b);
        var method=Shapes.class.getDeclaredMethod("createIndexMerger",int.class,it.unimi.dsi.fastutil.doubles.DoubleList.class,it.unimi.dsi.fastutil.doubles.DoubleList.class,boolean.class,boolean.class);
        method.setAccessible(true);var merged=(IndexMerger)method.invoke(null,1,first,second,firstOnly,secondOnly);
        out.writeByte(16);out.writeInt(a.length);doubles(out,a);out.writeInt(b.length);doubles(out,b);out.writeBoolean(identity);out.writeBoolean(firstOnly);out.writeBoolean(secondOnly);writeMerger(merged);
    }
    static void iterableCollision(List<Integer> ids,Direction.Axis axis,AABB moving,double distance)throws Exception {
        out.writeByte(13);out.writeInt(ids.size());for(int id:ids)out.writeInt(id);out.writeByte(axis.ordinal());
        doubles(out,moving.minX,moving.minY,moving.minZ,moving.maxX,moving.maxY,moving.maxZ,distance);
        doubles(out,Shapes.collide(axis,moving,ids.stream().map(shapes::get).toList(),distance));
    }
    static net.minecraft.world.item.ItemStack heldStack() {
        // These queries need item identity only; component registries are normally
        // bound by a world datapack reload, which this geometry harness does not run.
        try {
            var constructor=net.minecraft.world.item.ItemStack.class.getDeclaredConstructor(net.minecraft.core.Holder.class,int.class,net.minecraft.core.component.PatchedDataComponentMap.class);
            constructor.setAccessible(true);
            return constructor.newInstance(net.minecraft.world.item.Items.STICK.builtInRegistryHolder(),1,new net.minecraft.core.component.PatchedDataComponentMap(net.minecraft.core.component.DataComponentMap.EMPTY));
        }catch(ReflectiveOperationException e){throw new RuntimeException(e);}
    }
    static class Context extends EntityCollisionContext {
        Context(boolean descending,boolean placement,double y,boolean fluid){super(descending,placement,y,heldStack(),fluid,null);}
    }
    static void context(int id,int kind,boolean descending,boolean placement,double y,boolean fluid,boolean defaultValue)throws Exception {
        CollisionContext context=switch(kind){case 0->CollisionContext.empty();case 1->CollisionContext.emptyWithFluidCollisions();case 2->CollisionContext.positionContext(y);case 3->new Context(descending,placement,y,fluid);default->throw new AssertionError();};
        out.writeByte(14);out.writeInt(id);out.writeByte(kind);out.writeBoolean(descending);out.writeBoolean(placement);doubles(out,y);out.writeBoolean(fluid);out.writeBoolean(defaultValue);
        out.writeBoolean(context.isDescending());out.writeBoolean(context.isPlacement());out.writeBoolean(context.alwaysCollideWithFluid());out.writeBoolean(context.isAbove(shapes.get(id),new BlockPos(0,5,0),defaultValue));
        out.writeBoolean(context.isHoldingItem(net.minecraft.world.item.Items.STICK));out.writeBoolean(context.isHoldingItem(net.minecraft.world.item.Items.STONE));
        out.writeBoolean(context.canStandOnFluid(net.minecraft.world.level.material.Fluids.EMPTY.defaultFluidState(),net.minecraft.world.level.material.Fluids.WATER.defaultFluidState()));
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
        out.writeByte(15);out.writeInt(id);
        for(var point:List.of(new Vec3(-2,0,0.5),new Vec3(0.5,0.5,0.5),new Vec3(1,1,1),new Vec3(2,-0.0,-3))) {
            doubles(out,point.x,point.y,point.z);var closest=shapes.get(id).closestPointTo(point);out.writeBoolean(closest.isPresent());
            if(closest.isPresent()){var p=closest.get();doubles(out,p.x,p.y,p.z);}
        }
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
    /** Independent registry-cache capture, used by ferraloom-data parity tests. */
    static void registryCaches(Path target)throws Exception {
        var identities=new IdentityHashMap<VoxelShape,Integer>();var values=new ArrayList<VoxelShape>();
        var rows=new ArrayList<int[]>();var contexts=new ArrayList<Boolean>();
        for(var block:BuiltInRegistries.BLOCK)for(var state:block.getStateDefinition().getPossibleStates()) {
            if(Block.getId(state)!=rows.size())throw new AssertionError("state order");
            var queries=new ArrayList<VoxelShape>();
            queries.add(state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO));
            queries.add(state.getShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO));
            queries.add(state.getOcclusionShape());
            for(var direction:Direction.values())queries.add(state.getFaceOcclusionShape(direction));
            int[] row=new int[9];
            for(int i=0;i<9;i++) {
                var shape=queries.get(i);Integer index=identities.get(shape);
                if(index==null){index=values.size();identities.put(shape,index);values.add(shape);}row[i]=index;
            }
            rows.add(row);
            contexts.add(block.hasDynamicShape()||state.hasOffsetFunction()||block instanceof net.minecraft.world.level.block.LiquidBlock||block instanceof net.minecraft.world.level.block.ScaffoldingBlock||block instanceof net.minecraft.world.level.block.PowderSnowBlock||block instanceof net.minecraft.world.level.block.LightBlock);
        }
        if(values.size()>65535)throw new AssertionError("shape index overflow");
        try(var stream=new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(target)))) {
            out=stream;out.writeInt(0x53484e31);out.writeInt(values.size());
            for(var shape:values)result(shape);
            out.writeInt(rows.size());out.writeByte(9);
            for(int i=0;i<rows.size();i++){out.writeBoolean(contexts.get(i));for(int index:rows.get(i))out.writeShort(index);}
        }
        System.out.printf("registry_cache_states=%d instances=%d snapshots=%d%n",rows.size(),values.size(),snapshots.size());
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        if(args.length>1&&args[1].equals("--registry-caches")){registryCaches(Path.of(args[0]));return;}
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
            for(double minimum:new double[]{-1,0,0.125,0.5,1})for(double width:new double[]{0,Double.MIN_VALUE,Math.nextDown(1e-7),1e-7,Math.nextUp(1e-7),1e-6})for(int axis=0;axis<3;axis++) {
                double[] low={0,0,0},high={1,1,1};low[axis]=minimum;high[axis]=minimum+width;
                optimize(box(low[0],low[1],low[2],high[0],high[1],high[2]));
            }
            int cube=box(0.125,0.25,0.5,0.625,0.75,1);
            int slice=face(cube,Direction.SOUTH);
            for(int id:List.of(empty,block,cube,slice))for(var rotation:OctahedralGroup.values()) {
                rotate(id,rotation,new Vec3(0.5,0.5,0.5));rotate(id,rotation,new Vec3(-0.0,0.25,-0.125));
            }
            for(int a:candidates)for(int op=0;op<16;op++){join(a,a,op,false);join(a,empty,op,false);join(empty,a,op,false);}
            for(int a:List.of(block,cube,slice,candidates.get(9)))for(int op=0;op<16;op++){join(infinity,a,op,false);join(a,infinity,op,false);}
            var unique=new HashSet<Integer>();int states=0;
            for(var blockType:BuiltInRegistries.BLOCK)for(var state:blockType.getStateDefinition().getPossibleStates()) {
                int collision=imported(state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO));
                int outline=imported(state.getShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO));
                int occlusion=imported(state.getOcclusionShape());
                unique.add(collision);unique.add(outline);unique.add(occlusion);
                out.writeByte(10);out.writeInt(Block.getId(state));out.writeInt(collision);out.writeInt(outline);out.writeInt(occlusion);states++;
            }
            for(int id:unique) {optimize(id);faces(id,block);probes(id,random);}
            var registryIds=new ArrayList<>(unique);Collections.sort(registryIds);
            for(int i=0;i<registryIds.size();i++) {
                faces(registryIds.get(i),registryIds.get((i+1)%registryIds.size()));
                faces(registryIds.get(i),registryIds.get(i));
            }
            for(int axis=0;axis<3;axis++) {
                double[] left={1,1,1},right={0,0,0};left[axis]=0.5;right[axis]=0.5;
                int a=box(0,0,0,left[0],left[1],left[2]),b=box(right[0],right[1],right[2],1,1,1);
                faces(a,b);faces(b,a);faces(a,a);faces(b,b);
            }
            // Touching faces on both sides of the tolerance and subshape slices.
            for(double gap:new double[]{Math.nextDown(1e-7),1e-7,Math.nextUp(1e-7),2e-7}) {
                int a=box(0,0,0,0.5,1,1), b=box(0.5+gap,0,0,1,1,1);
                for(int op=0;op<16;op++)join(a,b,op,false);
            }
            for(int i=1;i<candidates.size();i++)for(var axis:Direction.Axis.values())for(int flags=0;flags<4;flags++)for(int cost:new int[]{1,257}) {
                // Empty lists are excluded from the cube merger's nonzero gcd contract.
                int a=candidates.get(i),b=candidates.get(1+(i*13)%(candidates.size()-1));
                merger(a,b,axis,cost,(flags&1)!=0,(flags&2)!=0);
                merger(a,a,axis,cost,(flags&1)!=0,(flags&2)!=0);
            }
            for(var axis:Direction.Axis.values())for(double distance:new double[]{-2,-1e-7,Math.nextUp(-1e-7),-0.0,Math.nextDown(1e-7),1e-7,2,Double.NaN}) {
                iterableCollision(List.of(),axis,new AABB(-1,0,0,0,1,1),distance);
                iterableCollision(List.of(empty),axis,new AABB(-1,0,0,0,1,1),distance);
                iterableCollision(List.of(empty,block,candidates.get(7),infinity),axis,new AABB(-1,0,0,0,1,1),distance);
            }
            for(double gap:new double[]{-1e-7,Math.nextDown(-1e-7),Math.nextUp(-1e-7),-0.0,0,Math.nextDown(1e-7),1e-7,Math.nextUp(1e-7)})for(var axis:Direction.Axis.values()) {
                double[] low={0.25,0.25,0.25},high={0.75,0.75,0.75};
                low[axis.ordinal()]=-1;high[axis.ordinal()]=gap;
                for(double distance:new double[]{1e-7,0.25,1,2})collide(block,axis,new AABB(low[0],low[1],low[2],high[0],high[1],high[2]),distance);
                low[axis.ordinal()]=1+gap;high[axis.ordinal()]=2;
                for(double distance:new double[]{-1e-7,-0.25,-1,-2})collide(block,axis,new AABB(low[0],low[1],low[2],high[0],high[1],high[2]),distance);
            }
            for(int id:List.of(empty,block,candidates.get(7)))for(double y:new double[]{-Double.MAX_VALUE,5,6-9.999999747378752E-6,Math.nextUp(6-9.999999747378752E-6),6,Double.NaN})for(int kind=0;kind<4;kind++)for(int flags=0;flags<16;flags++)context(id,kind,(flags&1)!=0,(flags&2)!=0,y,(flags&4)!=0,(flags&8)!=0);
            for(var pair:List.of(new double[][]{{-0.0,1},{0.0,1}},new double[][]{{Double.NaN,1},{Double.NaN,1}},new double[][]{{0,Double.NaN},{0,Double.NaN}},new double[][]{{Double.NEGATIVE_INFINITY,Double.POSITIVE_INFINITY},{Double.NEGATIVE_INFINITY,Double.POSITIVE_INFINITY}}))for(int flags=0;flags<4;flags++) {
                literalMerger(pair[0],pair[1],false,(flags&1)!=0,(flags&2)!=0);
                literalMerger(pair[0],pair[0],true,(flags&1)!=0,(flags&2)!=0);
            }
            out.writeByte(255);
            System.out.printf("states=%d unique_registry_shapes=%d shape_results=%d%n",states,unique.size(),shapes.size());
        }
    }
}
