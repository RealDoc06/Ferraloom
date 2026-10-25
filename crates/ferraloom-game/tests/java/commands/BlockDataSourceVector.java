import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.EntityAnchorArgument;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.*;
import net.minecraft.network.chat.contents.data.BlockDataSource;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.TheEndPortalBlockEntity;
import net.minecraft.world.phys.*;
import org.apache.commons.lang3.mutable.MutableInt;
import static org.mockito.Mockito.*;

/** Executes BlockDataSource's loaded gate and real full-metadata BE save. */
public final class BlockDataSourceVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var registries=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var rows=new JsonArray();
        for(int coordinatesCase=0;coordinatesCase<4;coordinatesCase++)for(String state:List.of("present","absent","unloaded")) {
            String coordinates=switch(coordinatesCase){case 0->"17 -4 -1";case 1->"~1.5 ~-2.25 ~3.5";default->"^1 ^2 ^3";};
            boolean eyes=coordinatesCase==3;
            var level=mock(ServerLevel.class);var source=mock(CommandSourceStack.class);var entity=mock(Entity.class);
            doReturn(level).when(source).getLevel();doReturn(registries).when(source).registryAccess();
            doReturn(new Vec3(16.25,7.75,-8.5)).when(source).getPosition();
            doReturn(new Vec2(-19.25f,37.25f)).when(source).getRotation();
            doReturn(entity).when(source).getEntity();doReturn(1.25f).when(entity).getEyeHeight();
            doReturn(eyes?EntityAnchorArgument.Anchor.EYES:EntityAnchorArgument.Anchor.FEET).when(source).getAnchor();
            var compiled=BlockDataSource.BLOCK_POS_CODEC.parse(JsonOps.INSTANCE,new JsonPrimitive(coordinates)).getOrThrow();
            var pos=compiled.compiled().getBlockPos(source);
            doReturn(!state.equals("unloaded")).when(level).isLoaded(pos);
            var receiver=new TheEndPortalBlockEntity(pos,Blocks.END_PORTAL.defaultBlockState());
            receiver.setComponents(DataComponentMap.builder().set(DataComponents.CUSTOM_NAME,Component.literal("Stored portal")).set(DataComponents.MAX_STACK_SIZE,7).build());
            if(state.equals("present"))doReturn(receiver).when(level).getBlockEntity(pos);
            if(state.equals("unloaded"))doThrow(new AssertionError("unloaded block entity must not be queried")).when(level).getBlockEntity(any(BlockPos.class));
            var data=new BlockDataSource(compiled).getData(source).toList();
            String componentJson="{\"nbt\":\"id\",\"block\":"+new Gson().toJson(coordinates)+"}";
            var component=ComponentSerialization.CODEC.parse(JsonOps.INSTANCE,JsonParser.parseString(componentJson)).getOrThrow();
            var resolved=ComponentUtils.resolve(new ResolutionContext(source,entity,o->true,100,ResolutionContext.LimitBehavior.STOP_PROCESSING_AND_COPY_REMAINING,65536,new MutableInt()),component,0);
            var row=new JsonObject();row.addProperty("coordinates",coordinates);row.addProperty("eyes",eyes);row.addProperty("state",state);
            row.add("position",new Gson().toJsonTree(List.of(pos.getX(),pos.getY(),pos.getZ())));
            row.add("data",new Gson().toJsonTree(data.stream().map(Object::toString).toList()));
            row.add("component",JsonParser.parseString(componentJson));
            row.add("resolved",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,resolved).getOrThrow());
            rows.add(row);
            verify(level,times(state.equals("unloaded")?0:2)).getBlockEntity(pos);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows)+"\n");
    }
}
