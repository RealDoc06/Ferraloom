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
    public static void main(String[] args) { randoms(); noises(); geometry(); math(); }
}
