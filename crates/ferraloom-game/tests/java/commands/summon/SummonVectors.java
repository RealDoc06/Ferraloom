import com.google.gson.*;
import com.mojang.brigadier.*;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.arguments.EntityAnchorArgument;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.SummonCommand;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.util.RandomSource;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Native command parsing, call order and feedback; controlled loader outcomes
 * isolate SummonCommand from factory/finalization mechanics tested by entity. */
public final class SummonVectors {
    record Case(String command, boolean peaceful, boolean load, boolean duplicate, int passengers, int permission) {
        Case(String command) {this(command,false,true,false,0,4);}
    }
    static final Gson JSON=new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static final Vec3 SOURCE=new Vec3(80.25,64.5,-16.75);
    static JsonElement component(Component component) {
        return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,component).getOrThrow();
    }
    static JsonArray position(Vec3 pos) { return JSON.toJsonTree(List.of(pos.x,pos.y,pos.z)).getAsJsonArray(); }
    static CommandSourceStack source(ServerLevel level, boolean eyes, int permission) {
        var source=mock(CommandSourceStack.class);
        doReturn(level).when(source).getLevel(); doReturn(SOURCE).when(source).getPosition();
        doReturn(new Vec2(-20f,35f)).when(source).getRotation();
        doReturn(eyes?EntityAnchorArgument.Anchor.EYES:EntityAnchorArgument.Anchor.FEET).when(source).getAnchor();
        doReturn(LevelBasedPermissionSet.forLevel(net.minecraft.server.permissions.PermissionLevel.byId(permission))).when(source).permissions();
        doReturn(CommandResultCallback.EMPTY).when(source).callback();
        return source;
    }
    static List<Case> cases() {
        return List.of(
            new Case("summon minecraft:experience_orb"),new Case("summon minecraft:cow"),
            new Case("summon minecraft:experience_orb 8 -60 8"),
            new Case("summon minecraft:cow 8.5 -60 8.5 {}"),
            new Case("summon minecraft:cow 8.5 -60 8.5 {Age:-24000}"),
            new Case("summon minecraft:item 8.5 -60 8.5 {Item:{id:'minecraft:stone',count:3}}"),
            new Case("summon minecraft:tnt ~ ~1 ~-2 {fuse:120}"),
            new Case("summon minecraft:falling_block ^1 ^2 ^3 {BlockState:{Name:'minecraft:sand'}}"),
            new Case("summon minecraft:experience_orb 8.5 -60 8.5 {Value:7s,Count:3}"),
            new Case("summon minecraft:cow 8.5 -60 8.5 {id:'minecraft:tnt',Passengers:[{id:'minecraft:item'},{id:'minecraft:experience_orb'}]}",false,true,false,2,4),
            new Case("summon minecraft:cow 8.5 -60 8.5",false,true,true,0,4),
            new Case("summon minecraft:experience_orb 8.5 -60 8.5 {Passengers:[{id:'minecraft:item'}]}",false,true,true,1,4),
            new Case("summon minecraft:cow 8.5 -60 8.5",false,false,false,0,4),
            new Case("summon minecraft:experience_orb 8.5 -60 8.5 {}",false,false,false,0,4),
            new Case("summon minecraft:zombie 8.5 -60 8.5",true,true,false,0,4),
            new Case("summon minecraft:cow 8.5 -60 8.5",true,true,false,0,4),
            new Case("summon minecraft:zombie 30000000 64 0",true,true,false,0,4),
            new Case("summon minecraft:player 30000000 64 0"),
            new Case("summon minecraft:fishing_bobber"),
            new Case("summon minecraft:no_such_entity"),
            new Case("summon minecraft:cow 30000000 64 0"),
            new Case("summon minecraft:cow -30000001 64 0"),
            new Case("summon minecraft:cow 0 20000000 0"),
            new Case("summon minecraft:cow 0 -20000000.001 0"),
            new Case("summon minecraft:cow -30000000 -20000000 29999999"),
            new Case("summon minecraft:cow 29999999.99 19999999.99 -30000000"),
            new Case("summon minecraft:cow 8.5 -60 8.5 []"),
            new Case("summon minecraft:cow 8.5 -60"),
            new Case("summon minecraft:cow",false,true,false,0,1)
        );
    }
    static JsonArray capture(CommandBuildContext context) throws Exception {
        var vectors=new JsonArray();
        var spawn=Arrays.stream(SummonCommand.class.getDeclaredMethods()).filter(m->m.getName().equals("spawnEntity")).findFirst().orElseThrow();
        var display=Component.literal("Loaded ").append(Component.translatable("entity.minecraft.cow")).withStyle(net.minecraft.ChatFormatting.YELLOW);
        for(var test:cases()) {
            var row=JSON.toJsonTree(test).getAsJsonObject();row.add("display",component(display));var trace=new JsonArray();var feedback=new JsonArray();
            var level=mock(ServerLevel.class);var difficulty=new DifficultyInstance(Difficulty.NORMAL,0,0,0f);
            boolean eyes=test.command.contains("^");row.addProperty("eyes",eyes);
            var source=source(level,eyes,test.permission);var actor=mock(Entity.class);doReturn(actor).when(source).getEntity();
            doAnswer(i->{trace.add("eye_height");return 1.25f;}).when(actor).getEyeHeight();
            doAnswer(i->{trace.add("difficulty");return test.peaceful?Difficulty.PEACEFUL:Difficulty.NORMAL;}).when(level).getDifficulty();
            doAnswer(i->{var call=new JsonObject();call.addProperty("call","local_difficulty");var p=(net.minecraft.core.BlockPos)i.getArgument(0);call.add("pos",JSON.toJsonTree(List.of(p.getX(),p.getY(),p.getZ())));trace.add(call);return difficulty;}).when(level).getCurrentDifficultyAt(any());
            var dispatcher=new CommandDispatcher<CommandSourceStack>();SummonCommand.register(dispatcher,context);
            var parsed=dispatcher.parse(test.command,source);
            var raw=parsed.getContext().getArguments().get("nbt");
            CompoundTag original=raw==null?null:(CompoundTag)raw.getResult();
            String name=test.command.split(" ")[1];if(!name.contains(":"))name="minecraft:"+name;
            var type=BuiltInRegistries.ENTITY_TYPE.getValue(net.minecraft.resources.Identifier.parse(name));
            boolean mob=type==EntityTypes.COW;row.addProperty("mob",mob);row.addProperty("allowed",type!=null&&type.isAllowedInPeaceful());
            Entity root=mob?mock(Mob.class):mock(Entity.class);
            doReturn(display).when(root).getDisplayName();
            doAnswer(i->{trace.add("display_name");return display;}).when(root).getDisplayName();
            var spawned=new ArrayList<Entity>();spawned.add(root);
            for(int n=0;n<test.passengers;n++)spawned.add(mock(Entity.class));
            for(int n=0;n<spawned.size();n++) {
                final int index=n;var entity=spawned.get(n);doReturn(10.5f+n*10).when(entity).getYRot();doReturn(-25.25f+n*10).when(entity).getXRot();
                doAnswer(i->{var call=new JsonObject();call.addProperty("call","snap");call.addProperty("index",index);var pos=new Vec3(i.getArgument(0),i.getArgument(1),i.getArgument(2));call.add("pos",position(pos));call.addProperty("yaw",(float)i.getArgument(3));call.addProperty("pitch",(float)i.getArgument(4));trace.add(call);doReturn(net.minecraft.core.BlockPos.containing(pos)).when(root).blockPosition();return null;}).when(entity).snapTo(anyDouble(),anyDouble(),anyDouble(),anyFloat(),anyFloat());
            }
            if(mob)doAnswer(i->{trace.add("finalize");return null;}).when((Mob)root).finalizeSpawn(eq(level),eq(difficulty),eq(EntitySpawnReason.COMMAND),isNull());
            doAnswer(i->{trace.add("register");return !test.duplicate;}).when(level).tryAddFreshEntityWithPassengers(root);
            doAnswer(i->{var message=((Supplier<Component>)i.getArgument(0)).get();trace.add("success");var entry=new JsonObject();entry.addProperty("broadcast",(boolean)i.getArgument(1));entry.add("message",component(message));feedback.add(entry);return null;}).when(source).sendSuccess(any(),anyBoolean());
            try(var entityType=mockStatic(EntityType.class,CALLS_REAL_METHODS)) {
                entityType.when(()->EntityType.loadEntityRecursive(any(CompoundTag.class),eq(level),any(EntitySpawnRequest.class),any(EntityProcessor.class))).thenAnswer(i->{
                    var tag=(CompoundTag)i.getArgument(0);var request=(EntitySpawnRequest)i.getArgument(2);var call=new JsonObject();call.addProperty("call","load");call.addProperty("tag",tag.toString());call.addProperty("reason",request.reason().name());call.addProperty("ignore_checks",request.ignoreChecks());trace.add(call);
                    if(!test.load)return null;
                    var post=(EntityProcessor)i.getArgument(3);for(var entity:spawned)post.process(entity);
                    return root;
                });
                try {row.addProperty("value",dispatcher.execute(parsed));}
                catch(CommandSyntaxException error) {row.addProperty("error",error.getRawMessage().getString());row.addProperty("cursor",error.getCursor());row.add("error_component",component(error.getRawMessage() instanceof Component c?c:Component.literal(error.getRawMessage().getString())));}
            }
            row.add("trace",trace);row.add("feedback",feedback);
            if(original!=null)row.addProperty("original_after",original.toString());
            vectors.add(row);
        }
        return vectors;
    }
    static JsonArray cowPairingProof = new JsonArray();
    static String packetWire(net.minecraft.network.protocol.Packet<?> packet, RegistryAccess access) {
        var buffer = new net.minecraft.network.RegistryFriendlyByteBuf(io.netty.buffer.Unpooled.buffer(),access);
        if (packet instanceof net.minecraft.network.protocol.game.ClientboundAddEntityPacket p)
            net.minecraft.network.protocol.game.ClientboundAddEntityPacket.STREAM_CODEC.encode(buffer,p);
        else if (packet instanceof net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket p)
            net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket.STREAM_CODEC.encode(buffer,p);
        else if (packet instanceof net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket p)
            net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket.STREAM_CODEC.encode(buffer,p);
        else throw new AssertionError(packet);
        var bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);buffer.release();return HexFormat.of().formatHex(bytes);
    }
    static JsonArray uuidProof(HolderLookup.Provider lookup,CommandBuildContext context) throws Exception {
        var access=new RegistryAccess.ImmutableRegistryAccess(lookup.listRegistries().map(r->(Registry<?>)r).toList());
        var level=mock(ServerLevel.class);doReturn(access).when(level).registryAccess();
        var server=mock(net.minecraft.server.MinecraftServer.class);
        doReturn(server).when(level).getServer();
        doReturn(new net.minecraft.util.debug.ServerDebugSubscribers(server)).when(server).debugSubscribers();
        doReturn(new net.minecraft.server.ServerScoreboard(server)).when(level).getScoreboard();
        doReturn(Difficulty.NORMAL).when(level).getDifficulty();doReturn(FeatureFlags.DEFAULT_FLAGS).when(level).enabledFeatures();
        doReturn(Level.OVERWORLD).when(level).dimension();doReturn(level).when(level).getLevel();
        doReturn(Blocks.AIR.defaultBlockState()).when(level).getBlockState(any());
        doReturn(RandomSource.create(42)).when(level).getRandom();
        doReturn(lookup.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS)).when(level).getBiome(any());
        doReturn(new DifficultyInstance(Difficulty.NORMAL,0,0,0f)).when(level).getCurrentDifficultyAt(any());
        var source=source(level,false,4);var dispatcher=new CommandDispatcher<CommandSourceStack>();SummonCommand.register(dispatcher,context);
        var proof=new JsonArray();
        final Entity[] current={null};
        doAnswer(i->{current[0]=i.getArgument(0);return true;}).when(level).tryAddFreshEntityWithPassengers(any());
        doAnswer(i->{((Supplier<Component>)i.getArgument(0)).get();return null;}).when(source).sendSuccess(any(),anyBoolean());
        for(String type:List.of("experience_orb","item","tnt","falling_block","cow")) {
            var command="summon minecraft:"+type+" 8.5 -60 8.5";
            var row=new JsonObject();row.addProperty("command",command);row.addProperty("value",dispatcher.execute(command,source));
            row.addProperty("uuid",current[0].getUUID().toString());row.addProperty("removed",current[0].isRemoved());row.add("display_name",component(current[0].getDisplayName()));
            proof.add(row);
        }
        // As in a live Level, this second capture uses a process-seeded stream.
        // Repetition lets the rare native left-handed branch appear in the proof.
        doReturn(RandomSource.create()).when(level).getRandom();
        var soundGetter=net.minecraft.world.entity.animal.cow.Cow.class.getDeclaredMethod("getSoundVariant");soundGetter.setAccessible(true);
        for(int index=0;index<128;index++) {
            dispatcher.execute("summon minecraft:cow 8.5 -60 8.5",source);
            var cow=(net.minecraft.world.entity.animal.cow.Cow)current[0];
            cow.setId(1);cow.setUUID(new java.util.UUID(0,1));
            var row=new JsonObject();row.addProperty("left_handed",cow.isLeftHanded());
            row.addProperty("sound_variant",((Holder<?>)soundGetter.invoke(cow)).getRegisteredName());
            var followRange=cow.getAttribute(net.minecraft.world.entity.ai.attributes.Attributes.FOLLOW_RANGE);
            row.addProperty("follow_range_amount",followRange.getModifier(net.minecraft.resources.Identifier.withDefaultNamespace("random_spawn_bonus")).amount());
            row.addProperty("follow_range_syncable",followRange.getAttribute().value().isClientSyncable());
            var packets=new JsonArray();
            var tracker=new net.minecraft.server.level.ServerEntity(level,cow,UpdateInterval.periodic(3),true,mock(net.minecraft.server.level.ServerEntity.Synchronizer.class));
            tracker.sendPairingData(mock(net.minecraft.server.level.ServerPlayer.class),packet->{
                var entry=new JsonObject();entry.addProperty("kind",packet.getClass().getSimpleName());entry.addProperty("wire",packetWire(packet,access));
                if(packet instanceof net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket data) {
                    var stable=data.packedItems().stream().filter(v->v.id()!=15&&v.id()!=19).toList();
                    entry.addProperty("stable_wire",packetWire(new net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket(1,stable),access));
                }
                packets.add(entry);
            });
            row.add("packets",packets);cowPairingProof.add(row);
        }
        return proof;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var lookup=LootVectors.load();BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        var context=CommandBuildContext.simple(lookup,FeatureFlags.DEFAULT_FLAGS);
        var output=new JsonObject();output.add("vectors",capture(context));output.add("uuid_proof",uuidProof(lookup,context));output.add("cow_pairing_proof",cowPairingProof);
        Files.writeString(Path.of(args[0]),JSON.toJson(output)+"\n");System.out.println("Captured summon vectors and native implicit UUID proof");
    }
}
