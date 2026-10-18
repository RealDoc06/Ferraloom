import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicReference;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.util.RandomSource;
import net.minecraft.util.context.*;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.storage.loot.*;
import net.minecraft.world.level.storage.loot.parameters.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real jar tables and codecs; mocks reject any unspecified live fact by name. */
public final class LootVectors {
    static final Gson JSON=new GsonBuilder().disableHtmlEscaping().create();
    static final long[] SEEDS={0,1,42,-1};
    static final AtomicReference<RandomSource> RANDOM=new AtomicReference<>();
    static HolderLookup.Provider lookup;
    static ServerLevel level;
    static <T> T missing(Class<T> cls) {
        return mock(cls, invocation -> {throw new UnsupportedOperationException(cls.getSimpleName()+"."+invocation.getMethod().getName());});
    }
    static HolderLookup.Provider load() {
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        var manager=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(manager,builtins).forEach(Registry.PendingTags::apply);
        var base=builtins.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value()).toList();
        var world=RegistryDataLoader.load(manager,base,RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
        TagLoader.loadTagsForExistingRegistries(manager,world).forEach(Registry.PendingTags::apply);
        var context=java.util.stream.Stream.concat(base.stream(),world.listRegistries()).toList();
        var reload=RegistryDataLoader.load(manager,context,RegistryDataLoader.RELOADABLE_REGISTRIES,Runnable::run).join();
        return HolderLookup.Provider.create(java.util.stream.Stream.concat(context.stream(),reload.listRegistries()));
    }
    static JsonElement stack(ItemStack stack) {
        return stack.isEmpty()?JsonNull.INSTANCE:ItemStack.CODEC.encodeStart(lookup.createSerializationContext(JsonOps.INSTANCE),stack).getOrThrow();
    }
    static JsonArray stacks(List<ItemStack> stacks) {var a=new JsonArray();for(var stack:stacks)a.add(stack(stack));return a;}
    static LootParams params(Identifier id,LootTable table) {
        var values=ContextMap.builder();
        var block=Blocks.STONE;
        if(id.getPath().startsWith("blocks/")) block=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(id.getPath().substring(7)));
        var entity=missing(Entity.class);
        doReturn(EntityTypes.PIG).when(entity).getType();
        doReturn(new Vec3(0.5,64,0.5)).when(entity).position();
        doReturn(0.5).when(entity).getX();doReturn(64.0).when(entity).getY();doReturn(0.5).when(entity).getZ();
        doReturn(false).when(entity).isOnFire();doReturn(false).when(entity).isCrouching();
        doReturn(false).when(entity).isSprinting();doReturn(false).when(entity).isSwimming();
        doReturn(false).when(entity).isInWater();doReturn(true).when(entity).onGround();
        for(var key:table.getParamSet().allowed()) {
            if(key==LootContextParams.ORIGIN) values.set(LootContextParams.ORIGIN,new Vec3(0.5,64,0.5));
            else if(key==LootContextParams.TOOL) values.set(LootContextParams.TOOL,new ItemStack(Items.DIAMOND_PICKAXE));
            else if(key==LootContextParams.BLOCK_STATE) values.set(LootContextParams.BLOCK_STATE,block.defaultBlockState());
            else if(key==LootContextParams.THIS_ENTITY) values.set(LootContextParams.THIS_ENTITY,entity);
            else if(table.getParamSet().required().contains(key)) throw new UnsupportedOperationException("required_context."+key.name());
        }
        return new LootParams(level,values.buildAndValidate(table.getParamSet()),Map.of(),0);
    }
    static JsonObject evaluateTable(String definition,long seed) {
        var json=JsonParser.parseString(definition);
        var table=LootTable.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
        var params=new LootParams(level,ContextMap.EMPTY,Map.of(),0.75f);
        var random=RandomSource.create(seed);RANDOM.set(random);
        var result=new JsonObject();result.add("definition",json);result.addProperty("seed",seed);
        result.add("items",stacks(table.getRandomItems(params,random)));result.addProperty("next",random.nextLong());
        random=RandomSource.create(seed);RANDOM.set(random);var container=new SimpleContainer(9);container.setItem(3,new ItemStack(Items.APPLE));
        table.fill(container,params,0);var slots=new JsonArray();for(int slot=0;slot<9;slot++)slots.add(stack(container.getItem(slot)));
        result.add("slots",slots);result.addProperty("fill_next",random.nextLong());return result;
    }
    static void focused(JsonObject output) {
        var focused=new JsonArray();
        var definitions=List.of(
            "{\"type\":\"empty\",\"pools\":[{\"rolls\":1,\"entries\":[{\"type\":\"item\",\"name\":\"stone\",\"modifier\":{\"type\":\"set_count\",\"count\":130}}]}]}",
            "{\"pools\":[{\"rolls\":{\"type\":\"uniform\",\"min\":2,\"max\":6},\"bonus_rolls\":1.5,\"modifier\":{\"type\":\"set_count\",\"count\":{\"type\":\"uniform\",\"min\":1,\"max\":6}},\"entries\":[{\"type\":\"item\",\"name\":\"stone\",\"weight\":2,\"quality\":1},{\"type\":\"empty\",\"weight\":1},{\"type\":\"item\",\"name\":\"apple\",\"quality\":-1}]}]}",
            "{\"pools\":[{\"rolls\":4,\"entries\":[{\"type\":\"alternatives\",\"modifier\":{\"type\":\"set_count\",\"count\":{\"type\":\"uniform\",\"min\":1,\"max\":5}},\"children\":[{\"type\":\"group\",\"condition\":{\"type\":\"random_chance\",\"chance\":0.4},\"children\":[{\"type\":\"item\",\"name\":\"stick\"},{\"type\":\"item\",\"name\":\"apple\"}]},{\"type\":\"sequence\",\"children\":[{\"type\":\"item\",\"name\":\"stone\"},{\"type\":\"empty\",\"condition\":{\"type\":\"random_chance\",\"chance\":0.5}},{\"type\":\"item\",\"name\":\"dirt\"}]}]}]}]}",
            "{\"pools\":[{\"rolls\":1,\"entries\":[{\"type\":\"loot_table\",\"value\":{\"pools\":[{\"rolls\":2,\"entries\":[{\"type\":\"item\",\"name\":\"diamond_pickaxe\",\"modifier\":[{\"type\":\"set_damage\",\"damage\":{\"type\":\"uniform\",\"min\":0.1,\"max\":0.9}},{\"type\":\"set_custom_data\",\"tag\":\"{test:[1,2],nested:{a:4}}\"}]}]}]},\"modifier\":{\"type\":\"filtered\",\"item_filter\":{\"items\":\"diamond_pickaxe\"},\"on_pass\":{\"type\":\"set_item\",\"item\":\"iron_pickaxe\"}}}]}]}",
            "{\"pools\":[{\"rolls\":1,\"entries\":[{\"type\":\"item\",\"name\":\"stone\",\"modifier\":[{\"type\":\"set_count\",\"count\":4},{\"type\":\"set_components\",\"components\":{\"max_stack_size\":1}},{\"type\":\"set_count\",\"count\":-1},{\"type\":\"set_count\",\"count\":7}]}]}]}"
        );
        for(var definition:definitions)for(long seed:SEEDS)focused.add(evaluateTable(definition,seed));
        output.add("synthetic_tables",focused);
        var predicates=new JsonArray();
        var items=List.of("{\"id\":\"stone\",\"count\":5}","{\"id\":\"diamond_pickaxe\",\"components\":{\"damage\":17}}", "{\"id\":\"oak_log\"}","{\"id\":\"stone\",\"components\":{\"custom_data\":\"{list:[1,2,3],name:'Doc'}\"}}", "{\"id\":\"diamond_sword\",\"components\":{\"enchantments\":{\"sharpness\":3}}}");
        var tests=List.of("{}","{\"items\":\"stone\",\"count\":{\"min\":2,\"max\":8}}", "{\"items\":\"#minecraft:logs\"}","{\"predicates\":{\"damage\":{\"damage\":{\"min\":10}}}}","{\"predicates\":{\"custom_data\":\"{list:[2]}\"}}", "{\"predicates\":{\"enchantments\":[{\"enchantments\":\"sharpness\",\"levels\":3}]}}");
        for(var item:items)for(var test:tests) {
            var itemJson=JsonParser.parseString(item);var testJson=JsonParser.parseString(test);
            var stack=ItemStack.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),itemJson).getOrThrow();
            var predicate=net.minecraft.advancements.predicates.ItemPredicate.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),testJson).getOrThrow();
            var record=new JsonObject();record.add("item",itemJson);record.add("predicate",testJson);record.addProperty("matches",predicate.test(stack));predicates.add(record);
        }
        output.add("item_predicates",predicates);
        var modifiers=new JsonArray();
        var functionDefinitions=List.of(
            "{\"type\":\"copy_components\",\"source\":\"tool\",\"include\":[\"damage\"]}",
            "{\"type\":\"set_enchantments\",\"enchantments\":{\"sharpness\":3}}",
            "{\"type\":\"set_stew_effect\",\"effects\":[{\"type\":\"poison\",\"duration\":{\"type\":\"uniform\",\"min\":2,\"max\":5}},{\"type\":\"instant_health\",\"duration\":3}]}",
            "{\"type\":\"set_name\",\"name\":\"Doc\",\"target\":\"item_name\"}",
            "{\"type\":\"set_random_potion\",\"options\":[\"water\",\"healing\"]}",
            "{\"type\":\"set_banner_pattern\",\"patterns\":[{\"pattern\":\"stripe_bottom\",\"color\":\"white\"}],\"append\":true}",
            "{\"type\":\"toggle_tooltips\",\"toggles\":{\"enchantments\":false,\"attribute_modifiers\":true}}",
            "{\"type\":\"set_loot_table\",\"loot_table_id\":\"archaeology/desert_well\",\"seed\":42}",
            "{\"type\":\"copy_state\",\"block\":\"wheat\",\"properties\":[\"age\",\"invalid\"]}",
            "{\"type\":\"apply_bonus\",\"enchantment\":\"fortune\",\"formula\":\"ore_drops\"}",
            "{\"type\":\"apply_bonus\",\"enchantment\":\"fortune\",\"formula\":\"uniform_bonus_count\",\"parameters\":{\"bonusMultiplier\":2}}",
            "{\"type\":\"explosion_decay\"}"
        );
        for(var definition:functionDefinitions)for(String input:List.of("{\"id\":\"book\",\"count\":2}","{\"id\":\"suspicious_stew\"}","{\"id\":\"stone\",\"count\":16}"))for(long seed:SEEDS){
            var functionJson=JsonParser.parseString(definition);var inputJson=JsonParser.parseString(input);
            var function=net.minecraft.world.level.storage.loot.functions.LootItemFunctions.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),functionJson).getOrThrow();
            var item=ItemStack.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),inputJson).getOrThrow();
            var toolJson=JsonParser.parseString("{\"id\":\"diamond_pickaxe\",\"components\":{\"damage\":17,\"enchantments\":{\"fortune\":3}}}");
            var tool=ItemStack.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),toolJson).getOrThrow();
            var params=new LootParams(level,ContextMap.builder().set(LootContextParams.TOOL,tool).set(LootContextParams.BLOCK_STATE,Blocks.WHEAT.defaultBlockState()).set(LootContextParams.EXPLOSION_RADIUS,3f).build(),Map.of(),0);
            var random=RandomSource.create(seed);RANDOM.set(random);var context=new LootContext.Builder(params).withOptionalRandomSource(random).create(Optional.empty());
            var record=new JsonObject();record.add("definition",functionJson);record.add("input",inputJson);record.add("tool",toolJson);record.addProperty("seed",seed);record.add("result",stack(function.apply(item,context)));record.addProperty("next",random.nextLong());modifiers.add(record);
        }output.add("modifiers",modifiers);
        var numbers=new JsonArray();
        for(String definition:List.of("9223372036854775808","1e100","2147483647.9","-2147483649.99","{\"type\":\"uniform\",\"min\":-4,\"max\":9}","{\"type\":\"binomial\",\"n\":12,\"p\":0.3}","{\"type\":\"add\",\"inputs\":[5,{\"type\":\"div\",\"left\":1,\"right\":0}]}","{\"type\":\"floor_div\",\"left\":-7,\"right\":3}","{\"type\":\"from_float\",\"input\":-2.9}","{\"type\":\"pow\",\"base\":0,\"exponent\":0}")) {
            var json=JsonParser.parseString(definition);var value=net.minecraft.world.level.storage.loot.providers.number.ints.ContextIntProviders.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            for(long seed:SEEDS) {var random=RandomSource.create(seed);RANDOM.set(random);var context=new LootContext.Builder(new LootParams(level,ContextMap.EMPTY,Map.of(),0)).withOptionalRandomSource(random).create(Optional.empty());
                var record=new JsonObject();record.add("definition",json);record.addProperty("seed",seed);record.addProperty("value",value.getInt(context));record.addProperty("next",random.nextLong());numbers.add(record);}
        }
        output.add("int_providers",numbers);
        var floats=new JsonArray();
        for(String definition:List.of("1e100","{\"type\":\"round\",\"input\":{\"type\":\"mod\",\"left\":{\"type\":\"uniform\",\"min\":1,\"max\":2},\"right\":0}}","{\"type\":\"add\",\"inputs\":[3,{\"type\":\"round\",\"input\":{\"type\":\"div\",\"left\":0,\"right\":0}}]}","{\"type\":\"sin\",\"input\":2.3}"))for(long seed:SEEDS){
            var json=JsonParser.parseString(definition);var value=net.minecraft.world.level.storage.loot.providers.number.floats.ContextFloatProviders.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var random=RandomSource.create(seed);RANDOM.set(random);var context=new LootContext.Builder(new LootParams(level,ContextMap.EMPTY,Map.of(),0)).withOptionalRandomSource(random).create(Optional.empty());
            var record=new JsonObject();record.add("definition",json);record.addProperty("seed",seed);record.addProperty("bits",Float.floatToRawIntBits(value.getFloat(context)));record.addProperty("next",random.nextLong());floats.add(record);
        }output.add("float_providers",floats);

        var sequences=new JsonArray();
        for(long worldSeed:SEEDS)for(int salt:new int[]{0,-712})for(boolean includeWorld:new boolean[]{false,true})for(boolean includeId:new boolean[]{false,true}) {
            var manager=new net.minecraft.world.RandomSequences();manager.setSeedDefaults(salt,includeWorld,includeId);
            var key=Identifier.withDefaultNamespace("entities/pig");var random=manager.get(key,worldSeed);
            var draws=new JsonArray();for(int i=0;i<5;i++)draws.add(random.nextLong());
            var record=new JsonObject();record.addProperty("world_seed",worldSeed);record.addProperty("salt",salt);record.addProperty("include_world_seed",includeWorld);record.addProperty("include_sequence_id",includeId);record.addProperty("key",key.toString());record.add("draws",draws);
            record.add("saved",net.minecraft.world.RandomSequences.CODEC.encodeStart(JsonOps.INSTANCE,manager).getOrThrow());sequences.add(record);
        }output.add("random_sequences",sequences);
        var blockPredicates=new JsonArray();
        for(String block:List.of("stone","oak_log","oak_stairs","wheat"))for(String test:List.of("{}","{\"blocks\":\"stone\"}","{\"blocks\":\"#minecraft:logs\"}","{\"state\":{\"axis\":\"y\"}}","{\"state\":{\"age\":\"+0\"}}","{\"state\":{\"age\":\"٠\"}}","{\"state\":{\"age\":{\"min\":\"0\",\"max\":\"3\"}}}")) {
            var state=BuiltInRegistries.BLOCK.getValue(Identifier.parse(block)).defaultBlockState();
            var json=JsonParser.parseString(test);var predicate=net.minecraft.advancements.predicates.BlockPredicate.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var record=new JsonObject();record.addProperty("block",block);record.add("predicate",json);record.addProperty("matches",predicate.matchesState(state));blockPredicates.add(record);
        }output.add("block_predicates",blockPredicates);
        var locations=new JsonArray();
        doReturn(true).when(level).isLoaded(any(net.minecraft.core.BlockPos.class));doReturn(net.minecraft.world.level.Level.OVERWORLD).when(level).dimension();
        doReturn(7).when(level).getMaxLocalRawBrightness(any(net.minecraft.core.BlockPos.class));doReturn(true).when(level).canSeeSky(any(net.minecraft.core.BlockPos.class));
        doReturn(Blocks.STONE.defaultBlockState()).when(level).getBlockState(any(net.minecraft.core.BlockPos.class));
        for(String test:List.of("{}","{\"position\":{\"y\":{\"min\":60,\"max\":64}}}","{\"dimension\":\"the_nether\"}","{\"light\":{\"light\":7}}","{\"can_see_sky\":false}","{\"block\":{\"blocks\":\"stone\"}}"))for(double y:new double[]{59.9,64,64.1}) {
            var json=JsonParser.parseString(test);var predicate=net.minecraft.advancements.predicates.LocationPredicate.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var record=new JsonObject();record.add("predicate",json);record.addProperty("y",y);record.addProperty("matches",predicate.matches(level,0.5,y,-0.5));locations.add(record);
        }output.add("location_predicates",locations);

    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();lookup=load();BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        var server=missing(MinecraftServer.class);level=missing(ServerLevel.class);
        doReturn(server).when(level).getServer();doReturn(FeatureFlags.VANILLA_SET).when(level).enabledFeatures();
        doReturn(new ReloadableServerRegistries.Holder(lookup)).when(server).reloadableRegistries();
        doAnswer(i->RANDOM.get()).when(server).getRandomSequence(any());doAnswer(i->RANDOM.get()).when(level).getRandom();
        var output=new JsonObject();var vectors=new JsonArray();var pending=new JsonArray();
        var tables=lookup.lookupOrThrow(Registries.LOOT_TABLE).listElements().sorted(Comparator.comparing(h->h.key().identifier())).toList();
        for(var holder:tables) {
            var id=holder.key().identifier();var table=holder.value();
            for(long seed:SEEDS) {
                var result=new JsonObject();result.addProperty("table",id.toString());result.addProperty("seed",seed);
                try {
                    var params=params(id,table);var random=RandomSource.create(seed);RANDOM.set(random);
                    result.add("items",stacks(table.getRandomItems(params,random)));result.addProperty("next",random.nextLong());
                    random=RandomSource.create(seed);RANDOM.set(random);var container=new SimpleContainer(27);
                    table.fill(container,params,0);
                    var slots=new JsonArray();for(int slot=0;slot<27;slot++)slots.add(stack(container.getItem(slot)));
                    result.add("slots",slots);result.addProperty("fill_next",random.nextLong());vectors.add(result);
                } catch(UnsupportedOperationException e) {result.addProperty("missing_fact",e.getMessage());pending.add(result);}
            }
        }
        focused(output);output.addProperty("tables",tables.size());output.add("vectors",vectors);output.add("pending",pending);
        Files.writeString(Path.of(args[0]),JSON.toJson(output)+"\n");
        System.out.println("tables="+tables.size()+", vectors="+vectors.size()+", pending="+pending.size());
    }
}
