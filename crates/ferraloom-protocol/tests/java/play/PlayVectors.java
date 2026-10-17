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
import net.minecraft.world.item.crafting.display.*;
import net.minecraft.world.item.*;
import net.minecraft.core.registries.Registries;

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
            if (!Set.of("minecraft:worldgen/biome","minecraft:dimension_type","minecraft:damage_type","minecraft:chat_type","minecraft:dialog","minecraft:world_clock","minecraft:timeline","minecraft:waypoint_style","minecraft:trim_pattern").contains(l.key().identifier().toString())) return;
            var registry = new MappedRegistry(l.key(),Lifecycle.stable());
            l.listElements().sorted(Comparator.comparing(h -> h.key().identifier().toString())).forEach(h -> registry.register(h.key(),h.value(),RegistrationInfo.BUILT_IN));
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
        try {write.accept(b);byte[] bytes=new byte[b.readableBytes()];b.readBytes(bytes);vector(output,label,name.contains(".")?name:"net.minecraft.network.protocol.game."+name,bytes);}
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
            b.writeVarInt(-123);b.writeVarInt(456);var bits=new BitSet();bits.set(0);bits.set(65);net.minecraft.network.codec.ByteBufCodecs.BIT_SET.encode(b,bits);net.minecraft.network.codec.ByteBufCodecs.BIT_SET.encode(b,bits);net.minecraft.network.codec.ByteBufCodecs.BIT_SET.encode(b,new BitSet());net.minecraft.network.codec.ByteBufCodecs.BIT_SET.encode(b,new BitSet());
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
        var empty=SlotDisplay.Empty.INSTANCE;
        var item=new SlotDisplay.ItemSlotDisplay(Items.DIAMOND);
        var trim=registries.lookupOrThrow(Registries.TRIM_PATTERN).getOrThrow(net.minecraft.world.item.equipment.trim.TrimPatterns.SENTRY);
        var slots=List.<SlotDisplay>of(empty,SlotDisplay.AnyFuel.INSTANCE,new SlotDisplay.WithAnyPotion(item),new SlotDisplay.OnlyWithComponent(item,net.minecraft.core.component.DataComponents.DAMAGE),item,new SlotDisplay.ItemStackSlotDisplay(new ItemStackTemplate(Items.DIAMOND,64)),new SlotDisplay.TagSlotDisplay(HolderSet.direct(Items.DIAMOND.builtInRegistryHolder())),new SlotDisplay.DyedSlotDemo(item,item),new SlotDisplay.SmithingTrimDemoSlotDisplay(item,item,trim),new SlotDisplay.WithRemainder(item,empty),new SlotDisplay.Composite(List.of(item,empty)));
        var displays=List.<RecipeDisplay>of(new ShapelessCraftingRecipeDisplay(slots,item,item),new ShapedCraftingRecipeDisplay(3,3,slots.subList(0,9),item,item),new FurnaceRecipeDisplay(item,SlotDisplay.AnyFuel.INSTANCE,item,item,200,.5f),new StonecutterRecipeDisplay(item,item,item),new SmithingRecipeDisplay(item,item,item,item,item));
        for(int i=0;i<displays.size();i++){
            final var display=displays.get(i);
            sample(output,"clientbound_PlaceGhostRecipe_edge_display_"+i,"ClientboundPlaceGhostRecipePacket",b->{b.writeVarInt(300);RecipeDisplay.STREAM_CODEC.encode(b,display);});
        }
        sample(output,"clientbound_Commands_edge_all_arguments","ClientboundCommandsPacket",b->{
            int count=BuiltInRegistries.COMMAND_ARGUMENT_TYPE.size();b.writeVarInt(count+1);b.writeByte(0);b.writeVarInt(count);for(int i=1;i<=count;i++)b.writeVarInt(i);
            for(int i=0;i<count;i++){
                var name=BuiltInRegistries.COMMAND_ARGUMENT_TYPE.getKey(BuiltInRegistries.COMMAND_ARGUMENT_TYPE.byId(i)).toString();
                b.writeByte(2|4|16|32);b.writeVarInt(0);b.writeUtf("arg"+i);b.writeVarInt(i);
                switch(name){
                    case "brigadier:float":b.writeByte(3);b.writeFloat(-12.5f);b.writeFloat(42.125f);break;
                    case "brigadier:double":b.writeByte(3);b.writeDouble(-12.5);b.writeDouble(42.125);break;
                    case "brigadier:integer":b.writeByte(3);b.writeInt(-5);b.writeInt(10);break;
                    case "brigadier:long":b.writeByte(3);b.writeLong(-12345);b.writeLong(Long.MAX_VALUE);break;
                    case "brigadier:string":b.writeVarInt(2);break;
                    case "minecraft:entity":b.writeByte(3);break;
                    case "minecraft:score_holder":b.writeByte(1);break;
                    case "minecraft:time":b.writeInt(20);break;
                    case "minecraft:resource_or_tag":case "minecraft:resource_or_tag_key":case "minecraft:resource":case "minecraft:resource_key":case "minecraft:resource_selector":b.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("item"));break;
                }
                b.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("ask_server"));
            }
            b.writeVarInt(0);
        });
        sample(output,"clientbound_UpdateAdvancements_edge_full","ClientboundUpdateAdvancementsPacket",b->{
            var id=net.minecraft.resources.Identifier.withDefaultNamespace("test");b.writeBoolean(true);b.writeVarInt(1);b.writeIdentifier(id);b.writeBoolean(false);b.writeBoolean(true);
            text(b,"Title");text(b,"Description");ItemStackTemplate.STREAM_CODEC.encode(b,new ItemStackTemplate(Items.DIAMOND));b.writeVarInt(2);b.writeInt(7);b.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("gui/advancements/backgrounds/stone"));
            b.writeVarInt(2);b.writeVarInt(1);b.writeUtf("one");b.writeVarInt(2);b.writeUtf("two");b.writeUtf("three");b.writeBoolean(true);b.writeFloat(-1.5f);b.writeFloat(2.25f);
            b.writeVarInt(1);b.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("old"));b.writeVarInt(1);b.writeIdentifier(id);b.writeVarInt(1);b.writeUtf("one");b.writeBoolean(true);b.writeLong(123456789L);b.writeBoolean(true);
        });
        for(int i=1;i<=15;i++){
            final int id=i;
            sample(output,"clientbound_DebugEvent_edge_subscription_"+i,"ClientboundDebugEventPacket",b->{b.writeVarInt(id);debugValue(b,id);});
            sample(output,"clientbound_DebugEntityValue_edge_present_"+i,"ClientboundDebugEntityValuePacket",b->{b.writeVarInt(300);b.writeVarInt(id);b.writeBoolean(true);debugValue(b,id);});
            sample(output,"clientbound_DebugEntityValue_edge_absent_"+i,"ClientboundDebugEntityValuePacket",b->{b.writeVarInt(300);b.writeVarInt(id);b.writeBoolean(false);});
        }
        for(int i=0;i<=3;i++){
            final int type=i;
            sample(output,"clientbound_Waypoint_edge_type_"+i,"ClientboundTrackedWaypointPacket",b->{
                b.writeVarInt(2);b.writeBoolean(true);b.writeUUID(new UUID(123,456));b.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("default"));b.writeBoolean(true);b.writeByte(255);b.writeByte(128);b.writeByte(0);b.writeVarInt(type);
                switch(type){case 1:b.writeVarInt(-30000000);b.writeVarInt(-64);b.writeVarInt(30000000);break;case 2:b.writeVarInt(-1875000);b.writeVarInt(1875000);break;case 3:b.writeFloat(2.125f);break;}
            });
        }
        for(int stock=0;stock<2;stock++){
            var offer=new net.minecraft.world.item.trading.MerchantOffer(new net.minecraft.world.item.trading.ItemCost(Items.EMERALD,5),Optional.of(new net.minecraft.world.item.trading.ItemCost(Items.GOLD_INGOT,2)),new ItemStack(Items.DIAMOND,1),stock==0?0:4,4,2,.05f,1);
            sample(output,"clientbound_MerchantOffers_edge_stock_"+stock,"ClientboundMerchantOffersPacket",b->{b.writeVarInt(300);b.writeVarInt(1);net.minecraft.world.item.trading.MerchantOffer.STREAM_CODEC.encode(b,offer);b.writeVarInt(5);b.writeVarInt(100);b.writeBoolean(true);b.writeBoolean(true);});
        }
        sample(output,"clientbound_SetEquipment_edge_multiple","ClientboundSetEquipmentPacket",b->{b.writeVarInt(300);b.writeByte(128);ItemStack.OPTIONAL_STREAM_CODEC.encode(b,new ItemStack(Items.DIAMOND_SWORD,1));b.writeByte(5);ItemStack.OPTIONAL_STREAM_CODEC.encode(b,new ItemStack(Items.SHIELD,1));});
        for(int format=0;format<3;format++){
            final int f=format;
            sample(output,"clientbound_SetScore_edge_format_"+format,"ClientboundSetScorePacket",b->{b.writeUtf("player");b.writeUtf("objective");b.writeVarInt(Integer.MIN_VALUE);b.writeBoolean(true);text(b,"Name");b.writeBoolean(true);b.writeVarInt(f);if(f==1){net.minecraft.network.chat.Style.Serializer.TRUSTED_STREAM_CODEC.encode(b,net.minecraft.network.chat.Style.EMPTY.withBold(true));}if(f==2)text(b,"Score");});
        }
        sample(output,"clientbound_Commands_edge_literal","ClientboundCommandsPacket",b->{b.writeVarInt(2);b.writeByte(0);b.writeVarInt(1);b.writeVarInt(1);b.writeByte(5);b.writeVarInt(0);b.writeUtf("literal");b.writeVarInt(0);});
        sample(output,"clientbound_ForgetLevelChunk_edge_coordinates","ClientboundForgetLevelChunkPacket",b->{b.writeChunkPos(new net.minecraft.world.level.ChunkPos(-123,456));});
        sample(output,"clientbound_SectionBlocksUpdate_edge_changes","ClientboundSectionBlocksUpdatePacket",b->{b.writeLong(SectionPos.asLong(-2,-4,3));b.writeVarInt(2);b.writeVarLong(((long)net.minecraft.world.level.block.Block.getId(net.minecraft.world.level.block.Blocks.STONE.defaultBlockState())<<12)|4095);b.writeVarLong((long)net.minecraft.world.level.block.Block.getId(net.minecraft.world.level.block.Blocks.WATER.defaultBlockState())<<12);});
        sample(output,"clientbound_PlayerChat_edge_partial_filter","ClientboundPlayerChatPacket",b->{
            b.writeVarInt(Integer.MAX_VALUE);b.writeUUID(new UUID(123,456));b.writeVarInt(300);b.writeBoolean(true);b.writeBytes(new byte[256]);
            b.writeUtf("m".repeat(256));b.writeLong(Long.MIN_VALUE);b.writeLong(Long.MAX_VALUE);b.writeVarInt(20);for(int i=0;i<20;i++)b.writeVarInt(i+1);
            b.writeBoolean(true);text(b,"Unsigned");b.writeVarInt(2);var bits=new BitSet();bits.set(0);bits.set(9);bits.set(65);net.minecraft.network.codec.ByteBufCodecs.BIT_SET.encode(b,bits);
            b.writeVarInt(1);text(b,"Sender");b.writeBoolean(true);text(b,"Target");
        });
        // Delimiter identity, completion, and size limits are checked against vanilla's bundler.
        var protocol=net.minecraft.network.protocol.game.GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(registries));
        var in=Unpooled.wrappedBuffer(new byte[]{0});var delimiter=protocol.codec().decode(in);in.release();
        var bundler=protocol.bundlerInfo().startPacketBundling(delimiter);
        for(int i=0;i<4096;i++)if(bundler.addPacket(new net.minecraft.network.protocol.game.ClientboundTickingStepPacket(i))!=null)throw new AssertionError();
        try{bundler.addPacket(new net.minecraft.network.protocol.game.ClientboundTickingStepPacket(0));throw new AssertionError("bundle limit not enforced");}catch(IllegalStateException expected){}
        if(bundler.addPacket(delimiter)==null)throw new AssertionError("bundle did not close");
    }
    static void normalization(Path output,String label,String className,byte[] input) throws Exception {
        vector(output,label,className,input.clone());
        Files.writeString(output.resolve(label+".input.hex"),HexFormat.of().formatHex(input)+"\n");
    }
    @SuppressWarnings({"rawtypes","unchecked"})
    static void rejected(Path output,String label,byte[] input) throws Exception {
        var b=new FriendlyByteBuf(Unpooled.wrappedBuffer(input));
        try {
            try {net.minecraft.network.protocol.common.ServerboundCustomClickActionPacket.STREAM_CODEC.decode(b);throw new AssertionError(label+" should reject");}
            catch(net.minecraft.nbt.NbtAccounterException expected){}
            Files.writeString(output.resolve(label+".reject.hex"),HexFormat.of().formatHex(input)+"\n");
        }finally{b.release();}
    }
    static byte[] clickSeed(net.minecraft.nbt.Tag tag){
        var inner=new FriendlyByteBuf(Unpooled.buffer());var outer=new FriendlyByteBuf(Unpooled.buffer());
        try{inner.writeNbt(tag);outer.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("test"));byte[] nbt=new byte[inner.readableBytes()];inner.readBytes(nbt);outer.writeByteArray(nbt);byte[] input=new byte[outer.readableBytes()];outer.readBytes(input);return input;}
        finally{inner.release();outer.release();}
    }
    @SuppressWarnings({"rawtypes","unchecked"})
    static void creativeRejected(Path output,String label,int count,Integer maxStackSize) throws Exception {
        var b=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);
        var stack=new ItemStack(Items.DIAMOND,count);
        if(maxStackSize!=null)stack.set(net.minecraft.core.component.DataComponents.MAX_STACK_SIZE,maxStackSize);
        b.writeShort(-1);ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC.encode(b,stack);byte[] input=new byte[b.readableBytes()];b.readBytes(input);b.readerIndex(0);
        try {
            try {net.minecraft.network.protocol.game.ServerboundSetCreativeModeSlotPacket.STREAM_CODEC.decode(b);throw new AssertionError(label+" should reject");}
            catch(io.netty.handler.codec.DecoderException expected){}
            Files.writeString(output.resolve(label+".reject.hex"),HexFormat.of().formatHex(input)+"\n");
        }finally{b.release();}
    }
    static void normalizationCases(Path output) throws Exception {
        normalization(output,"serverbound_CustomClickAction_norm_trailing","net.minecraft.network.protocol.common.ServerboundCustomClickActionPacket",new byte[]{0,4,0,1,2,3});
        var b=new FriendlyByteBuf(Unpooled.buffer());
        b.writeUtf("en_us");b.writeByte(-1);b.writeVarInt(-1);b.writeBoolean(true);b.writeByte(255);b.writeVarInt(99);b.writeBoolean(true);b.writeBoolean(false);b.writeVarInt(-5);
        byte[] input=new byte[b.readableBytes()];b.readBytes(input);b.release();
        normalization(output,"serverbound_ClientInformation_norm_wrapping","net.minecraft.network.protocol.common.ServerboundClientInformationPacket",input);
        b=new FriendlyByteBuf(Unpooled.buffer());b.writeVarInt(1);b.writeBoolean(true);b.writeVarInt(99);b.writeUtf("https://example.com");input=new byte[b.readableBytes()];b.readBytes(input);b.release();
        normalization(output,"clientbound_ServerLinks_norm_zero","net.minecraft.network.protocol.common.ClientboundServerLinksPacket",input);
        b=new FriendlyByteBuf(Unpooled.buffer());b.writeIdentifier(net.minecraft.resources.Identifier.withDefaultNamespace("unknown"));b.writeBytes(new byte[]{1,2,3,4});input=new byte[b.readableBytes()];b.readBytes(input);b.release();
        normalization(output,"serverbound_CustomPayload_norm_discarded","net.minecraft.network.protocol.common.ServerboundCustomPayloadPacket",input);
        normalization(output,"clientbound_CustomPayload_norm_discarded","net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket",input);
        rejected(output,"serverbound_CustomClickAction_quota",clickSeed(net.minecraft.nbt.StringTag.valueOf("q".repeat(17000))));
        var tag=new net.minecraft.nbt.CompoundTag();var current=tag;
        for(int i=0;i<17;i++){var next=new net.minecraft.nbt.CompoundTag();current.put("nested",next);current=next;}
        rejected(output,"serverbound_CustomClickAction_depth",clickSeed(tag));
        sample(output,"clientbound_ResourcePackPush_edge_prompt","net.minecraft.network.protocol.common.ClientboundResourcePackPushPacket",buf->{buf.writeUUID(new UUID(123,456));buf.writeUtf("https://example.com/pack");buf.writeUtf("f".repeat(40));buf.writeBoolean(true);buf.writeBoolean(true);text(buf,"Download?");});
    }
    static void node(RegistryFriendlyByteBuf b){b.writeInt(-1);b.writeInt(64);b.writeInt(2);b.writeFloat(1.5f);b.writeFloat(-1);b.writeBoolean(true);b.writeVarInt(26);b.writeFloat(.75f);}
    static void debugValue(RegistryFriendlyByteBuf b,int id){
        var pos=new BlockPos(-1,64,2);
        switch(id){
            case 1:b.writeBoolean(true);b.writeBlockPos(pos);b.writeBoolean(false);b.writeVarInt(300);b.writeVarInt(1);b.writeBlockPos(pos);break;
            case 2:b.writeUtf("Name");b.writeUtf("Profession");b.writeInt(10);b.writeFloat(1.5f);b.writeFloat(20);b.writeUtf("inventory");b.writeBoolean(true);b.writeInt(7);for(int i=0;i<4;i++){b.writeVarInt(1);b.writeUtf("entry");}for(int i=0;i<2;i++){b.writeVarInt(1);b.writeBlockPos(pos);}break;
            case 3:b.writeBoolean(true);b.writeVarInt(123);b.writeBoolean(true);b.writeBlockPos(pos);break;
            case 4:b.writeVarInt(1);b.writeVarInt(10);b.writeBoolean(true);b.writeUtf("g".repeat(255));break;
            case 5:b.writeBoolean(true);b.writeInt(0);b.writeBlockPos(pos);b.writeVarInt(1);node(b);b.writeVarInt(0);b.writeVarInt(0);b.writeVarInt(1);node(b);b.writeFloat(1.25f);break;
            case 6:b.writeVarInt(2);break;
            case 7:b.writeVarInt(0);b.writeVarInt(3);b.writeVarInt(5);b.writeBoolean(true);break;
            case 8:b.writeBlockPos(pos);b.writeVarInt(0);b.writeVarInt(2);break;
            case 9:b.writeVarInt(47);break;
            case 10:break;
            case 11:b.writeVarInt(1);b.writeBlockPos(pos);break;
            case 12:b.writeVarInt(1);b.writeBlockPos(pos);b.writeBlockPos(pos.offset(5,6,7));b.writeVarInt(1);b.writeBlockPos(pos);b.writeBlockPos(pos.offset(2,3,4));b.writeBoolean(true);break;
            case 13:b.writeVarInt(32);break;
            case 14:b.writeBlockPos(pos);break;
            case 15:b.writeVarInt(0);b.writeDouble(-1.5);b.writeDouble(64);b.writeDouble(2.125);break;
        }
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
                b.writeByteArray(HexFormat.of().parseHex(Files.readString(output.resolve("../../../tests/fixtures/rsa_public.hex").normalize()).trim()));b.writeByteArray(new byte[0]);
                seed=new byte[b.readableBytes()];b.readBytes(seed);b.release();
            }
            try {vector(output,cols[0],cols[1],seed);count++;}
            catch(Exception e){throw new IllegalStateException(cols[0],e);}
        }
        edgeCases(output);
        normalizationCases(output);
        sample(output,"serverbound_SetCreativeModeSlot_edge_count_99","ServerboundSetCreativeModeSlotPacket",b->{b.writeShort(-1);ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC.encode(b,new ItemStack(Items.DIAMOND,99));});
        creativeRejected(output,"serverbound_SetCreativeModeSlot_count_100",100,null);
        creativeRejected(output,"serverbound_SetCreativeModeSlot_stack_limit_0",1,0);
        Files.write(output.resolve("packets.tsv"),packetRows);
        System.out.println("Encoded "+count+" packet samples plus bundle delimiter");
    }
}
