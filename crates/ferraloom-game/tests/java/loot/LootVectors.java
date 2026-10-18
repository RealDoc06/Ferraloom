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
    static JsonElement stackNbt(ItemStack stack) {return stack.isEmpty()?JsonNull.INSTANCE:new JsonPrimitive(ItemStack.CODEC.encodeStart(lookup.createSerializationContext(net.minecraft.nbt.NbtOps.INSTANCE),stack).getOrThrow().toString());}
    static JsonArray stacksNbt(List<ItemStack> stacks) {var a=new JsonArray();for(var stack:stacks)a.add(stackNbt(stack));return a;}

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
            else if(key==LootContextParams.DAMAGE_SOURCE) values.set(LootContextParams.DAMAGE_SOURCE,new net.minecraft.world.damagesource.DamageSource(lookup.lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(net.minecraft.world.damagesource.DamageTypes.GENERIC)));
            else if(key==LootContextParams.TARGET_ENTITY) values.set(LootContextParams.TARGET_ENTITY,entity);
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
        var generated=table.getRandomItems(params,random);result.add("items",stacks(generated));result.add("items_nbt",stacksNbt(generated));result.addProperty("next",random.nextLong());
        random=RandomSource.create(seed);RANDOM.set(random);var container=new SimpleContainer(9);container.setItem(3,new ItemStack(Items.APPLE));
        table.fill(container,params,0);var slots=new JsonArray();var slotsNbt=new JsonArray();for(int slot=0;slot<9;slot++){slots.add(stack(container.getItem(slot)));slotsNbt.add(stackNbt(container.getItem(slot)));}
        result.add("slots",slots);result.add("slots_nbt",slotsNbt);result.addProperty("fill_next",random.nextLong());return result;
    }
    static void focused(JsonObject output) throws Exception {
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
            "{\"type\":\"discard\"}",
            "{\"type\":\"limit_count\",\"limit\":{\"min\":{\"type\":\"uniform\",\"min\":1,\"max\":5},\"max\":{\"type\":\"uniform\",\"min\":2,\"max\":6}}}",
            "{\"type\":\"set_ominous_bottle_amplifier\",\"amplifier\":{\"type\":\"uniform\",\"min\":-3,\"max\":8}}",
            "{\"type\":\"set_potion\",\"id\":\"healing\"}",
            "{\"type\":\"set_instrument\",\"options\":[\"ponder_goat_horn\",\"sing_goat_horn\"]}",
            "{\"type\":\"sequence\",\"functions\":[{\"type\":\"set_custom_data\",\"tag\":\"{targets:[{keep:1},{keep:2},17],lists:[[1],{skip:1},[3]]}\"},{\"type\":\"copy_custom_data\",\"source\":\"block_entity\",\"ops\":[{\"source\":\"data.obj\",\"target\":\"targets[]\",\"op\":\"merge\"},{\"source\":\"data.values[]\",\"target\":\"lists[]\",\"op\":\"append\"}]}]}",
            "{\"type\":\"set_loot_table\",\"loot_table_id\":\"archaeology/desert_well\",\"seed\":9223372036854775808}",
            "{\"type\":\"set_components\",\"components\":{\"container\":[{\"slot\":0,\"item\":{\"id\":\"stone\",\"count\":65}}]}}",
            "{\"type\":\"set_components\",\"components\":{\"charged_projectiles\":[{\"id\":\"stone\",\"count\":65}]}}",
            "{\"type\":\"set_lore\",\"lore\":[\"one\",\"two\"],\"mode\":\"replace_all\"}",
            "{\"type\":\"set_book_cover\",\"title\":\"Title\",\"author\":\"Doc\",\"generation\":2}",
            "{\"type\":\"set_written_book_pages\",\"pages\":[\"hello\"],\"mode\":\"append\"}",
            "{\"type\":\"set_writable_book_pages\",\"pages\":[\"draft\"],\"mode\":\"insert\",\"offset\":0}",
            "{\"type\":\"set_firework_explosion\",\"shape\":\"creeper\",\"colors\":[16711680],\"trail\":true}",
            "{\"type\":\"set_fireworks\",\"flight_duration\":256,\"explosions\":{\"values\":[{\"shape\":\"star\",\"colors\":[65280]}],\"mode\":\"append\"}}",
            "{\"type\":\"set_custom_model_data\",\"floats\":{\"values\":[{\"type\":\"uniform\",\"min\":0,\"max\":1},2],\"mode\":\"append\"},\"colors\":{\"values\":[[1,0.5,0]],\"mode\":\"replace_all\"},\"flags\":{\"values\":[true,false],\"mode\":\"append\"}}",
            "{\"type\":\"set_attributes\",\"replace\":false,\"modifiers\":[{\"id\":\"test\",\"attribute\":\"attack_damage\",\"operation\":\"add_value\",\"amount\":{\"type\":\"uniform\",\"min\":1,\"max\":3},\"slot\":[\"mainhand\",\"offhand\"]},{\"id\":\"test\",\"attribute\":\"attack_damage\",\"operation\":\"add_value\",\"amount\":5,\"slot\":\"any\"}]}",
            "{\"type\":\"set_random_dyes\",\"number_of_dyes\":3}",
            "{\"type\":\"set_contents\",\"component\":\"container\",\"entries\":[{\"type\":\"group\",\"children\":[{\"type\":\"item\",\"name\":\"stone\",\"modifier\":{\"type\":\"set_count\",\"count\":{\"type\":\"uniform\",\"min\":1,\"max\":8}}},{\"type\":\"item\",\"name\":\"apple\",\"condition\":{\"type\":\"random_chance\",\"chance\":0.5}}]}]}",
            "{\"type\":\"modify_contents\",\"component\":\"container\",\"modifier\":{\"type\":\"set_count\",\"count\":70}}",
            "{\"type\":\"copy_custom_data\",\"source\":\"block_entity\",\"ops\":[{\"source\":\"data.x\",\"target\":\"copy\",\"op\":\"replace\"},{\"source\":\"data.obj\",\"target\":\"nested\",\"op\":\"merge\"},{\"source\":\"data.values[]\",\"target\":\"list\",\"op\":\"append\"}]}",
            "{\"type\":\"fill_player_head\",\"entity\":\"this\"}",

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
        for(var definition:functionDefinitions)for(String input:List.of("{\"id\":\"book\",\"count\":2}","{\"id\":\"suspicious_stew\"}","{\"id\":\"stone\",\"count\":16}","{\"id\":\"chest\",\"components\":{\"container\":[{\"slot\":1,\"item\":{\"id\":\"stone\",\"count\":3}}]}}"))for(long seed:SEEDS){
            var functionJson=JsonParser.parseString(definition);var inputJson=JsonParser.parseString(input);
            var function=net.minecraft.world.level.storage.loot.functions.LootItemFunctions.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),functionJson).getOrThrow();
            var item=ItemStack.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),inputJson).getOrThrow();
            var toolJson=JsonParser.parseString("{\"id\":\"diamond_pickaxe\",\"components\":{\"damage\":17,\"enchantments\":{\"fortune\":3}}}");
            var tool=ItemStack.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),toolJson).getOrThrow();
            var sourceTag=new net.minecraft.nbt.CompoundTag();var sourceData=new net.minecraft.nbt.CompoundTag();sourceData.putInt("x",17);var obj=new net.minecraft.nbt.CompoundTag();obj.putInt("a",3);sourceData.put("obj",obj);var values=new net.minecraft.nbt.ListTag();values.add(net.minecraft.nbt.IntTag.valueOf(2));values.add(net.minecraft.nbt.IntTag.valueOf(4));sourceData.put("values",values);sourceTag.put("data",sourceData);
            var blockEntity=missing(net.minecraft.world.level.block.entity.BlockEntity.class);doReturn(level).when(blockEntity).getLevel();doReturn(RegistryAccess.EMPTY).when(level).registryAccess();doReturn(sourceTag).when(blockEntity).saveWithFullMetadata(any(HolderLookup.Provider.class));
            var params=new LootParams(level,ContextMap.builder().set(LootContextParams.BLOCK_ENTITY,blockEntity).set(LootContextParams.TOOL,tool).set(LootContextParams.BLOCK_STATE,Blocks.WHEAT.defaultBlockState()).set(LootContextParams.EXPLOSION_RADIUS,3f).build(),Map.of(),0);
            var random=RandomSource.create(seed);RANDOM.set(random);var context=new LootContext.Builder(params).withOptionalRandomSource(random).create(Optional.empty());
            var record=new JsonObject();record.add("definition",functionJson);record.add("input",inputJson);record.add("tool",toolJson);record.addProperty("block_entity",sourceTag.toString());record.addProperty("seed",seed);var modified=function.apply(item,context);record.add("result",stack(modified));record.add("result_nbt",stackNbt(modified));record.addProperty("next",random.nextLong());modifiers.add(record);
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
        var sequenceDirty=new JsonArray();
        for(boolean initialDirty:new boolean[]{false,true})for(String action:List.of("get","defaults","clear","reset","consume_zero","consume_negative","int","bound","long","boolean","float","double","gaussian","fork","positional","seed","consume_one")) {
            var manager=new net.minecraft.world.RandomSequences();var key=Identifier.withDefaultNamespace("entities/pig");var random=manager.get(key,42);manager.setDirty(initialDirty);
            switch(action) {
                case "get" -> manager.get(key,42);
                case "defaults" -> manager.setSeedDefaults(71,false,false);
                case "clear" -> manager.clear();
                case "reset" -> manager.reset(key,42,-3,false,true);
                case "consume_zero" -> random.consumeCount(0);
                case "consume_negative" -> random.consumeCount(-1);
                case "int" -> random.nextInt();
                case "bound" -> random.nextInt(17);
                case "long" -> random.nextLong();
                case "boolean" -> random.nextBoolean();
                case "float" -> random.nextFloat();
                case "double" -> random.nextDouble();
                case "gaussian" -> random.nextGaussian();
                case "fork" -> random.fork();
                case "positional" -> random.forkPositional();
                case "seed" -> random.setSeed(-17);
                case "consume_one" -> random.consumeCount(1);
            }
            var record=new JsonObject();record.addProperty("initial_dirty",initialDirty);record.addProperty("action",action);record.addProperty("dirty",manager.isDirty());record.addProperty("saved_nbt",net.minecraft.world.RandomSequences.CODEC.encodeStart(net.minecraft.nbt.NbtOps.INSTANCE,manager).getOrThrow().toString());record.addProperty("next",manager.get(key,42).nextLong());sequenceDirty.add(record);
        }output.add("sequence_dirty",sequenceDirty);
        var sequenceCodecs=new JsonArray();
        for(String snbt:List.of("{salt:0,sequences:{}}","{salt:-1.9d,include_world_seed:0.5d,include_sequence_id:0b,sequences:{'minecraft:test':{source:[1,2]}}}","{salt:2147483648L,sequences:{'minecraft:test':{source:[B;1b,2b]}}}","{salt:0,sequences:{'minecraft:test':{source:[L;0L,0L]}}}","{salt:0,sequences:{'minecraft:test':{source:[1.9d,-1.9d]}}}","{salt:0,include_world_seed:'true',sequences:{}}","{sequences:{}}","{salt:0,sequences:{'minecraft:test':{source:[1]}}}")) {
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully(snbt);var decoded=net.minecraft.world.RandomSequences.CODEC.parse(net.minecraft.nbt.NbtOps.INSTANCE,tag);var record=new JsonObject();record.addProperty("input",snbt);record.addProperty("accepted",decoded.isSuccess());decoded.result().ifPresent(value->record.addProperty("output",net.minecraft.world.RandomSequences.CODEC.encodeStart(net.minecraft.nbt.NbtOps.INSTANCE,value).getOrThrow().toString()));sequenceCodecs.add(record);
        }output.add("sequence_codecs",sequenceCodecs);
        var doubleMatches=new JsonArray();
        for(String[] endpoints:new String[][]{{null,null},{null,"0.0"},{"0.0",null},{"0.0","0.0"},{"NaN","NaN"},{null,"NaN"},{"NaN",null},{"-3.0","4.0"},{"Infinity","Infinity"},{"-Infinity","-Infinity"}})for(double value:new double[]{Double.NaN,Double.NEGATIVE_INFINITY,-4,-0.0,0.0,2,16,Double.POSITIVE_INFINITY}) {
            var bounds=endpoints[0]==null?(endpoints[1]==null?net.minecraft.advancements.predicates.MinMaxBounds.Doubles.ANY:net.minecraft.advancements.predicates.MinMaxBounds.Doubles.atMost(Double.parseDouble(endpoints[1]))):(endpoints[1]==null?net.minecraft.advancements.predicates.MinMaxBounds.Doubles.atLeast(Double.parseDouble(endpoints[0])):net.minecraft.advancements.predicates.MinMaxBounds.Doubles.between(Double.parseDouble(endpoints[0]),Double.parseDouble(endpoints[1])));
            var record=new JsonObject();if(endpoints[0]!=null)record.addProperty("min",endpoints[0]);if(endpoints[1]!=null)record.addProperty("max",endpoints[1]);record.addProperty("value",Double.toString(value));record.addProperty("matches",bounds.matches(value));record.addProperty("matches_sqr",bounds.matchesSqr(value));doubleMatches.add(record);
        }output.add("double_matches",doubleMatches);
        var registryCodecs=new JsonArray();
        for(String[] test:new String[][]{
            {"item","{\"items\":\"not_an_item\"}"},{"item","{\"items\":\"stone\"}"},{"item","{\"items\":[\"stone\",\"not_an_item\"]}"},
            {"block","{\"blocks\":\"not_a_block\"}"},{"block","{\"blocks\":\"stone\"}"},
            {"entity","{\"entity_type\":\"not_an_entity\"}"},{"entity","{\"entity_type\":\"pig\"}"},{"entity","{\"entity_type\":\"pig\",\"minecraft:entity_type\":\"sheep\"}"},
            {"fluid","{\"fluids\":\"not_a_fluid\"}"},{"fluid","{\"fluids\":\"water\"}"},
            {"function","{\"type\":\"set_enchantments\",\"enchantments\":{\"sharpness\":1,\"minecraft:sharpness\":2}}"},
            {"dynamic_function","{\"type\":\"set_enchantments\",\"enchantments\":{\"not_an_enchantment\":1}}"},
            {"dynamic_function","{\"type\":\"set_enchantments\",\"enchantments\":{\"sharpness\":1}}"},
            {"dynamic_function","{\"type\":\"enchant_randomly\",\"options\":\"#not_a_tag\"}"},
            {"dynamic_function","{\"type\":\"set_instrument\",\"options\":\"#regular_goat_horns\"}"}
        }) {
            var definition=JsonParser.parseString(test[1]);var ops=lookup.createSerializationContext(JsonOps.INSTANCE);
            boolean accepted=switch(test[0]) {
                case "item" -> net.minecraft.advancements.predicates.ItemPredicate.CODEC.parse(ops,definition).isSuccess();
                case "block" -> net.minecraft.advancements.predicates.BlockPredicate.CODEC.parse(ops,definition).isSuccess();
                case "entity" -> net.minecraft.advancements.predicates.entity.EntityPredicate.CODEC.parse(ops,definition).isSuccess();
                case "fluid" -> net.minecraft.advancements.predicates.FluidPredicate.CODEC.parse(ops,definition).isSuccess();
                default -> net.minecraft.world.level.storage.loot.functions.LootItemFunctions.DIRECT_CODEC.parse(ops,definition).isSuccess();
            };
            var record=new JsonObject();record.addProperty("kind",test[0]);record.add("definition",definition);record.addProperty("accepted",accepted);registryCodecs.add(record);
        }output.add("registry_codecs",registryCodecs);
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
        var movements=new JsonArray();
        for(String test:List.of("{}","{\"x\":{\"max\":0}}","{\"speed\":{\"min\":-3,\"max\":5}}","{\"horizontal_speed\":2}","{\"vertical_speed\":{\"min\":2}}","{\"fall_distance\":{\"max\":0.5}}"))for(double x:new double[]{-0.1,0,0.3}) {
            var json=JsonParser.parseString(test);var predicate=net.minecraft.advancements.predicates.entity.MovementPredicate.CODEC.parse(JsonOps.INSTANCE,json).getOrThrow();
            var record=new JsonObject();record.add("predicate",json);record.addProperty("x",x);record.addProperty("matches",predicate.matches(x*20,-0.1*20,0.05*20,0.75));movements.add(record);
        }output.add("movement_predicates",movements);
        var distances=new JsonArray();
        for(String test:List.of("{}","{\"x\":0.1}","{\"absolute\":{\"max\":1}}","{\"horizontal\":{\"min\":-1,\"max\":2}}"))for(double x:new double[]{0.1,1.00000001,3.0}) {
            var json=JsonParser.parseString(test);var predicate=net.minecraft.advancements.predicates.DistancePredicate.CODEC.parse(JsonOps.INSTANCE,json).getOrThrow();
            var record=new JsonObject();record.add("predicate",json);record.addProperty("x",x);record.addProperty("matches",predicate.matches(0,0,0,x,0.5,0));distances.add(record);
        }output.add("distance_predicates",distances);
        var fluids=new JsonArray();
        for(String fluid:List.of("empty","water","flowing_water","lava","flowing_lava"))for(String test:List.of("{}","{\"fluids\":\"water\"}","{\"state\":{\"falling\":\"false\"}}","{\"state\":{\"level\":{\"min\":\"+1\",\"max\":\"8\"}}}")) {
            var state=BuiltInRegistries.FLUID.getValue(Identifier.parse(fluid)).defaultFluidState();doReturn(state).when(level).getFluidState(any(net.minecraft.core.BlockPos.class));
            var json=JsonParser.parseString(test);var predicate=net.minecraft.advancements.predicates.FluidPredicate.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var record=new JsonObject();record.add("predicate",json);record.addProperty("fluid",fluid);var properties=new JsonObject();state.getValues().forEach(value->properties.addProperty(value.property().getName(),value.valueName()));record.add("properties",properties);record.addProperty("matches",predicate.matches(level,net.minecraft.core.BlockPos.ZERO));fluids.add(record);
        }output.add("fluid_predicates",fluids);
        var checks=new JsonArray();
        for(String test:List.of("{\"attribute\":\"gameplay/water_evaporates\",\"value\":true}","{\"attribute\":\"gameplay/water_evaporates\",\"value\":1}","{\"attribute\":\"visual/star_brightness\",\"value\":1.1}","{\"attribute\":\"visual/fog_start_distance\",\"value\":-1}","{\"attribute\":\"gameplay/eyeblossom_open\",\"value\":\"default\"}","{\"attribute\":\"visual/moon_phase\",\"value\":\"full_moon\"}","{\"attribute\":\"visual/moon_phase\",\"value\":0}","{\"attribute\":\"visual/fog_color\",\"value\":\"#010203\"}","{\"attribute\":\"visual/cloud_color\",\"value\":[0,0.5,1,0.75]}","{\"attribute\":\"gameplay/bed_rule\",\"value\":{\"can_sleep\":\"always\",\"can_set_spawn\":\"never\"}}")) {
            var json=JsonParser.parseString(test).getAsJsonObject();json.addProperty("type","environment_attribute_check");var record=new JsonObject();record.add("definition",json);record.addProperty("accepts",net.minecraft.world.level.storage.loot.predicates.LootItemCondition.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).result().isPresent());checks.add(record);
        }output.add("environment_codecs",checks);
        var damagePredicates=new JsonArray();var player=missing(net.minecraft.server.level.ServerPlayer.class);doReturn(level).when(player).level();doReturn(new Vec3(0,64,0)).when(player).position();
        var source=new net.minecraft.world.damagesource.DamageSource(lookup.lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(net.minecraft.world.damagesource.DamageTypes.GENERIC));
        for(String test:List.of("{}","{\"dealt\":{\"min\":1,\"max\":3}}","{\"taken\":1.5,\"blocked\":true}","{\"source_entity\":{}}","{\"type\":{\"is_direct\":true}}","{\"type\":{\"direct_entity\":{}}}"))for(float original:new float[]{0.5f,2.0f})for(boolean blocked:new boolean[]{false,true}) {
            var json=JsonParser.parseString(test);var predicate=net.minecraft.advancements.predicates.DamagePredicate.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var record=new JsonObject();record.add("predicate",json);record.addProperty("original",original);record.addProperty("blocked",blocked);record.addProperty("matches",predicate.matches(player,source,original,1.5f,blocked));damagePredicates.add(record);
        }output.add("damage_predicates",damagePredicates);
        var pages=new JsonArray();
        for(String character:List.of("a","\u2028","\ud83d\ude00"))for(int length:character.equals("a")?new int[]{32765,32766}:character.equals("\u2028")?new int[]{5460,5461}:new int[]{16382,16383}) {
            var definition=new JsonObject();definition.addProperty("type","set_written_book_pages");definition.addProperty("mode","replace_all");var content=new JsonArray();content.add(character.repeat(length));definition.add("pages",content);
            var record=new JsonObject();record.addProperty("character",character);record.addProperty("length",length);record.addProperty("accepts",net.minecraft.world.level.storage.loot.functions.LootItemFunctions.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),definition).result().isPresent());pages.add(record);
        }output.add("book_page_codecs",pages);
        var conditions=new JsonArray();
        doReturn(true).when(level).isRaining();doReturn(false).when(level).isThundering();
        var clockManager=missing(net.minecraft.world.clock.ServerClockManager.class);var clock=missing(net.minecraft.world.clock.ServerClockManager.ServerClockInstance.class);doReturn(23567L).when(clock).totalTicks();doReturn(clock).when(clockManager).getInstance(any());doReturn(clockManager).when(level).clockManager();
        var environment=missing(net.minecraft.world.attribute.EnvironmentAttributeSystem.class);doReturn(7f).when(environment).getValue(any(LootContext.class),same(net.minecraft.world.attribute.EnvironmentAttributes.SKY_LIGHT_LEVEL));doReturn(environment).when(level).environmentAttributes();
        var conditionToolJson=JsonParser.parseString("{\"id\":\"diamond_pickaxe\",\"components\":{\"damage\":17,\"enchantments\":{\"fortune\":3}}}");
        var conditionTool=ItemStack.CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),conditionToolJson).getOrThrow();
        var conditionParams=new LootParams(level,ContextMap.builder().set(LootContextParams.ORIGIN,new Vec3(0.5,64,0.5)).set(LootContextParams.TOOL,conditionTool).set(LootContextParams.BLOCK_STATE,Blocks.WHEAT.defaultBlockState()).set(LootContextParams.EXPLOSION_RADIUS,3f).set(LootContextParams.ENCHANTMENT_ACTIVE,true).set(LootContextParams.DAMAGE_SOURCE,source).build(),Map.of(),0);
        for(String definition:List.of(
            "{\"type\":\"inverted\",\"term\":{\"type\":\"random_chance\",\"chance\":0.4}}",
            "{\"type\":\"all_of\",\"terms\":[{\"type\":\"random_chance\",\"chance\":0.4},{\"type\":\"random_chance\",\"chance\":0.4}]}",
            "{\"type\":\"any_of\",\"terms\":[{\"type\":\"random_chance\",\"chance\":0.4},{\"type\":\"random_chance\",\"chance\":0.4}]}",
            "{\"type\":\"random_chance\",\"chance\":{\"type\":\"uniform\",\"min\":0.1,\"max\":0.9}}",
            "{\"type\":\"random_chance_with_enchanted_bonus\",\"unenchanted_chance\":0.4,\"enchanted_chance\":{\"type\":\"linear\",\"base\":0.5,\"per_level_above_first\":0.1},\"enchantment\":\"fortune\"}",
            "{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{}}",
            "{\"type\":\"killed_by_player\"}",
            "{\"type\":\"entity_scores\",\"entity\":\"this\",\"scores\":{\"test\":3}}",
            "{\"type\":\"match_block\",\"blocks\":\"wheat\",\"state\":{\"age\":\"0\"}}",
            "{\"type\":\"match_tool\",\"predicate\":{\"items\":\"diamond_pickaxe\",\"predicates\":{\"damage\":{\"damage\":17}}}}",
            "{\"type\":\"table_bonus\",\"enchantment\":\"fortune\",\"chances\":[0.0,0.1,0.2,0.9]}",
            "{\"type\":\"survives_explosion\"}",
            "{\"type\":\"damage_source_properties\",\"predicate\":{\"is_direct\":true}}",
            "{\"type\":\"location_check\",\"predicate\":{\"position\":{\"y\":64},\"light\":{\"light\":7}}}",
            "{\"type\":\"weather_check\",\"raining\":true,\"thundering\":false}",
            "{\"type\":\"time_check\",\"clock\":\"overworld\",\"period\":24000.9,\"value\":{\"min\":23500,\"max\":23600}}",
            "{\"type\":\"int_value_check\",\"value\":{\"type\":\"uniform\",\"min\":0,\"max\":8},\"test\":{\"min\":2,\"max\":6}}",
            "{\"type\":\"float_value_check\",\"value\":{\"type\":\"uniform\",\"min\":0,\"max\":8},\"test\":{\"min\":2.5,\"max\":6.3}}",
            "{\"type\":\"enchantment_active_check\",\"active\":true}",
            "{\"type\":\"environment_attribute_check\",\"attribute\":\"gameplay/sky_light_level\",\"value\":7}"
        ))for(long seed:SEEDS){
            var json=JsonParser.parseString(definition);var condition=net.minecraft.world.level.storage.loot.predicates.LootItemCondition.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var random=RandomSource.create(seed);RANDOM.set(random);var context=new LootContext.Builder(conditionParams).withOptionalRandomSource(random).create(Optional.empty());
            var record=new JsonObject();record.add("definition",json);record.addProperty("seed",seed);record.add("tool",conditionToolJson);record.addProperty("matches",condition.test(context));record.addProperty("next",random.nextLong());conditions.add(record);
        }output.add("conditions",conditions);





    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();lookup=load();BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        if(args[0].equals("--map-order")) {
            var json=JsonParser.parseString("{\"type\":\"set_enchantments\",\"enchantments\":{\"fire_protection\":1,\"projectile_protection\":2,\"protection\":3}}");
            var function=net.minecraft.world.level.storage.loot.functions.LootItemFunctions.DIRECT_CODEC.parse(lookup.createSerializationContext(JsonOps.INSTANCE),json).getOrThrow();
            var encoded=net.minecraft.world.level.storage.loot.functions.LootItemFunctions.DIRECT_CODEC.encodeStart(lookup.createSerializationContext(JsonOps.INSTANCE),function).getOrThrow();
            System.out.println("enchantment_map_order="+encoded.getAsJsonObject().getAsJsonObject("enchantments").keySet());return;
        }
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
                    var generated=table.getRandomItems(params,random);result.add("items",stacks(generated));result.add("items_nbt",stacksNbt(generated));result.addProperty("next",random.nextLong());
                    random=RandomSource.create(seed);RANDOM.set(random);var container=new SimpleContainer(27);
                    table.fill(container,params,0);
                    var slots=new JsonArray();var slotsNbt=new JsonArray();for(int slot=0;slot<27;slot++){slots.add(stack(container.getItem(slot)));slotsNbt.add(stackNbt(container.getItem(slot)));}
                    result.add("slots",slots);result.add("slots_nbt",slotsNbt);result.addProperty("fill_next",random.nextLong());vectors.add(result);
                } catch(UnsupportedOperationException e) {result.addProperty("missing_fact",e.getMessage());pending.add(result);}
            }
        }
        focused(output);output.addProperty("tables",tables.size());output.add("vectors",vectors);output.add("pending",pending);
        Files.writeString(Path.of(args[0]),JSON.toJson(output)+"\n");
        System.out.println("tables="+tables.size()+", vectors="+vectors.size()+", pending="+pending.size());
    }
}
