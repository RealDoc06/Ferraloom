import com.mojang.serialization.Lifecycle;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistrationInfo;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.dedicated.DedicatedServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.levelgen.DebugLevelSource;
import net.minecraft.world.level.levelgen.FlatLevelSource;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.WorldDimensions;
import net.minecraft.world.level.storage.PrimaryLevelData;
import sun.misc.Unsafe;

/** Actual native classification and ServerLevel#isFlat. These methods inspect
 * only generator classes / the selected special-world flag; no terrain is run. */
public class WorldFlagsVectors {
    static final Unsafe U;
    static { try { Field f=Unsafe.class.getDeclaredField("theUnsafe");f.setAccessible(true);U=(Unsafe)f.get(null); } catch(Exception e){throw new RuntimeException(e);} }
    static void set(Class<?> type,Object owner,String name,Object value) throws Exception {
        Field f=type.getDeclaredField(name);f.setAccessible(true);f.set(owner,value);
    }
    public static void main(String[] args) throws Exception {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        Method classification=WorldDimensions.class.getDeclaredMethod("specialWorldProperty",Registry.class);classification.setAccessible(true);
        Class<?>[] types={NoiseBasedChunkGenerator.class,FlatLevelSource.class,DebugLevelSource.class};
        String[] names={"minecraft:normal","minecraft:flat","minecraft:debug_all_block_states"};
        for(int ow=0;ow<types.length;ow++)for(int current=0;current<types.length;current++) {
            var registry=new MappedRegistry<LevelStem>(Registries.LEVEL_STEM,Lifecycle.stable());
            var dimension=(DimensionType)U.allocateInstance(DimensionType.class);
            registry.register(LevelStem.OVERWORLD,new LevelStem(Holder.direct(dimension),(ChunkGenerator)U.allocateInstance(types[ow])),RegistrationInfo.BUILT_IN);
            registry.register(LevelStem.NETHER,new LevelStem(Holder.direct(dimension),(ChunkGenerator)U.allocateInstance(types[current])),RegistrationInfo.BUILT_IN);
            var property=classification.invoke(null,registry.freeze());
            var data=(PrimaryLevelData)U.allocateInstance(PrimaryLevelData.class);
            set(PrimaryLevelData.class,data,"specialWorldProperty",property);
            var server=(DedicatedServer)U.allocateInstance(DedicatedServer.class);
            set(MinecraftServer.class,server,"worldData",data);
            var level=(ServerLevel)U.allocateInstance(ServerLevel.class);
            set(ServerLevel.class,level,"server",server);
            System.out.println("FLAGS\t"+names[ow]+"\t"+types[current].getSimpleName()+"\t"+data.isDebugWorld()+"\t"+data.isFlatWorld()+"\t"+level.isFlat());
        }
    }
}
