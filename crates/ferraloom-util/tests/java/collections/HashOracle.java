import java.io.*;
import java.util.UUID;
import net.minecraft.core.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.*;
import net.minecraft.resources.Identifier;

public class HashOracle {
    record Pair(int value,String name) {}
    public static void main(String[] args) throws Exception {
        try(PrintWriter out=new PrintWriter(args[0])) {
            var comparable=java.util.HashMap.class.getDeclaredMethod("comparableClassFor",Object.class);
            comparable.setAccessible(true);
            out.println("comparable "+(comparable.invoke(null,new BlockPos(0,0,0))==null ? 0:1)+" "+(comparable.invoke(null,new Vec3i(0,0,0))==null ? 0:1));
            String[] strings={"","abc","Aa","BB","😀\uD800","\ufffffoo"};
            for(int i=0;i<strings.length;i++)out.println("string "+i+" "+strings[i].hashCode());
            long[] longs={0,1,-1,Long.MIN_VALUE,Long.MAX_VALUE,0x123456789abcdefL,-1234567890123456L};
            for(long v:longs) {
                out.println("long "+v+" "+Long.hashCode(v));
                out.println("uuid "+v+" "+new UUID(v,Long.rotateLeft(v,17)).hashCode());
            }
            for(int x:new int[]{0,1,-1,Integer.MIN_VALUE,Integer.MAX_VALUE,33554431}) {
                int y=x*31,z=x^0x12345678;
                out.println("pos "+x+" "+new Vec3i(x,y,z).hashCode()+" "+new BlockPos(x,y,z).hashCode()+" "+SectionPos.of(x,y,z).hashCode()+" "+new ChunkPos(x,z).hashCode());
                out.println("record "+x+" "+new Pair(x,"abc").hashCode());
            }
            double[] doubles={0.0,-0.0,1.25,-7.5,Double.NaN,Double.POSITIVE_INFINITY,Double.NEGATIVE_INFINITY};
            for(int i=0;i<doubles.length;i++) {
                double x=doubles[i],y=doubles[(i+1)%doubles.length],z=doubles[(i+2)%doubles.length];
                out.println("geometry "+i+" "+new Vec3(x,y,z).hashCode()+" "+new AABB(x,y,z,z,x,y).hashCode());
            }
            out.println("identifier "+Identifier.fromNamespaceAndPath("minecraft","stone").hashCode());
            out.println("boolean "+Boolean.hashCode(true)+" "+Boolean.hashCode(false));
        }
    }
}
