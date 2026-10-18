import com.google.gson.*;
import java.nio.file.Path;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.placement.*;

/** Authored StructureCheck scenarios, including callback counts and cache invalidation. */
public final class StructureCheckGolden {
    static final class Probe extends Structure {
        final boolean possible;
        final AtomicInteger calls;
        Probe(Holder<Biome> biome, boolean possible, AtomicInteger calls) {super(new StructureSettings(HolderSet.direct(biome)));this.possible=possible;this.calls=calls;}
        public Optional<GenerationStub> findGenerationPoint(GenerationContext context) {calls.incrementAndGet();return possible?Optional.of(new GenerationStub(BlockPos.ZERO,b->{})):Optional.empty();}
        public StructureType<?> type() {return StructureType.JIGSAW;}
    }
    static JsonObject query(String id, boolean require, boolean reject) {var op=new JsonObject();op.addProperty("op","query");op.addProperty("id",id);op.addProperty("require",require);op.addProperty("reject",reject);return op;}
    static JsonObject increment(String id) {var op=new JsonObject();op.addProperty("op","increment");op.addProperty("id",id);return op;}
    static JsonObject load(String contents) {var op=new JsonObject();op.addProperty("op","load");op.add("counts",JsonParser.parseString(contents));return op;}
    static void capture(RegistryAccess.Frozen registries,Path directory) throws Exception {
        var plains=registries.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
        var source=new FixedBiomeSource(plains);
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        var random=RandomState.create(registries.lookupOrThrow(Registries.NOISE),99,settings);
        var generator=new net.minecraft.world.level.levelgen.FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(registries.lookupOrThrow(Registries.BIOME),registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var ops=RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE,registries);
        var allowed=StructurePlacement.CODEC.parse(ops,JsonParser.parseString("{type:'minecraft:dimension_origin'}")).getOrThrow();
        var rejected=StructurePlacement.CODEC.parse(ops,JsonParser.parseString("{type:'minecraft:random_spread',spacing:32,separation:8,salt:7,frequency:0}")).getOrThrow();
        var out=new JsonArray();
        for(String mode:List.of("known","missing","no_starts","empty","failed")) {
            var scans=new AtomicInteger();var creates=new AtomicInteger();var possible=new Probe(plains,true,creates);var impossible=new Probe(plains,false,creates);
            var tag=TagParser.parseCompoundFully(switch(mode) {
                case "known"->"{DataVersion:5023,structures:{starts:{'minecraft:village_plains':{id:'minecraft:village_plains',references:0},'minecraft:stronghold':{id:'INVALID'},'minecraft:village_taiga':{references:-1},'minecraft:buried_treasure':{},'minecraft:shipwreck':3,'bad key':{references:0},'unknown:thing':{references:0}}}}";
                case "empty"->"{DataVersion:5023,structures:{starts:{}}}";
                default->"{DataVersion:5023}";
            });
            net.minecraft.world.level.chunk.storage.ChunkScanAccess storage=(pos,visitor)-> {scans.incrementAndGet();if(mode.equals("failed"))return CompletableFuture.failedFuture(new java.io.IOException("authored scan failure"));if(!mode.equals("missing"))tag.acceptAsRoot(visitor);return CompletableFuture.completedFuture(null);};
            var check=new StructureCheck(storage,registries,null,net.minecraft.world.level.Level.OVERWORLD,generator,random,LevelHeightAccessor.create(-64,384),source,99,net.minecraft.util.datafix.DataFixers.getDataFixer());
            var actions=new ArrayList<JsonObject>();
            if(mode.equals("known")) {
                actions.add(query("minecraft:village_plains",true,false));actions.add(query("minecraft:stronghold",false,false));actions.add(query("minecraft:village_taiga",false,false));actions.add(query("minecraft:buried_treasure",false,false));actions.add(increment("minecraft:village_plains"));actions.add(query("minecraft:village_plains",true,false));actions.add(query("minecraft:village_plains",false,false));
                actions.add(load("{'minecraft:village_plains':2147483647,'minecraft:village_taiga':null}"));actions.add(increment("minecraft:village_plains"));actions.add(query("minecraft:village_plains",false,false));actions.add(query("minecraft:village_plains",true,false));
            } else {
                actions.add(query("test:possible",false,true));actions.add(query("test:possible",false,false));actions.add(query("test:possible",true,false));actions.add(query("test:impossible",false,false));actions.add(query("test:impossible",false,false));actions.add(increment("test:possible"));actions.add(query("test:possible",false,false));actions.add(query("test:possible",true,false));actions.add(load("{}"));actions.add(query("test:possible",false,false));
            }
            var item=new JsonObject();item.addProperty("mode",mode);item.addProperty("tag",tag.toString());var results=new JsonArray();var position=new ChunkPos(-7,11);
            for(var action:actions) {
                var result=action.deepCopy();String name=action.get("op").getAsString();
                if(name.equals("load")) {var starts=new HashMap<Structure,StructureStart>();for(var e:action.getAsJsonObject("counts").entrySet()) {var id=Identifier.parse(e.getKey());var structure=registries.lookupOrThrow(Registries.STRUCTURE).getValue(id);if(e.getValue().isJsonNull())starts.put(structure,StructureStart.INVALID_START);else starts.put(structure,new StructureStart(structure,position,e.getValue().getAsInt(),new PiecesContainer(List.of(new net.minecraft.world.level.levelgen.structure.structures.BuriedTreasurePieces.BuriedTreasurePiece(BlockPos.ZERO)))));}check.onStructureLoad(position,starts);}
                else {var id=Identifier.parse(action.get("id").getAsString());Structure structure=id.toString().equals("test:possible")?possible:id.toString().equals("test:impossible")?impossible:registries.lookupOrThrow(Registries.STRUCTURE).getValue(id);if(name.equals("increment"))check.incrementReference(position,structure);else result.addProperty("result",check.checkStart(position,structure,action.get("reject").getAsBoolean()?rejected:allowed,action.get("require").getAsBoolean()).name());}
                result.addProperty("scans",scans.get());result.addProperty("creates",creates.get());results.add(result);
            }
            item.add("actions",results);out.add(item);
        }
        StructureGolden.write(directory.resolve("structure_checks.json.gz"),out);
    }
}
