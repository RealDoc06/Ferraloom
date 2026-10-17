// Our reference-vector generator. Compile against the unmodified 26.3 server
// jar and its libraries with JDK 25. Output is Rust constants; no server starts.
import net.minecraft.core.*;
import net.minecraft.util.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.synth.*;
import net.minecraft.world.phys.*;
import java.util.*;

public final class VanillaVectors {
    static final long[] SEEDS = {0, 1, -1, Long.MIN_VALUE, 0x123456789abcdefL};
    static final double[][] POINTS = {{0,0,0}, {-0.125,-64.75,17.3}, {30000000.25,-30000000.75,16777216}, {-33554432.1,0.000000001,33554431.9}, {1.25e12,-1.5e12,1234.5}};
    static void ints(int... values) { System.out.print("["); for (int v : values) System.out.print(v+","); System.out.print("]"); }
    static void longs(long... values) { System.out.print("["); for (long v : values) System.out.print(v+","); System.out.print("]"); }
    static void floats(float... values) { System.out.print("["); for (float v : values) System.out.printf("0x%08x,",Float.floatToRawIntBits(v)); System.out.print("]"); }
    static void doubles(double... values) { System.out.print("["); for (double v : values) System.out.printf("0x%016x,",Double.doubleToRawLongBits(v)); System.out.print("]"); }
    static RandomSource source(int algorithm, long seed) { return switch (algorithm) { case 0 -> new LegacyRandomSource(seed); case 1 -> new SingleThreadedRandomSource(seed); default -> new XoroshiroRandomSource(seed); }; }
    static void randoms() {
        System.out.println("pub const RNG: [(i64, usize, [i32;8], [i64;4], [u64;8], [u32;4], [i32;8]);15] = [");
        for (int algorithm=0;algorithm<3;algorithm++) for (long seed:SEEDS) {
            System.out.print("("+seed+","+algorithm+",");
            RandomSource r=source(algorithm,seed); int[] is=new int[8]; for (int i=0;i<8;i++) is[i]=r.nextInt(); ints(is); System.out.print(",");
            r=source(algorithm,seed); long[] ls=new long[4]; for (int i=0;i<4;i++) ls[i]=r.nextLong(); longs(ls); System.out.print(",");
            r=source(algorithm,seed); double[] ds={r.nextDouble(),r.nextGaussian(),r.nextGaussian(),r.triangle(-2.5,3.0),r.nextDouble(),r.nextGaussian(),r.nextGaussian(),r.nextDouble()}; doubles(ds); System.out.print(",");
            r=source(algorithm,seed); float[] fs={r.nextFloat(),r.nextFloat(),r.triangle(-2.5f,3f),r.nextFloat()}; floats(fs); System.out.print(",");
            r=source(algorithm,seed); int[] bounds={1,2,3,1073741825,2147483647,17,1024,1073741825}; int[] bs=new int[8]; for (int i=0;i<8;i++) bs[i]=r.nextInt(bounds[i]); ints(bs); System.out.println("),");
        }
        System.out.println("];\npub const POSITIONAL: [(i64,usize,[i64;6]);15] = [");
        for (int a=0;a<3;a++) for (long seed:SEEDS) {
            RandomSource r=source(a,seed); PositionalRandomFactory f=r.forkPositional();
            System.out.print("("+seed+","+a+","); longs(f.at(-30000000,-2048,33554431).nextLong(),f.at(0,0,0).nextLong(),f.fromHashOf("minecraft:overworld").nextLong(),f.fromHashOf("\ud83d\ude00\u00e9").nextLong(),f.fromSeed(-1).nextLong(),r.fork().nextLong()); System.out.println("),");
        }
        System.out.println("];\npub const WORLDGEN: [(i64,usize,i64,[i64;5],i32);15] = [");
        for (int a=0;a<3;a++) for (long seed:SEEDS) {
            WorldgenRandom r=new WorldgenRandom(source(a,seed)); long d=r.setDecorationSeed(seed,-12345,98765); long first=r.nextLong();
            r.setFeatureSeed(d,-7,300000); long second=r.nextLong();
            r.setLargeFeatureSeed(seed,-30000000,30000000); long third=r.nextLong();
            r.setLargeFeatureWithSalt(seed,-17,99999,-123); long fourth=r.nextLong();
            System.out.print("("+seed+","+a+","+d+","); longs(first,second,third,fourth,WorldgenRandom.seedSlimeChunk(-30000000,30000000,seed,987234911L).nextLong()); System.out.println(","+r.getCount()+"),");
        }
        System.out.println("];");
    }
    static void noises() {
        System.out.println("pub const NOISE: [(i64,usize,usize,[u32;11]);50] = [");
        for (int a:new int[]{0,2}) for (long seed:SEEDS) {
            PerlinNoise p=new PerlinNoise(source(a,seed)); SimplexNoise s=new SimplexNoise(source(a,seed)); SimplexNoise so=new SimplexNoise(source(a,seed),true);
            NormalNoise n=NormalNoise.builder().setBaseOctave(-5).setOctaveCount(4).setBaseAmplitude(1.7).setAmplitudeModifier(1,0).setAmplitudeModifier(2,0.3).build();
            Noise normal=n.create(source(a,seed)); Noise parity=NormalNoise.createParity(-3,1,0,0.5).create(source(a,seed)); Noise legacy=n.createForLegacyNetherBiome(source(a,seed));
            for (int i=0;i<POINTS.length;i++) {
                double[] q=POINTS[i]; float[] derivative={0.25f,-0.5f,1.5f}; float dv=p.noiseWithDerivative(q[0],q[1],q[2],derivative);
                System.out.print("("+seed+","+a+","+i+","); floats(p.get(q[0],q[1],q[2]),p.get(q[0],q[1]),s.get(q[0],q[1],q[2]),s.get(q[0],q[1]),so.get(q[0],q[1],q[2]),normal.get(q[0],q[1],q[2]),parity.get(q[0],q[1],q[2]),legacy.get(q[0],q[1],q[2]),derivative[0],derivative[1],derivative[2]); System.out.println("),");
                if (Float.floatToIntBits(dv)!=Float.floatToIntBits(p.get(q[0],q[1],q[2]))) throw new AssertionError();
            }
        }
        System.out.println("];");
    }
    static void geometry() {
        int[][] positions={{0,0,0},{-1,-1,-1},{33554431,2047,33554431},{-33554432,-2048,-33554432},{33554432,2048,-33554433},{Integer.MIN_VALUE,Integer.MAX_VALUE,Integer.MIN_VALUE},{30000000,-64,-30000000}};
        System.out.println("pub const PACKING: [([i32;3],i64,[i32;3],i64,[i32;3],i64);7] = [");
        for (int[] v:positions) { BlockPos p=new BlockPos(v[0],v[1],v[2]); BlockPos b=BlockPos.of(p.asLong()); SectionPos s=SectionPos.of(SectionPos.of(v[0],v[1],v[2]).asLong()); System.out.print("("); ints(v); System.out.print(","+p.asLong()+","); ints(b.getX(),b.getY(),b.getZ()); System.out.print(","+SectionPos.of(v[0],v[1],v[2]).asLong()+","); ints(s.x(),s.y(),s.z()); System.out.println(","+new ChunkPos(v[0],v[2]).pack()+"),"); }
        System.out.println("];\npub const BETWEEN: [[i32;3];12] = [");
        for (BlockPos p:BlockPos.betweenClosed(new BlockPos(1,2,3),new BlockPos(-1,1,2))) {ints(p.getX(),p.getY(),p.getZ());System.out.println(",");}
        System.out.println("];\npub const AABB_CLIPS: [Option<[u64;3]>;7] = [");
        AABB box=new AABB(0,0,0,1,1,1); Vec3[][] rays={{new Vec3(-1,0.5,0.5),new Vec3(2,0.5,0.5)},{new Vec3(0.5,0.5,0.5),new Vec3(2,0.5,0.5)},{new Vec3(-1,0.5,0.5),new Vec3(0,0.5,0.5)},{new Vec3(-1,-1,-1),new Vec3(2,2,2)},{new Vec3(0,0.5,0.5),new Vec3(1,0.5,0.5)},{new Vec3(-1,1.00000005,0.5),new Vec3(2,1.00000005,0.5)},{new Vec3(-1,1.0000002,0.5),new Vec3(2,1.0000002,0.5)}};
        for (Vec3[] ray:rays) {Optional<Vec3> hit=box.clip(ray[0],ray[1]); if (hit.isPresent()) {Vec3 p=hit.get(); System.out.print("Some(");doubles(p.x,p.y,p.z);System.out.println("),");} else System.out.println("None,");}
        System.out.println("];");
    }
    static void math() {
        double[] values={0.0,-0.0,-1.00000001,0.125,Math.PI,-12345678.75,1e20,Double.NaN,Double.POSITIVE_INFINITY,Double.NEGATIVE_INFINITY};
        System.out.println("pub const MATH: [(u64,i32,i32,i64,u32,u32,u64,u64,u64);10] = [");
        for (double x:values) {System.out.printf("(0x%016x,%d,%d,%d,0x%08x,0x%08x,0x%016x,0x%016x,0x%016x),%n",Double.doubleToRawLongBits(x),Mth.floor(x),Mth.ceil(x),Mth.lfloor(x),Float.floatToRawIntBits(Mth.sin(x)),Float.floatToRawIntBits(Mth.cos(x)),Double.doubleToRawLongBits(Mth.fastInvSqrt(x)),Double.doubleToRawLongBits(Mth.wrapDegrees(x)),Double.doubleToRawLongBits(Mth.positiveModulo(x,7.25)));}
        System.out.println("];\npub const SEED_HASH: [([i32;3],i64,i32);7] = [");
        for (int x:new int[]{0,1,-1,Integer.MIN_VALUE,Integer.MAX_VALUE,30000000,-30000000}) {System.out.print("(");ints(x,x^0xdeadbeef,-x);System.out.println(","+Mth.getSeed(x,x^0xdeadbeef,-x)+","+Mth.murmurHash3Mixer(x)+"),");}
        System.out.println("];\npub const HASH: [(&str,i32,[i64;2],[i32;4]);4] = [");
        for (String name:new String[]{"","abc","minecraft:overworld","\ud83d\ude00\u00e9"}) {RandomSupport.Seed128bit seed=RandomSupport.seedFromHashOf(name);System.out.print("(\""+name+"\","+name.hashCode()+",");longs(seed.seedLo(),seed.seedHi());System.out.print(",");ints(UUIDUtil.uuidToIntArray(UUIDUtil.createOfflinePlayerUUID(name)));System.out.println("),");}
        System.out.println("];");
    }

    static void edges() {
        System.out.println("pub const DIRECTIONS: [[i32;17];6] = [");
        for (Direction d:Direction.values()) {
            ints(d.get3DDataValue(),d.get2DDataValue(),d.getOpposite().get3DDataValue(),d.getClockWise(Direction.Axis.X).get3DDataValue(),d.getClockWise(Direction.Axis.Y).get3DDataValue(),d.getClockWise(Direction.Axis.Z).get3DDataValue(),d.getCounterClockWise(Direction.Axis.X).get3DDataValue(),d.getCounterClockWise(Direction.Axis.Y).get3DDataValue(),d.getCounterClockWise(Direction.Axis.Z).get3DDataValue(),Rotation.CLOCKWISE_90.rotate(d).get3DDataValue(),Rotation.CLOCKWISE_180.rotate(d).get3DDataValue(),Rotation.COUNTERCLOCKWISE_90.rotate(d).get3DDataValue(),Mirror.LEFT_RIGHT.mirror(d).get3DDataValue(),Mirror.FRONT_BACK.mirror(d).get3DDataValue(),d.getStepX(),d.getStepY(),d.getStepZ()); System.out.println(",");
        }
        System.out.println("];\npub const NEAREST: [([i32;3],i32,i32);8] = [");
        for (int[] v:new int[][]{{0,0,0},{1,1,1},{-1,-1,0},{2,1,1},{0,-2,1},{0,1,-2},{Integer.MIN_VALUE,0,0},{Integer.MAX_VALUE,Integer.MAX_VALUE-1,0}}) {
            Direction exact=Direction.getNearest(v[0],v[1],v[2],null); Direction approx=Direction.getApproximateNearest(v[0],v[1],v[2]);
            System.out.print("("); ints(v); System.out.println(","+(exact==null?-1:exact.get3DDataValue())+","+approx.get3DDataValue()+"),");
        }
        System.out.println("];\npub const DIVISION: [([i32;2],[i32;8]);8] = [");
        for (int[] v:new int[][]{{-7,3},{7,-3},{-7,-3},{Integer.MIN_VALUE,-1},{Integer.MAX_VALUE,17},{-1,32},{0,-8},{12345,1}}) {
            int a=v[0],b=v[1];System.out.print("(");ints(v);System.out.print(",");ints(Math.floorDiv(a,b),Math.floorMod(a,b),a>>b,a>>>b,a<<b,Mth.positiveCeilDiv(a,b),Mth.roundToward(a,b),Mth.wrapDegrees(a));System.out.println("),");
        }
        System.out.println("];\npub const INTERPOLATION: [(u64,[u64;6],[u32;9],[i32;2]);8] = [");
        for (double x:new double[]{-1.25,-0.000001,0,0.125,0.3333333333333333,0.99999999,1.25,Double.NaN}) {
            System.out.printf("(0x%016x,",Double.doubleToRawLongBits(x));
            doubles(Mth.lerp(x,-7.3,11.1),Mth.clampedLerp(x,-7.3,11.1),Mth.inverseLerp(x,-0.3,2.7),Mth.map(x,-0.3,2.7,-7.3,11.1),Mth.clampedMap(x,-0.3,2.7,-7.3,11.1),Mth.rotLerp(x,179.3,-179.7));System.out.print(",");
            float f=(float)x;
            floats(Mth.lerp(f,-7.3f,11.1f),Mth.clampedLerp(f,-7.3f,11.1f),Mth.inverseLerp(f,-0.3f,2.7f),Mth.smoothstep(f),Mth.smoothstepDerivative(f),Mth.fastInvCubeRoot(f),Mth.invSqrt(f),Mth.rotLerpRad(f,3.1f,-3.1f),Mth.catmullrom(f,-3f,7f,-11f,13f));System.out.print(",");ints(Mth.lerpInt(f,Integer.MAX_VALUE,-98765),Mth.lerpDiscrete(f,7,-11));System.out.println("),");
        }
        System.out.println("];\npub const ATAN2: [([u64;2],u64);12] = [");
        for (double[] q:new double[][]{{0,0},{-0.0,1},{1,0},{0,-1},{1,1},{-0.7,3.4},{30000.25,-0.00001},{-12,-7},{1e-15,1e-17},{1e10,1e8},{Double.NaN,1},{Double.POSITIVE_INFINITY,1}}) {
            System.out.print("(");doubles(q);System.out.printf(",0x%016x),%n",Double.doubleToRawLongBits(Mth.atan2(q[0],q[1])));
        }
        System.out.println("];\npub const VECTORS: [[u64;3];9] = [");
        Vec3 v=new Vec3(-12.25,0.000001,30000000.3); Vec3 w=new Vec3(3.7,-9.1,11.2);
        for (Vec3 q:new Vec3[]{v.normalize(),new Vec3(0.0000001,0,0).normalize(),v.cross(w),v.lerp(w,0.37),v.xRot(0.37f),v.yRot(0.37f),v.zRot(0.37f),Vec3.directionFromRotation(-37.25f,179.3f),v.projectedOn(w)}) {doubles(q.x,q.y,q.z);System.out.println(",");}
        System.out.println("];\npub const BOXES: [[u64;6];8] = [");
        AABB b=new AABB(1,-2,3,0,4,-5); AABB c=new AABB(7,8,9,11,13,17);
        for (AABB q:new AABB[]{b,b.intersect(c),b.inflate(-3,0.3,1.1),b.move(0.1,-1e10,17.3),b.contract(-3,0.3,1.1),b.expandTowards(-3,0.3,1.1),new AABB(-0.0,-0.0,-0.0,1,1,1).expandTowards(0,Double.NaN,-0.0),new AABB(-0.0,-0.0,-0.0,1,1,1).contract(0,Double.NaN,-0.0)}) {doubles(q.minX,q.minY,q.minZ,q.maxX,q.maxY,q.maxZ);System.out.println(",");}
        System.out.println("];\npub const RAW_XOROSHIRO: [([i64;2],[i64;6]);3] = [");
        for (long[] seed:new long[][]{{0,0},{-1,1},{Long.MIN_VALUE,Long.MAX_VALUE}}) {Xoroshiro128PlusPlus r=new Xoroshiro128PlusPlus(seed[0],seed[1]); System.out.print("(");longs(seed);System.out.print(",");longs(r.nextLong(),r.nextLong(),r.nextLong(),r.nextLong(),r.nextLong(),r.nextLong());System.out.println("),");}
        System.out.println("];\npub const NOISE_NORMALIZATION: [[u32;5];4] = [");
        for (int mode=0;mode<4;mode++) {
            NormalNoise.Builder builder=NormalNoise.builder().setBaseOctave(-5).setOctaveCount(4).setBaseAmplitude(1.7).setAmplitudeModifier(1,0).setAmplitudeModifier(2,0.3);
            if (mode==0) builder.setNormalize(false); else if (mode==2) builder.setLegacyNormalization(); else if (mode==3) for (int i=0;i<4;i++) builder.setAmplitudeModifier(i,0);
            NormalNoise n=builder.build(); Noise stack=n.create(new XoroshiroRandomSource(-1));
            floats(n.range().min(),n.range().max(),stack.range().min(),stack.range().max(),stack.get(-33554432.1,0.000000001,33554431.9));System.out.println(",");
        }
        System.out.println("];");
    }
    public static void main(String[] args) { randoms(); noises(); geometry(); math(); edges(); }
}
