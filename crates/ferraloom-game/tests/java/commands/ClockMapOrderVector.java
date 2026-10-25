import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.clock.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.GameRules;
import static org.mockito.Mockito.*;

/** Observes actual createFullSyncPacket ordering, keyed by identity-hashed Holder.Reference. */
public final class ClockMapOrderVector {
    static volatile int padding;
    public static void main(String[] args) throws Exception {
        // Consume identity hashes without changing any registry or clock state.
        int identityCalls=Integer.parseInt(args[1]);
        for(int i=0;i<identityCalls;i++)padding^=System.identityHashCode(new Object());
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var lookup=VanillaRegistries.createWorldLookup();
        var clocks=mock(Registry.class);var timelines=mock(Registry.class);
        doAnswer(call->lookup.lookupOrThrow(Registries.WORLD_CLOCK).listElements()).when(clocks).listElements();
        doAnswer(call->lookup.lookupOrThrow(Registries.TIMELINE).listElements()).when(timelines).listElements();
        var access=mock(RegistryAccess.Frozen.class);
        doReturn(clocks).when(access).lookupOrThrow(Registries.WORLD_CLOCK);
        doReturn(timelines).when(access).lookupOrThrow(Registries.TIMELINE);
        var server=mock(MinecraftServer.class);var level=mock(ServerLevel.class);
        doReturn(access).when(server).registryAccess();doReturn(level).when(server).overworld();
        doReturn(new GameRules(FeatureFlags.DEFAULT_FLAGS)).when(server).getGlobalGameRules();
        var ctor=ServerClockManager.class.getDeclaredConstructor(PackedClockStates.class);ctor.setAccessible(true);
        var manager=ctor.newInstance(PackedClockStates.EMPTY);manager.init(server);
        lookup.lookupOrThrow(Registries.WORLD_CLOCK).listElements().forEach(clock->{
            boolean overworld=clock.unwrapKey().orElseThrow().identifier().getPath().equals("overworld");
            manager.getInstance(clock).loadFrom(new ClockState(overworld?1234:5678,overworld?0.25f:0.75f,overworld?2.0f:0.5f,false));
        });
        var result=new JsonArray();
        for(var entry:manager.createFullSyncPacket().clockUpdates().entrySet()) {
            var row=new JsonObject();row.addProperty("clock",entry.getKey().unwrapKey().orElseThrow().identifier().toString());
            row.addProperty("identity_hash",System.identityHashCode(entry.getKey()));
            row.addProperty("hash",entry.getKey().hashCode());
            row.addProperty("total_ticks",entry.getValue().totalTicks());row.addProperty("partial_tick",entry.getValue().partialTick());row.addProperty("rate",entry.getValue().rate());result.add(row);
        }
        var capture=new JsonObject();capture.addProperty("identity_calls",identityCalls);capture.add("entries",result);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(capture)+"\n");
    }
}
