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
            if (!Set.of("minecraft:worldgen/biome","minecraft:dimension_type","minecraft:damage_type","minecraft:chat_type","minecraft:dialog","minecraft:world_clock","minecraft:timeline","minecraft:waypoint_style").contains(l.key().identifier().toString())) return;
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
    static void sample(Path output,String label,String name,java.util.function.Consumer<RegistryFriendlyByteBuf> write) throws Exception {
        var b=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);
        try {write.accept(b);byte[] bytes=new byte[b.readableBytes()];b.readBytes(bytes);vector(output,label,"net.minecraft.network.protocol.game."+name,bytes);}
        finally {b.release();}
    }
    static void text(RegistryFriendlyByteBuf b,String value) {
        net.minecraft.network.chat.ComponentSerialization.TRUSTED_STREAM_CODEC.encode(b,net.minecraft.network.chat.Component.literal(value));
    }
    static void edgeCases(Path output) throws Exception {
        sample(output,"serverbound_EditBook_edge_max","ServerboundEditBookPacket",b->{
            b.writeVarInt(300);b.writeVarInt(100);for(int i=0;i<100;i++)b.writeUtf("p".repeat(1024));b.writeBoolean(true);b.writeUtf("t".repeat(32));
        });
        sample(output,"serverbound_SignUpdate_edge_max","ServerboundSignUpdatePacket",b->{
            b.writeBlockPos(new BlockPos(-33554432,-2048,33554431));for(int i=0;i<4;i++)b.writeUtf("x".repeat(384));b.writeVarInt(1);
        });
        sample(output,"serverbound_ContainerClick_edge_max_slots","ServerboundContainerClickPacket",b->{
            b.writeVarInt(300);b.writeVarInt(Integer.MAX_VALUE);b.writeShort(-999);b.writeByte(-1);b.writeVarInt(6);b.writeVarInt(128);
            for(int i=0;i<128;i++){b.writeShort(i);b.writeBoolean(false);}b.writeBoolean(false);
        });
        sample(output,"serverbound_Chat_edge_signature","ServerboundChatPacket",b->{
            b.writeUtf("m".repeat(256));b.writeLong(Long.MAX_VALUE);b.writeLong(Long.MIN_VALUE);b.writeBoolean(true);b.writeBytes(new byte[256]);b.writeVarInt(123);b.writeBytes(new byte[]{-1,-1,15});b.writeByte(127);
        });
        sample(output,"serverbound_ChatCommandSigned_edge_max","ServerboundChatCommandSignedPacket",b->{
            b.writeUtf("say hello");b.writeLong(12345);b.writeLong(-987);b.writeVarInt(8);
            for(int i=0;i<8;i++){b.writeUtf("n".repeat(15)+i);b.writeBytes(new byte[256]);}b.writeVarInt(2);b.writeBytes(new byte[]{-1,-1,15});b.writeByte(1);
        });
        for(int flags=0;flags<4;flags++){
            final int f=flags;
            sample(output,"serverbound_MovePlayerPosRot_edge_flags_"+flags,"ServerboundMovePlayerPacket$PosRot",b->{b.writeDouble(-30000000.5);b.writeDouble(-64.125);b.writeDouble(30000000.5);b.writeFloat(179.5f);b.writeFloat(-89.5f);b.writeByte(f);});
        }
        for(int id=0;id<6;id++){
            final int operation=id;
            sample(output,"clientbound_BossEvent_edge_operation_"+id,"ClientboundBossEventPacket",b->{
                b.writeUUID(new UUID(123,-456));b.writeVarInt(operation);
                switch(operation){case 0:text(b,"Boss");b.writeFloat(.75f);b.writeVarInt(6);b.writeVarInt(4);b.writeByte(7);break;case 2:b.writeFloat(.125f);break;case 3:text(b,"Renamed");break;case 4:b.writeVarInt(5);b.writeVarInt(3);break;case 5:b.writeByte(7);break;}
            });
        }
        sample(output,"clientbound_LightUpdate_edge_present","ClientboundLightUpdatePacket",b->{
            b.writeVarInt(-123);b.writeVarInt(456);var bits=new BitSet();bits.set(0);bits.set(65);b.writeBitSet(bits);b.writeBitSet(bits);b.writeBitSet(new BitSet());b.writeBitSet(new BitSet());
            for(int j=0;j<2;j++){b.writeVarInt(2);byte[] layer=new byte[2048];Arrays.fill(layer,(byte)0xa5);b.writeByteArray(layer);b.writeByteArray(new byte[2048]);}
        });
        sample(output,"clientbound_MoveEntityPosRot_edge_stepped","ClientboundMoveEntityPacket$PosRot",b->{
            b.writeVarInt(300);b.writeVarInt(5);b.writeVarInt(1);b.writeShort(Short.MIN_VALUE);b.writeShort(128);b.writeShort(Short.MAX_VALUE);b.writeVarInt(300);b.writeShort(32);b.writeShort(-64);b.writeShort(0);b.writeByte(127);b.writeByte(-128);
        });
        for(double value:new double[]{0.,.00001,.5,3.,3.0001,1000.,1.7179869183E10,-1.7179869183E10,Double.NaN,Double.POSITIVE_INFINITY}){
            sample(output,"clientbound_SetEntityMotion_edge_"+Double.toString(value).replace('.','_'),"ClientboundSetEntityMotionPacket",b->{
                b.writeVarInt(123);LpVec3.write(b,new net.minecraft.world.phys.Vec3(value,-value,value*.5));
            });
        }
        for(int entries:new int[]{1,2,300}){
            var factory=net.minecraft.world.level.chunk.PalettedContainerFactory.create(registries);
            var section=new net.minecraft.world.level.chunk.LevelChunkSection(factory);
            for(int i=0;i<entries-1;i++)section.setBlockState(i&15,(i>>8)&15,(i>>4)&15,net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.byId(i+1));
            sample(output,"clientbound_LevelChunkWithLight_edge_palette_"+entries,"ClientboundLevelChunkWithLightPacket",b->{
                b.writeInt(-23);b.writeInt(45);b.writeVarInt(1);b.writeVarInt(1);b.writeLongArray(new long[]{0,Long.MIN_VALUE,Long.MAX_VALUE});
                var data=new FriendlyByteBuf(Unpooled.buffer());section.write(data);byte[] bytes=new byte[data.readableBytes()];data.readBytes(bytes);data.release();b.writeByteArray(bytes);
                b.writeVarInt(1);b.writeByte(0x9f);b.writeShort(-64);b.writeVarInt(0);b.writeNbt(null);
                for(int i=0;i<4;i++)b.writeBitSet(new BitSet());b.writeVarInt(0);b.writeVarInt(0);
            });
        }
        sample(output,"clientbound_MapItemData_edge_patch","ClientboundMapItemDataPacket",b->{
            b.writeVarInt(321);b.writeByte(4);b.writeBoolean(true);b.writeBoolean(true);b.writeVarInt(1);b.writeVarInt(0);b.writeByte(-128);b.writeByte(127);b.writeByte(255);b.writeBoolean(true);text(b,"marker");b.writeByte(128);b.writeByte(128);b.writeByte(0);b.writeByte(0);b.writeByteArray(new byte[16384]);
        });
        sample(output,"clientbound_PlayerInfoUpdate_edge_all_actions","ClientboundPlayerInfoUpdatePacket",b->{
            b.writeByte(255);b.writeVarInt(1);b.writeUUID(new UUID(123,456));b.writeUtf("player");b.writeVarInt(16);
            for(int i=0;i<16;i++){b.writeUtf("prop"+i);b.writeUtf("value"+i);b.writeBoolean(true);b.writeUtf("signature"+i);}
            b.writeBoolean(false);b.writeVarInt(3);b.writeBoolean(true);b.writeVarInt(Integer.MAX_VALUE);b.writeBoolean(true);text(b,"Display");b.writeVarInt(-1);b.writeBoolean(true);
        });
        // Delimiter identity, completion, and size limits are checked against vanilla's bundler.
        var protocol=net.minecraft.network.protocol.game.GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(registries));
        var in=Unpooled.wrappedBuffer(new byte[]{0});var delimiter=protocol.codec().decode(in);in.release();
        var bundler=protocol.bundlerInfo().startPacketBundling(delimiter);
        for(int i=0;i<4096;i++)if(bundler.addPacket(new net.minecraft.network.protocol.game.ClientboundTickingStepPacket(i))!=null)throw new AssertionError();
        try{bundler.addPacket(new net.minecraft.network.protocol.game.ClientboundTickingStepPacket(0));throw new AssertionError("bundle limit not enforced");}catch(IllegalStateException expected){}
        if(bundler.addPacket(delimiter)==null)throw new AssertionError("bundle did not close");
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
            if(!Set.of("minecraft:chat_type","minecraft:dialog","minecraft:sound_event","minecraft:trim_pattern","minecraft:point_of_interest_type","minecraft:game_event","minecraft:worldgen/biome").contains(e.key().identifier().toString()))return;
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
            catch(Exception e){throw new IllegalStateException(cols[0],e);}
        }
        edgeCases(output);
        Files.write(output.resolve("packets.tsv"),packetRows);
        System.out.println("Encoded "+count+" packet samples plus bundle delimiter");
    }
}
