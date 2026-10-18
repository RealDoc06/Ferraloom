import io.netty.buffer.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.network.FriendlyByteBuf;

/** Native DataOutputStream versus Netty NBT writes, including both DataInput implementations. */
public final class NbtFloatOutputs {
    static byte[] file(Tag value) throws Exception {
        var bytes=new ByteArrayOutputStream();
        NbtIo.writeAnyTag(value,new DataOutputStream(bytes));
        return bytes.toByteArray();
    }
    static byte[] network(Tag value) {
        var buf=Unpooled.buffer();
        try {
            FriendlyByteBuf.writeNbt(buf,value);
            byte[] bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);return bytes;
        } finally {buf.release();}
    }
    static byte[] read(byte[] bytes) throws Exception {
        Tag data=NbtIo.readAnyTag(new DataInputStream(new ByteArrayInputStream(bytes)),NbtAccounter.unlimitedHeap());
        var buf=Unpooled.wrappedBuffer(bytes);
        try {
            Tag netty=NbtIo.readAnyTag(new ByteBufInputStream(buf),NbtAccounter.unlimitedHeap());
            byte[] output=network(data);
            if(!Arrays.equals(output,network(netty))) throw new AssertionError("DataInput bits differ");
            return output;
        } finally {buf.release();}
    }
    static void vector(StringBuilder out,String kind,String bits,Tag scalar) throws Exception {
        var compound=new CompoundTag();compound.put("value",scalar);
        var list=new ListTag();list.add(scalar);list.add(scalar);compound.put("list",list);
        for(String shape:List.of("scalar","nested")) {
            Tag value=shape.equals("scalar") ? scalar:compound;
            byte[] file=file(value),network=network(value);
            out.append(kind).append('\t').append(bits).append('\t').append(shape);
            for(byte[] bytes:List.of(file,network,read(file),read(network))) out.append('\t').append(HexFormat.of().formatHex(bytes));
            out.append('\n');
        }
    }
    @SuppressWarnings("removal")
    public static void main(String[] args) throws Exception {
        var out=new StringBuilder();
        for(int bits:FloatValues.FLOATS) vector(out,"float",Integer.toHexString(bits),new FloatTag(Float.intBitsToFloat(bits)));
        for(long bits:FloatValues.DOUBLES) vector(out,"double",Long.toHexString(bits),new DoubleTag(Double.longBitsToDouble(bits)));
        Files.writeString(Path.of(args[0]),out);
        System.out.println("NBT floating outputs: 72 scalar/nested cases, both output and input paths");
    }
}
