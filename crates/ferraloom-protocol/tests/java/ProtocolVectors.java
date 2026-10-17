import io.netty.buffer.*;
import io.netty.channel.embedded.EmbeddedChannel;
import net.minecraft.util.Crypt;
import javax.crypto.spec.SecretKeySpec;
import java.math.BigInteger;
import java.nio.file.*;
import java.util.*;
import java.util.function.Consumer;
import net.minecraft.network.*;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.chat.*;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;

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
        for (int size : new int[]{255,256,257}) {
            byte[] plain = new byte[size]; for (int j=0;j<size;j++) plain[j] = (byte)(j%13);
            vector("compression_"+size, b -> {
                var channel = new EmbeddedChannel(new CompressionEncoder(256));
                channel.writeOutbound(Unpooled.wrappedBuffer(plain));
                ByteBuf compressed = channel.readOutbound(); b.writeBytes(compressed); compressed.release(); channel.finishAndReleaseAll();
            });
        }
        byte[] secret = new byte[16]; for (int i=0;i<16;i++) secret[i] = (byte)i;
        byte[] plain = new byte[257]; for (int i=0;i<plain.length;i++) plain[i] = (byte)(i%13);
        var key = new SecretKeySpec(secret,"AES");
        byte[] cipher = Crypt.getCipher(1,key).doFinal(plain);
        vector("cipher",b -> b.writeBytes(cipher));
        var keyPair = Crypt.generateKeyPair();
        vector("rsa_private",b -> b.writeBytes(keyPair.getPrivate().getEncoded()));
        vector("rsa_public",b -> b.writeBytes(keyPair.getPublic().getEncoded()));
        var encrypted = Crypt.encryptUsingKey(keyPair.getPublic(),secret);
        vector("rsa_secret",b -> b.writeBytes(encrypted));
        Files.writeString(output.resolve("server_hash.txt"),new BigInteger(Crypt.digestData("",keyPair.getPublic(),key)).toString(16)+"\n");
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var lookup = VanillaRegistries.createWorldLookup();
        var registryOps = lookup.createSerializationContext(JsonOps.INSTANCE);
        String[] components = {
            "\"literal\"",
            "{\"text\":\"root\",\"bold\":true,\"color\":\"red\",\"extra\":[{\"text\":\"child\",\"bold\":false,\"italic\":true,\"hover_event\":{\"action\":\"show_text\",\"value\":{\"translate\":\"tooltip.test\",\"with\":[\"arg\",2]}}}]}",
            "{\"translate\":\"chat.type.text\",\"fallback\":\"%s: %s\",\"with\":[\"Doc\",{\"text\":\"hi\",\"underlined\":true}],\"shadow_color\":-16777216,\"font\":\"minecraft:default\",\"click_event\":{\"action\":\"run_command\",\"command\":\"/help\"}}",
            "{\"nbt\":\"foo.bar\",\"storage\":\"minecraft:test\",\"plain\":true,\"separator\":\", \"}",
            "{\"sprite\":\"minecraft:item/diamond\",\"fallback\":\"diamond\"}",
            "{\"selector\":\"@a\",\"separator\":\", \",\"hover_event\":{\"action\":\"show_entity\",\"id\":\"minecraft:pig\",\"uuid\":\"01234567-89ab-cdef-fedc-ba9876543210\",\"name\":\"pig\"}}"
        };
        for (int i=0;i<components.length;i++) {
            var component = ComponentSerialization.CODEC.parse(registryOps,JsonParser.parseString(components[i])).getOrThrow();
            var finalComponent = component;
            if (i == 1 || i == 2) vector("style_"+i,b -> Style.Serializer.TRUSTED_STREAM_CODEC.encode(new RegistryFriendlyByteBuf(b, RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)), finalComponent.getStyle()));
            vector("component_"+i, b -> ComponentSerialization.TRUSTED_STREAM_CODEC.encode(new RegistryFriendlyByteBuf(b, RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)), finalComponent));
            Files.writeString(output.resolve("component_"+i+".json"),ComponentSerialization.CODEC.encodeStart(registryOps,component).getOrThrow().toString()+"\n");
        }
        vector("max_string", b -> b.writeUtf("\uD83D\uDE80".repeat(16383) + "x", 32767));
    }
}
