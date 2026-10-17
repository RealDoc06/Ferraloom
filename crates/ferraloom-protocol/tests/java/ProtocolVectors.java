import io.netty.buffer.*;
import java.nio.file.*;
import java.util.*;
import java.util.function.Consumer;
import net.minecraft.network.*;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.*;

/** Executable oracle: only calls vanilla's own wire writers. */
public final class ProtocolVectors {
    static Path output;
    static void vector(String name, Consumer<FriendlyByteBuf> write) throws Exception {
        var buf = new FriendlyByteBuf(Unpooled.buffer());
        try {
            write.accept(buf);
            byte[] bytes = new byte[buf.readableBytes()];
            buf.readBytes(bytes);
            Files.writeString(output.resolve(name + ".hex"), HexFormat.of().formatHex(bytes) + "\n");
        } finally { buf.release(); }
    }
    public static void main(String[] args) throws Exception {
        output = Path.of(args[0]);
        Files.createDirectories(output);
        vector("primitives", b -> {
            for (int v : new int[]{0, 1, 127, 128, 255, 2147483647, -1, -2147483648}) b.writeVarInt(v);
            for (long v : new long[]{0, 127, 128, Long.MAX_VALUE, -1, Long.MIN_VALUE}) b.writeVarLong(v);
            b.writeUtf("a\uD83D\uDE80é", 4);
            b.writeBlockPos(new BlockPos(-33554432, -2048, 33554431));
            b.writeUUID(new UUID(0x0123456789abcdefL, 0xfedcba9876543210L));
            var bits = new BitSet(); bits.set(0); bits.set(9); bits.set(65);
            b.writeBitSet(bits);
            b.writeFixedBitSet(bits, 70);
            var tag = new CompoundTag(); tag.putString("name", "network"); tag.putInt("answer", 42);
            b.writeNbt(tag);
        });
        vector("max_string", b -> b.writeUtf("\uD83D\uDE80".repeat(16383) + "x", 32767));
    }
}
