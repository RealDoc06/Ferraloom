import io.netty.buffer.*;
import java.nio.file.*;
import java.util.*;
import com.mojang.serialization.Lifecycle;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.*;
import net.minecraft.network.codec.StreamCodec;

/** Runs vanilla's packet codecs. Fixtures are emitted by the real encoder after decoding a neutral seed. */
public final class PlayVectors {
    static RegistryAccess registries;
    @SuppressWarnings({"rawtypes", "unchecked"})
    static RegistryAccess createRegistries() {
        var lookup = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p -> p.apply());
        Map values = new HashMap();
        RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY).registries().forEach(e -> values.put(e.key(),e.value()));
        lookup.listRegistries().forEach(l -> {
            if (values.containsKey(l.key())) return;
            if (!Set.of("minecraft:biome","minecraft:dimension_type","minecraft:damage_type","minecraft:chat_type","minecraft:dialog","minecraft:world_clock","minecraft:timeline","minecraft:waypoint_style").contains(l.key().identifier().toString())) return;
            var registry = new MappedRegistry(l.key(),Lifecycle.stable());
            l.listElements().forEach(h -> registry.register(h.key(),h.value(),RegistrationInfo.BUILT_IN));
            registry.freeze();
            values.put(l.key(),registry);
        });
        return new RegistryAccess.ImmutableRegistryAccess(values);
    }
    @SuppressWarnings({"rawtypes", "unchecked"})
    static void vector(Path output, String label, String className, byte[] seed) throws Exception {
        var codec = (StreamCodec)Class.forName(className).getField(label.startsWith("clientbound_CustomPayload")?"GAMEPLAY_STREAM_CODEC":"STREAM_CODEC").get(null);
        var input = new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(seed),registries);
        var encoded = new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);
        try {
            Object packet=null;
            for (int attempt=0;attempt<32;attempt++) {
                try {input.readerIndex(0);packet=codec.decode(input);break;}
                catch(Exception e){
                    if (!Objects.toString(e.getMessage(),"").contains("Expected non-null compound tag")) throw e;
                    int offset=input.readerIndex()-1;
                    if(offset<0 || seed[offset]!=0)throw e;
                    seed[offset]=(byte)(label.equals("clientbound_BlockEntityData")?10:8);
                }
            }
            if(packet==null)throw new IllegalStateException("Could not create seed");
            codec.encode(encoded,packet);
            byte[] bytes=new byte[encoded.readableBytes()];encoded.readBytes(bytes);
            Files.writeString(output.resolve(label+".hex"),HexFormat.of().formatHex(bytes)+"\n");
        } finally {input.release();encoded.release();}
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); registries=createRegistries();
        Path output=Path.of(args[0]); Files.createDirectories(output);
        int count=0;
        for (String row:Files.readAllLines(Path.of(args[1]))) {
            if(row.isBlank() || row.startsWith("#"))continue;
            var cols=row.split("\\t",-1);
            byte[] seed=cols.length>2?HexFormat.of().parseHex(cols[2]):new byte[65536];
            if(cols[0].endsWith("CustomPayload"))seed=HexFormat.of().parseHex("0f6d696e6563726166743a6272616e640776616e696c6c61");
            if(cols[0].equals("serverbound_CustomClickAction"))seed=new byte[]{0,1,0};
            if(cols[0].equals("clientbound_SetEntityData"))seed=new byte[]{0,(byte)255};
            if(cols[0].equals("serverbound_ChatSessionUpdate")){
                var b=new FriendlyByteBuf(Unpooled.buffer());
                b.writeUUID(new UUID(0,0));b.writeLong(0);
                b.writeByteArray(java.security.KeyPairGenerator.getInstance("RSA").generateKeyPair().getPublic().getEncoded());b.writeByteArray(new byte[0]);
                seed=new byte[b.readableBytes()];b.readBytes(seed);b.release();
            }
            try {vector(output,cols[0],cols[1],seed);count++;}
            catch(Exception e){System.err.println(cols[0]+": "+e.getClass().getSimpleName()+": "+e.getMessage());}
        }
        System.out.println("Encoded "+count+" packet samples");
    }
}
