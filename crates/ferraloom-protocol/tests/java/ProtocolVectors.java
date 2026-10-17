import io.netty.buffer.*;
import io.netty.channel.embedded.EmbeddedChannel;
import net.minecraft.util.Crypt;
import net.minecraft.util.HashOps;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.core.HolderSet;
import net.minecraft.tags.TagKey;
import net.minecraft.world.level.block.Blocks;
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
import net.minecraft.network.protocol.*;
import net.minecraft.network.protocol.handshake.*;
import net.minecraft.network.protocol.status.*;
import net.minecraft.network.protocol.login.*;
import net.minecraft.network.protocol.configuration.*;
import net.minecraft.network.protocol.common.*;
import net.minecraft.network.protocol.cookie.*;
import net.minecraft.network.protocol.ping.*;
import net.minecraft.network.protocol.common.custom.BrandPayload;
import net.minecraft.network.protocol.login.custom.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.core.registries.Registries;
import net.minecraft.core.RegistrySynchronization;
import net.minecraft.core.Holder;
import net.minecraft.server.packs.repository.KnownPack;
import net.minecraft.server.ServerLinks;
import net.minecraft.server.dialog.Dialog;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.tags.TagNetworkSerialization;
import com.mojang.authlib.GameProfile;
import com.mojang.authlib.properties.*;
import com.google.common.collect.ImmutableMultimap;
import it.unimi.dsi.fastutil.ints.IntList;
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
    static <L extends PacketListener> void packet(String name, ProtocolInfo<L> protocol, Packet<? super L> packet) throws Exception {
        vector(name,b -> protocol.codec().encode(b,packet));
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
        var hashOps = lookup.createSerializationContext(HashOps.CRC32C_INSTANCE);
        vector("registry_holders",b -> {
            var buffer = new RegistryFriendlyByteBuf(b,RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
            ByteBufCodecs.registry(Registries.BLOCK).encode(buffer,Blocks.STONE);
            SoundEvent.STREAM_CODEC.encode(buffer,SoundEvents.AMBIENT_CAVE);
            SoundEvent.STREAM_CODEC.encode(buffer,Holder.direct(SoundEvent.createFixedRangeEvent(Identifier.parse("test:direct"),24.5f)));
            ByteBufCodecs.holderSet(Registries.BLOCK).encode(buffer,HolderSet.direct(Blocks.STONE.builtInRegistryHolder(),Blocks.DIRT.builtInRegistryHolder()));
            ByteBufCodecs.holderSet(Registries.BLOCK).encode(buffer,HolderSet.emptyNamed(BuiltInRegistries.BLOCK,TagKey.create(Registries.BLOCK,Identifier.parse("test:named"))));
        });
        byte[][] malformed = {
            {(byte)0xed,(byte)0xa0,(byte)0x80}, {(byte)0xed,(byte)0xa0},
            {(byte)0xed,(byte)0xa0,65}, {(byte)0xe0,(byte)0x80,(byte)0x80},
            {(byte)0xf0,(byte)0x9f,(byte)0x92}, {(byte)0xf4,(byte)0x90,(byte)0x80,(byte)0x80}
        };
        for (int i=0;i<malformed.length;i++) {
            byte[] raw = malformed[i];
            vector("malformed_utf8_input_"+i,b -> { b.writeVarInt(raw.length); b.writeBytes(raw); });
            var input = new FriendlyByteBuf(Unpooled.buffer()); input.writeVarInt(raw.length); input.writeBytes(raw);
            String text = input.readUtf(16); input.release();
            vector("malformed_utf8_normalized_"+i,b -> b.writeUtf(text,16));
        }
        var uuid = UUID.fromString("01234567-89ab-cdef-fedc-ba9876543210");
        var testId = Identifier.parse("minecraft:test");
        var reason = Component.literal("test").withStyle(style -> style.withBold(true));
        var profile = new GameProfile(uuid,"Doc",new PropertyMap(ImmutableMultimap.of("textures",new Property("textures","value","signature"))));
        var registryTag = new CompoundTag(); registryTag.putInt("value",42);
        packet("handshake_sb_intention",HandshakeProtocols.SERVERBOUND,new ClientIntentionPacket(777,"localhost",25565,ClientIntent.LOGIN));
        packet("status_sb_request",StatusProtocols.SERVERBOUND,ServerboundStatusRequestPacket.INSTANCE);
        packet("status_sb_ping",StatusProtocols.SERVERBOUND,new ServerboundPingRequestPacket(-123456789L));
        packet("status_cb_pong",StatusProtocols.CLIENTBOUND,new ClientboundPongResponsePacket(-123456789L));
        var status = new ServerStatus(reason,Optional.of(new ServerStatus.Players(20,1,List.of(new net.minecraft.server.players.NameAndId(uuid,"Doc")))),Optional.of(new ServerStatus.Version("26.3",777)),Optional.of(new ServerStatus.Favicon(new byte[]{1,2,3})),true);
        packet("status_cb_response",StatusProtocols.CLIENTBOUND,new ClientboundStatusResponsePacket(status));
        packet("login_sb_hello",LoginProtocols.SERVERBOUND,new ServerboundHelloPacket("Doc",uuid));
        packet("login_sb_key",LoginProtocols.SERVERBOUND,new ServerboundKeyPacket(key,keyPair.getPublic(),new byte[]{1,2,3,4}));
        packet("login_sb_query",LoginProtocols.SERVERBOUND,new ServerboundCustomQueryAnswerPacket(42,DiscardedQueryAnswerPayload.INSTANCE));
        packet("login_sb_ack",LoginProtocols.SERVERBOUND,ServerboundLoginAcknowledgedPacket.INSTANCE);
        packet("login_sb_cookie",LoginProtocols.SERVERBOUND,new ServerboundCookieResponsePacket(testId,new byte[]{1,2,3}));
        packet("login_cb_disconnect",LoginProtocols.CLIENTBOUND,new ClientboundLoginDisconnectPacket(reason));
        packet("login_cb_hello",LoginProtocols.CLIENTBOUND,new ClientboundHelloPacket("",keyPair.getPublic().getEncoded(),new byte[]{1,2,3,4},true));
        packet("login_cb_finished",LoginProtocols.CLIENTBOUND,new ClientboundLoginFinishedPacket(profile,uuid));
        packet("login_cb_compression",LoginProtocols.CLIENTBOUND,new ClientboundLoginCompressionPacket(256));
        packet("login_cb_query",LoginProtocols.CLIENTBOUND,new ClientboundCustomQueryPacket(42,new DiscardedQueryPayload(testId)));
        packet("login_cb_cookie",LoginProtocols.CLIENTBOUND,new ClientboundCookieRequestPacket(testId));
        var packs = List.of(new KnownPack("minecraft","core","26.3"));
        packet("config_sb_information",ConfigurationProtocols.SERVERBOUND,new ServerboundClientInformationPacket(ClientInformation.createDefault()));
        packet("config_sb_cookie",ConfigurationProtocols.SERVERBOUND,new ServerboundCookieResponsePacket(testId,null));
        packet("config_sb_payload",ConfigurationProtocols.SERVERBOUND,new ServerboundCustomPayloadPacket(new BrandPayload("ferraloom")));
        packet("config_sb_finish",ConfigurationProtocols.SERVERBOUND,ServerboundFinishConfigurationPacket.INSTANCE);
        packet("config_sb_keepalive",ConfigurationProtocols.SERVERBOUND,new ServerboundKeepAlivePacket(-42L));
        packet("config_sb_pong",ConfigurationProtocols.SERVERBOUND,new ServerboundPongPacket(42));
        packet("config_sb_pack",ConfigurationProtocols.SERVERBOUND,new ServerboundResourcePackPacket(uuid,ServerboundResourcePackPacket.Action.ACCEPTED));
        packet("config_sb_knownpacks",ConfigurationProtocols.SERVERBOUND,new ServerboundSelectKnownPacks(packs));
        packet("config_sb_click",ConfigurationProtocols.SERVERBOUND,new ServerboundCustomClickActionPacket(testId,Optional.of(registryTag)));
        packet("config_sb_conduct",ConfigurationProtocols.SERVERBOUND,ServerboundAcceptCodeOfConductPacket.INSTANCE);
        packet("config_cb_cookie",ConfigurationProtocols.CLIENTBOUND,new ClientboundCookieRequestPacket(testId));
        packet("config_cb_payload",ConfigurationProtocols.CLIENTBOUND,new ClientboundCustomPayloadPacket(new BrandPayload("vanilla")));
        packet("config_cb_disconnect",ConfigurationProtocols.CLIENTBOUND,new ClientboundDisconnectPacket(reason));
        packet("config_cb_finish",ConfigurationProtocols.CLIENTBOUND,ClientboundFinishConfigurationPacket.INSTANCE);
        packet("config_cb_keepalive",ConfigurationProtocols.CLIENTBOUND,new ClientboundKeepAlivePacket(-42L));
        packet("config_cb_ping",ConfigurationProtocols.CLIENTBOUND,new ClientboundPingPacket(42));
        packet("config_cb_reset",ConfigurationProtocols.CLIENTBOUND,ClientboundResetChatPacket.INSTANCE);
        packet("config_cb_registry",ConfigurationProtocols.CLIENTBOUND,new ClientboundRegistryDataPacket(ResourceKey.createRegistryKey(testId),List.of(new RegistrySynchronization.PackedRegistryEntry(testId,Optional.of(registryTag)),new RegistrySynchronization.PackedRegistryEntry(Identifier.parse("minecraft:omitted"),Optional.empty()))));
        packet("config_cb_pop",ConfigurationProtocols.CLIENTBOUND,new ClientboundResourcePackPopPacket(Optional.of(uuid)));
        packet("config_cb_push",ConfigurationProtocols.CLIENTBOUND,new ClientboundResourcePackPushPacket(uuid,"https://example.com/pack.zip","abc123",true,Optional.of(reason)));
        packet("config_cb_effects",ConfigurationProtocols.CLIENTBOUND,new ClientboundPostEffectsPacket(List.of(testId)));
        packet("config_cb_store",ConfigurationProtocols.CLIENTBOUND,new ClientboundStoreCookiePacket(testId,new byte[]{1,2,3}));
        packet("config_cb_transfer",ConfigurationProtocols.CLIENTBOUND,new ClientboundTransferPacket("example.com",25565));
        packet("config_cb_features",ConfigurationProtocols.CLIENTBOUND,new ClientboundUpdateEnabledFeaturesPacket(Set.of(Identifier.parse("minecraft:vanilla"))));
        packet("config_cb_tags",ConfigurationProtocols.CLIENTBOUND,new ClientboundUpdateTagsPacket(Map.of(Registries.BLOCK,new TagNetworkSerialization.NetworkPayload(Map.of(testId,IntList.of(1,2,3))))));
        packet("config_cb_knownpacks",ConfigurationProtocols.CLIENTBOUND,new ClientboundSelectKnownPacks(packs));
        packet("config_cb_report",ConfigurationProtocols.CLIENTBOUND,new ClientboundCustomReportDetailsPacket(Map.of("key","value")));
        packet("config_cb_links",ConfigurationProtocols.CLIENTBOUND,new ClientboundServerLinksPacket(List.of(new ServerLinks.UntrustedEntry(com.mojang.datafixers.util.Either.right(reason),"https://example.com"))));
        packet("config_cb_clear",ConfigurationProtocols.CLIENTBOUND,ClientboundClearDialogPacket.INSTANCE);
        var dialog = Dialog.DIRECT_CODEC.parse(registryOps,JsonParser.parseString("{\"type\":\"minecraft:notice\",\"title\":\"Notice\"}")).getOrThrow();
        packet("config_cb_dialog",ConfigurationProtocols.CLIENTBOUND,new ClientboundShowDialogPacket(Holder.direct(dialog)));
        packet("config_cb_conduct",ConfigurationProtocols.CLIENTBOUND,new ClientboundCodeOfConductPacket("Be kind."));
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
            Files.writeString(output.resolve("component_"+i+".hash"),Integer.toString(ComponentSerialization.CODEC.encodeStart(hashOps,component).getOrThrow().asInt())+"\n");
            if (i == 1 || i == 2) {
                vector("style_"+i,b -> Style.Serializer.TRUSTED_STREAM_CODEC.encode(new RegistryFriendlyByteBuf(b, RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)), finalComponent.getStyle()));
                Files.writeString(output.resolve("style_"+i+".hash"),Integer.toString(Style.Serializer.CODEC.encodeStart(hashOps,component.getStyle()).getOrThrow().asInt())+"\n");
            }
            vector("component_"+i, b -> ComponentSerialization.TRUSTED_STREAM_CODEC.encode(new RegistryFriendlyByteBuf(b, RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY)), finalComponent));
            Files.writeString(output.resolve("component_"+i+".json"),ComponentSerialization.CODEC.encodeStart(registryOps,component).getOrThrow().toString()+"\n");
        }
        vector("max_string", b -> b.writeUtf("\uD83D\uDE80".repeat(16383) + "x", 32767));
    }
}
