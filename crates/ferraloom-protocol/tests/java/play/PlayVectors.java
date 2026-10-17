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
    static StreamCodec clientboundPackets;
    static StreamCodec serverboundPackets;
    static List<String> packetRows=new ArrayList<>();
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
            var full=Unpooled.buffer();
            (label.startsWith("clientbound_")?clientboundPackets:serverboundPackets).encode(full,packet);
            byte[] packetBytes=new byte[full.readableBytes()];full.readBytes(packetBytes);full.release();
            packetRows.add(label+"\t"+HexFormat.of().formatHex(packetBytes));
        } finally {input.release();encoded.release();}
    }
    static <T> void registryRows(RegistryAccess.RegistryEntry<T> e,List<String> rows) {
        e.value().entrySet().forEach(v -> rows.add(e.key().identifier()+"\t"+e.value().getId(v.getValue())+"\t"+v.getKey().identifier()));
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); registries=createRegistries();
        Path output=Path.of(args[0]); Files.createDirectories(output);
        clientboundPackets=net.minecraft.network.protocol.game.GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(registries)).codec();
        serverboundPackets=net.minecraft.network.protocol.game.GameProtocols.SERVERBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(registries),new net.minecraft.network.protocol.game.GameProtocols.Context(){public boolean hasInfiniteMaterials(){return true;}public boolean canUseCommandBlocks(){return true;}}).codec();
        var registryRows=new ArrayList<String>();
        registries.registries().forEach(e -> {
            if(!Set.of("minecraft:chat_type","minecraft:dialog","minecraft:sound_event","minecraft:trim_pattern","minecraft:point_of_interest_type","minecraft:game_event","minecraft:biome").contains(e.key().identifier().toString()))return;
            registryRows(e,registryRows);
        });
        Collections.sort(registryRows);Files.write(output.resolve("registries.tsv"),registryRows);
        var delimiter=Unpooled.buffer();
        var playProtocol=net.minecraft.network.protocol.game.GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(registries));
        var delimiterSeed=Unpooled.wrappedBuffer(new byte[]{0});
        var delimiterPacket=playProtocol.codec().decode(delimiterSeed);delimiterSeed.release();
        playProtocol.codec().encode(delimiter,delimiterPacket);
        byte[] delimiterBytes=new byte[delimiter.readableBytes()];delimiter.readBytes(delimiterBytes);delimiter.release();
        packetRows.add("clientbound_BundleDelimiter\t"+HexFormat.of().formatHex(delimiterBytes));
        Files.writeString(output.resolve("clientbound_BundleDelimiter.hex"),HexFormat.of().formatHex(Arrays.copyOfRange(delimiterBytes,1,delimiterBytes.length))+"\n");
        int count=0;
        for (String row:Files.readAllLines(Path.of(args[1]))) {
            if(row.isBlank() || row.startsWith("#"))continue;
            var cols=row.split("\\t",-1);
            byte[] seed=cols.length>2?HexFormat.of().parseHex(cols[2]):new byte[65536];
            if(cols[0].equals("clientbound_DisguisedChat"))seed=HexFormat.of().parseHex("0800000108000000");
            if(cols[0].equals("clientbound_PlayerChat")){
                var b=new FriendlyByteBuf(Unpooled.buffer());
                b.writeVarInt(0);b.writeUUID(new UUID(0,0));b.writeVarInt(0);b.writeBoolean(false);
                b.writeUtf("");b.writeLong(0);b.writeLong(0);b.writeVarInt(0);b.writeBoolean(false);b.writeVarInt(0);
                b.writeVarInt(1);net.minecraft.network.chat.ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC.encode(b,net.minecraft.network.chat.Component.literal("sender"));b.writeBoolean(false);
                seed=new byte[b.readableBytes()];b.readBytes(seed);b.release();
            }
            if(cols[0].equals("clientbound_ShowDialog")){
                var dialog=new net.minecraft.server.dialog.NoticeDialog(new net.minecraft.server.dialog.CommonDialogData(net.minecraft.network.chat.Component.literal("Title"),Optional.empty(),true,true,net.minecraft.server.dialog.DialogAction.CLOSE,List.of(),List.of()),net.minecraft.server.dialog.NoticeDialog.DEFAULT_ACTION);
                var b=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);
                net.minecraft.server.dialog.Dialog.STREAM_CODEC.encode(b,Holder.direct(dialog));
                seed=new byte[b.readableBytes()];b.readBytes(seed);b.release();
            }
            if(cols[0].equals("clientbound_DebugBlockValue")){seed=new byte[10];seed[8]=10;}
            if(cols[0].equals("clientbound_DebugChunkValue")){seed=new byte[10];seed[8]=10;}
            if(cols[0].equals("clientbound_DebugEntityValue"))seed=new byte[]{0,10,0};
            if(cols[0].equals("clientbound_DebugEvent"))seed=new byte[]{10};
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
        Files.write(output.resolve("packets.tsv"),packetRows);
        System.out.println("Encoded "+count+" packet samples plus bundle delimiter");
    }
}
