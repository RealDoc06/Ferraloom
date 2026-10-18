import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.state.properties.NoteBlockInstrument;

/** Immutable BlockBehaviour and NoteBlockInstrument constructor facts. */
public final class ExtractNoteProperties {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var blocks=new JsonArray();var instruments=new JsonArray();
        for(var block:BuiltInRegistries.BLOCK)blocks.add(block.defaultBlockState().instrument().getSerializedName());
        for(var instrument:NoteBlockInstrument.values()){
            var entry=new JsonObject();entry.addProperty("name",instrument.getSerializedName());
            entry.addProperty("sound",BuiltInRegistries.SOUND_EVENT.getId(instrument.getSoundEvent().value()));
            entry.addProperty("tunable",instrument.isTunable());entry.addProperty("custom",instrument.hasCustomSound());entry.addProperty("above",instrument.worksAboveNoteBlock());
            instruments.add(entry);
        }
        var result=new JsonObject();result.add("blocks",blocks);result.add("instruments",instruments);
        Files.writeString(Path.of(args[0]),new Gson().toJson(result)+"\n");
    }
}
