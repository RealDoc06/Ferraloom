import com.google.gson.*;
import com.mojang.serialization.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.SharedConstants;
import net.minecraft.advancements.*;
import net.minecraft.advancements.predicates.entity.EntityPredicate;
import net.minecraft.advancements.triggers.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.util.RandomSource;
import net.minecraft.util.context.ContextMap;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.item.*;
import net.minecraft.world.item.alchemy.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.storage.loot.*;
import net.minecraft.world.level.storage.loot.parameters.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Observations from actual vanilla codecs, matchers and player progression. */
public final class AdvancementVectors {
    static final Gson JSON=new GsonBuilder().serializeNulls().disableHtmlEscaping().create();
    static RegistryAccess.Frozen lookup;
    static RegistryOps<JsonElement> ops;
    static ServerLevel level;
    static MinecraftServer server;
    static ServerPlayer player;
    static Entity pig,cow;
    static final Vec3 ORIGIN=new Vec3(1,64,2),START=new Vec3(-3,60,2);
    static final JsonObject OUT=new JsonObject();
    static <T> T missing(Class<T> cls) {return mock(cls,call->{throw new UnsupportedOperationException(cls.getSimpleName()+"."+call.getMethod().getName());});}
    static Identifier id(String s) {return Identifier.parse(s);}
    static JsonElement json(String s) {return JsonParser.parseString(s);}
    static Entity entity(EntityType<?> type) {
        var e=missing(Entity.class);doReturn(type).when(e).getType();doReturn(type.builtInRegistryHolder()).when(e).typeHolder();doReturn(ORIGIN).when(e).position();doReturn(1.0).when(e).getX();doReturn(64.0).when(e).getY();doReturn(2.0).when(e).getZ();return e;
    }
    static void setup() {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        var resources=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(resources,builtins).forEach(Registry.PendingTags::apply);
        var world=RegistryDataLoader.load(resources,builtins.listRegistries().toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
        TagLoader.loadTagsForExistingRegistries(resources,world).forEach(Registry.PendingTags::apply);
        var context=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        var reload=RegistryDataLoader.load(resources,context.listRegistries().toList(),RegistryDataLoader.RELOADABLE_REGISTRIES,Runnable::run).join();
        lookup=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(context.registries(),reload.registries())).freeze();ops=lookup.createSerializationContext(JsonOps.INSTANCE);BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(pending->pending.apply());
        level=missing(ServerLevel.class);server=missing(MinecraftServer.class);player=missing(ServerPlayer.class);pig=entity(EntityTypes.PIG);cow=entity(EntityTypes.COW);
        doReturn(net.minecraft.world.flag.FeatureFlags.VANILLA_SET).when(level).enabledFeatures();doReturn(server).when(level).getServer();doReturn(RandomSource.create(42)).when(level).getRandom();doReturn(lookup).when(level).registryAccess();doReturn(Level.OVERWORLD).when(level).dimension();doReturn(true).when(level).isLoaded(any());
        var reloadable=mock(ReloadableServerRegistries.Holder.class);doReturn(lookup).when(reloadable).lookup();doReturn(reloadable).when(server).reloadableRegistries();
        doReturn(EntityTypes.PLAYER).when(player).getType();doReturn(EntityTypes.PLAYER.builtInRegistryHolder()).when(player).typeHolder();doReturn(level).when(player).level();doReturn(ORIGIN).when(player).position();doReturn(1.0).when(player).getX();doReturn(64.0).when(player).getY();doReturn(2.0).when(player).getZ();doReturn(Map.of()).when(player).getActiveEffectsMap();doReturn(Component.literal("Doc")).when(player).getDisplayName();
    }
    static LootContext ctx(Entity entity) {return EntityPredicate.createContext(player,entity);}
    static ItemStack stack(int n) {if(n==2)return ItemStack.EMPTY;var s=new ItemStack(n==0?Items.STONE:Items.DIAMOND_PICKAXE,n==0?5:1);if(n==1)s.setDamageValue(17);return s;}
    static String wire(net.minecraft.network.protocol.game.ClientboundUpdateAdvancementsPacket packet) {
        var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),lookup);net.minecraft.network.protocol.game.ClientboundUpdateAdvancementsPacket.STREAM_CODEC.encode(buf,packet);var bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);buf.release();return HexFormat.of().formatHex(bytes);
    }
    static void builtins() {
        var manager=new ServerAdvancementManager(lookup);var rows=new JsonArray();
        for(var holder:manager.getAllAdvancements()) {
            var node=manager.tree().get(holder);var row=new JsonObject();row.addProperty("id",holder.id().toString());row.add("value",Advancement.CODEC.encodeStart(ops,holder.value()).getOrThrow());
            row.addProperty("x",node.x());row.addProperty("y",node.y());var children=new JsonArray();for(var child:node.children())children.add(child.holder().id().toString());row.add("children",children);rows.add(row);
        }
        OUT.add("builtins",rows);
    }
    static JsonObject base(String name) {
        var c=new JsonObject();if(name.contains("recipe_"))c.addProperty("recipes","minecraft:crafting_table");if(name.equals("player_generates_container_loot"))c.addProperty("loot_tables","minecraft:chests/simple_dungeon");return c;
    }
    static List<JsonObject> conditions(String name) {
        var list=new ArrayList<JsonObject>();list.add(base(name));
        String extra=switch(name) {
            case "enter_block","slide_down_block"->"{\"blocks\":[\"stone\",\"oak_log\"],\"state\":{\"axis\":\"y\"}}";
            case "inventory_changed"->"{\"slots\":{\"occupied\":3,\"full\":1,\"empty\":1},\"items\":[{\"items\":\"stone\"},{\"items\":\"stone\",\"count\":{\"min\":5}}]}";
            case "consume_item","filled_bucket","shot_crossbow","used_totem","using_item"->"{\"item\":{\"items\":\"stone\",\"count\":{\"min\":2}}}";
            case "player_killed_entity","entity_killed_player","kill_mob_near_sculk_catalyst"->"{\"entity\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}},\"killing_blow\":{\"is_direct\":true}}";
            case "player_hurt_entity","entity_hurt_player"->"{\"damage\":{\"dealt\":{\"min\":4},\"taken\":3,\"blocked\":false}}";
            case "enchanted_item"->"{\"item\":{\"items\":\"diamond_pickaxe\"},\"levels\":{\"min\":2}}";
            case "brewed_potion"->"{\"potion\":{\"potions\":\"healing\"}}";
            case "construct_beacon"->"{\"level\":{\"min\":2}}";
            case "used_ender_eye"->"{\"distance\":{\"min\":4,\"max\":5}}";
            case "bred_animals"->"{\"parent\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\"}},\"child\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}";
            case "cured_zombie_villager"->"{\"zombie\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}";
            case "villager_trade"->"{\"item\":{\"items\":\"stone\"}}";
            case "item_durability_changed"->"{\"durability\":{\"min\":100},\"delta\":{\"min\":1}}";
            case "levitation"->"{\"distance\":{\"y\":4},\"duration\":{\"min\":20}}";
            case "changed_dimension"->"{\"from\":\"overworld\",\"to\":\"the_nether\"}";
            case "effects_changed"->"{\"effects\":{\"speed\":{}},\"source\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}";
            case "nether_travel","fall_from_height","ride_entity_in_lava","fall_after_explosion"->"{\"start_position\":{\"position\":{\"y\":{\"max\":61}}},\"distance\":{\"absolute\":{\"min\":5}}}";
            case "fishing_rod_hooked"->"{\"item\":{\"items\":\"apple\"},\"rod\":{\"items\":\"stone\"}}";
            case "channeled_lightning","killed_by_arrow"->"{\"victims\":[{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}},{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}]}";
            case "spear_mobs"->"{\"count\":2}";
            case "bee_nest_destroyed"->"{\"blocks\":\"stone\",\"num_bees_inside\":{\"min\":2}}";
            case "target_hit"->"{\"signal_strength\":{\"min\":2}}";
            case "recipe_crafted","crafter_recipe_crafted"->"{\"ingredients\":[{\"items\":\"stone\"},{\"items\":\"stone\"}]}";
            case "thrown_item_picked_up_by_entity","thrown_item_picked_up_by_player","player_interacted_with_entity","player_sheared_equipment"->"{\"item\":{\"items\":\"stone\"}}";
            case "lightning_strike"->"{\"bystander\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}";
            case "summoned_entity","tame_animal"->"{\"entity\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}";
            case "placed_block","item_used_on_block","default_block_use","any_block_use","allay_drop_item_on_block"->"{\"location\":{\"type\":\"match_tool\",\"predicate\":{\"items\":\"stone\"}}}";
            default->"{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"player\"}}}";
        };
        var c=base(name);json(extra).getAsJsonObject().entrySet().forEach(e->c.add(e.getKey(),e.getValue()));list.add(c);return list;
    }
    static boolean matches(Object instance,int n) throws Exception {
        var entity=ctx(n==1?cow:pig);var item=stack(n);var block=(n==1?Blocks.OAK_LOG:Blocks.STONE).defaultBlockState();
        var source=new net.minecraft.world.damagesource.DamageSource(lookup.lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(net.minecraft.world.damagesource.DamageTypes.GENERIC),pig);
        var recipe=lookup.lookupOrThrow(Registries.RECIPE).getOrThrow(ResourceKey.create(Registries.RECIPE,id(n==1?"furnace":"crafting_table")));
        var params=new LootParams(level,ContextMap.builder().set(LootContextParams.THIS_ENTITY,player).set(LootContextParams.ORIGIN,ORIGIN).set(LootContextParams.TOOL,item).set(LootContextParams.BLOCK_STATE,block).buildAndValidate(LootContextParamSets.ADVANCEMENT_LOCATION),Map.of(),0);
        var location=new LootContext.Builder(params).create(Optional.empty());
        return switch(instance) {
            case ImpossibleTrigger.TriggerInstance i->false;
            case PlayerTrigger.TriggerInstance i->true;
            case StartRidingTrigger.TriggerInstance i->true;
            case ConsumeItemTrigger.TriggerInstance i->i.matches(item);
            case FilledBucketTrigger.TriggerInstance i->i.matches(item);
            case ShotCrossbowTrigger.TriggerInstance i->i.matches(item);
            case UsedTotemTrigger.TriggerInstance i->i.matches(item);
            case UsingItemTrigger.TriggerInstance i->i.matches(item);
            case SummonedEntityTrigger.TriggerInstance i->i.matches(entity);
            case TameAnimalTrigger.TriggerInstance i->i.matches(entity);
            case EnterBlockTrigger.TriggerInstance i->i.matches(block);
            case SlideDownBlockTrigger.TriggerInstance i->i.matches(block);
            case ItemUsedOnLocationTrigger.TriggerInstance i->i.matches(location);
            case AnyBlockInteractionTrigger.TriggerInstance i->i.matches(location);
            case DefaultBlockInteractionTrigger.TriggerInstance i->i.matches(location);
            case KilledTrigger.TriggerInstance i->i.matches(player,entity,source);
            case InventoryChangeTrigger.TriggerInstance i->{var inventory=new Inventory(player,new EntityEquipment());inventory.setItem(0,new ItemStack(Items.STONE,5));inventory.setItem(1,new ItemStack(Items.APPLE));inventory.setItem(3,new ItemStack(Items.STONE,64));yield i.matches(inventory,item,1,1,3);}
            case RecipeUnlockedTrigger.TriggerInstance i->i.matches(new RecipeHolder<>(recipe.key(),recipe.value()));
            case RecipeCraftedTrigger.TriggerInstance i->{var method=i.getClass().getDeclaredMethod("matches",ResourceKey.class,List.class);method.setAccessible(true);yield (boolean)method.invoke(i,recipe.key(),n==1?List.of(item):List.of(item,item));}
            case PlayerHurtEntityTrigger.TriggerInstance i->i.matches(player,entity,source,5,3,n==1);
            case EntityHurtPlayerTrigger.TriggerInstance i->i.matches(player,source,5,3,n==1);
            case EnchantedItemTrigger.TriggerInstance i->i.matches(item,n+1);
            case BrewedPotionTrigger.TriggerInstance i->i.matches(new PotionContents(n==0?Potions.HEALING:Potions.WATER));
            case ConstructBeaconTrigger.TriggerInstance i->i.matches(n+1);
            case UsedEnderEyeTrigger.TriggerInstance i->i.matches(n==0?16:n==1?25:26);
            case BredAnimalsTrigger.TriggerInstance i->i.matches(entity,ctx(cow),n==2?null:ctx(pig));
            case CuredZombieVillagerTrigger.TriggerInstance i->i.matches(entity,ctx(cow));
            case TradeTrigger.TriggerInstance i->i.matches(entity,item);
            case ItemDurabilityTrigger.TriggerInstance i->i.matches(item,10);
            case LevitationTrigger.TriggerInstance i->i.matches(player,START,n*20);
            case ChangeDimensionTrigger.TriggerInstance i->i.matches(Level.OVERWORLD,n==1?Level.NETHER:Level.OVERWORLD);
            case EffectsChangedTrigger.TriggerInstance i->i.matches(player,n==2?null:entity);
            case DistanceTrigger.TriggerInstance i->i.matches(level,START,ORIGIN);
            case FallAfterExplosionTrigger.TriggerInstance i->i.matches(level,START,ORIGIN,n==2?null:entity);
            case FishingRodHookedTrigger.TriggerInstance i->i.matches(item,entity,n==2?List.of():List.of(new ItemStack(Items.APPLE)));
            case ChanneledLightningTrigger.TriggerInstance i->i.matches(n==2?List.of():List.of(ctx(pig),ctx(cow)));
            case SpearMobsTrigger.TriggerInstance i->i.matches(n);
            case KilledByArrowTrigger.TriggerInstance i->i.matches(n==2?List.of():List.of(ctx(pig),ctx(cow)),n==2?0:2,n==2?null:item);
            case BeeNestDestroyedTrigger.TriggerInstance i->i.matches(block,item,n);
            case TargetBlockTrigger.TriggerInstance i->i.matches(entity,ORIGIN,n);
            case LootTableTrigger.TriggerInstance i->i.matches(ResourceKey.create(Registries.LOOT_TABLE,id(n==1?"chests/spawn_bonus_chest":"chests/simple_dungeon")));
            case PickedUpItemTrigger.TriggerInstance i->i.matches(player,item,entity);
            case PlayerInteractTrigger.TriggerInstance i->i.matches(item,entity);
            case LightningStrikeTrigger.TriggerInstance i->i.matches(entity,n==2?List.of():List.of(ctx(pig),ctx(cow)));
            default->throw new AssertionError(instance.getClass());
        };
    }
    static void triggers() throws Exception {
        var decodes=new JsonArray();var matches=new JsonArray();
        for(var trigger:BuiltInRegistries.TRIGGER_TYPES) {
            String name=BuiltInRegistries.TRIGGER_TYPES.getKey(trigger).getPath();
            var definitions=conditions(name);
            var specific=definitions.get(1);
            for(var field:new ArrayList<>(specific.keySet()))if(!field.equals("player")){var withNull=specific.deepCopy();withNull.add(field,JsonNull.INSTANCE);definitions.add(withNull);}
            var invalid=base(name);invalid.add("player",JsonNull.INSTANCE);definitions.add(invalid);
            invalid=base(name);invalid.add("player",json("{}"));definitions.add(invalid);
            invalid=base(name);invalid.add("player",json("[]"));definitions.add(invalid);
            for(var conditions:definitions) {
                var definition=new JsonObject();definition.addProperty("trigger",name);definition.add("conditions",conditions);
                var parsed=Criterion.CODEC.parse(ops,definition);var row=new JsonObject();row.add("input",definition);row.addProperty("accepted",parsed.isSuccess());decodes.add(row);
                if(parsed.isSuccess())for(int n=0;n<3;n++) {
                    var instance=parsed.getOrThrow().triggerInstance();var result=new JsonObject();result.add("input",definition);result.addProperty("scenario",n);result.addProperty("matches",matches(instance,n));matches.add(result);
                }
            }
        }
        OUT.add("trigger_decode",decodes);OUT.add("trigger_matches",matches);
    }
    static void progress() {
        var rows=new JsonArray();
        for(String raw:List.of("{}","{\"criteria\":{},\"done\":false}","{\"criteria\":{\"a\":\"2024-02-29 12:34:56 +0230\"},\"done\":true}","{\"criteria\":{\"a\":\"2023-02-29 12:34:56 +0000\",\"b\":\"1970-01-01 00:00:00 +0000\"}}","{\"criteria\":{\"a\":\"bad\"}}","{\"done\":null}","{\"done\":0}","{\"criteria\":null}")) {
            var row=new JsonObject();row.add("input",json(raw));var parsed=AdvancementProgress.CODEC.parse(JsonOps.INSTANCE,json(raw));row.addProperty("accepted",parsed.isSuccess());
            if(parsed.isSuccess()) {var p=parsed.getOrThrow();p.update(new AdvancementRequirements(List.of(List.of("a","b"),List.of("c"))));var steps=new JsonArray();for(String action:List.of("initial","grant:a","grant:a","grant:c","revoke:a","revoke:a","grant:b","revoke:c")) {
                boolean changed=false;if(action.startsWith("grant"))changed=p.grantProgress(action.substring(6));else if(action.startsWith("revoke"))changed=p.revokeProgress(action.substring(7));
                // Control the clock through the public stream codec, not a wall-clock fixture.
                if(action.startsWith("grant")&&changed) {var f=Arrays.stream(p.getClass().getDeclaredFields()).filter(field->field.getName().equals("criteria")).findFirst().orElseThrow();try{f.setAccessible(true);@SuppressWarnings("unchecked") var map=(Map<String,CriterionProgress>)f.get(p);map.put(action.substring(6),new CriterionProgress(Instant.ofEpochMilli(123456789)));}catch(Exception e){throw new RuntimeException(e);}}
                var step=new JsonObject();step.addProperty("action",action);step.addProperty("changed",changed);step.addProperty("done",p.isDone());step.addProperty("percent",p.getPercent());step.add("json",AdvancementProgress.CODEC.encodeStart(JsonOps.INSTANCE,p).getOrThrow());
                var buf=new net.minecraft.network.FriendlyByteBuf(Unpooled.buffer());AdvancementProgress.STREAM_CODEC.encode(buf,p);var bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);buf.release();step.addProperty("wire",HexFormat.of().formatHex(bytes));steps.add(step);
            }row.add("steps",steps);}
            rows.add(row);
        }
        OUT.add("progress",rows);
    }
    static void loadedProgress() {
        var rows=new JsonArray();
        for(var names:List.of(List.of("distract_piglin","distract_piglin_directly"),List.of("Aa","BB","AaAa","BBBB"))) {
            var criteria=new JsonObject();for(var name:names)criteria.addProperty(name,"1970-01-02 10:17:36 +0000");var input=new JsonObject();input.add("criteria",criteria);var requirements=new AdvancementRequirements(List.of(names));var progress=AdvancementProgress.CODEC.parse(JsonOps.INSTANCE,input).getOrThrow();progress.update(requirements);
            var buffer=new net.minecraft.network.FriendlyByteBuf(Unpooled.buffer());AdvancementProgress.STREAM_CODEC.encode(buffer,progress);var bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);buffer.release();var row=new JsonObject();row.add("input",input);row.add("requirements",AdvancementRequirements.CODEC.encodeStart(JsonOps.INSTANCE,requirements).getOrThrow());row.addProperty("wire",HexFormat.of().formatHex(bytes));rows.add(row);
        }
        OUT.add("loaded_progress",rows);
    }
    static void treeMutation() {
        var input=new JsonArray();var holders=new ArrayList<AdvancementHolder>();
        for(String[] entry:List.of(new String[]{"child","root"},new String[]{"orphan","missing"},new String[]{"root",""},new String[]{"grandchild","child"},new String[]{"cycle_a","cycle_b"},new String[]{"cycle_b","cycle_a"})) {
            var definition=json("{\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}").getAsJsonObject();if(!entry[1].isEmpty())definition.addProperty("parent","test:"+entry[1]);var key=id("test:"+entry[0]);holders.add(new AdvancementHolder(key,Advancement.CODEC.parse(ops,definition).getOrThrow()));var row=new JsonObject();row.addProperty("id",key.toString());row.add("value",definition);input.add(row);
        }
        var tree=new AdvancementTree();tree.addAll(holders);var steps=new JsonArray();
        for(String action:List.of("initial","replace_root","remove:test:child","remove:test:unknown","clear")) {
            if(action.equals("replace_root")){var value=json("{\"parent\":\"test:child\",\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}");tree.addAll(List.of(new AdvancementHolder(id("test:root"),Advancement.CODEC.parse(ops,value).getOrThrow())));}
            else if(action.startsWith("remove:"))tree.remove(Set.of(id(action.substring(7))));else if(action.equals("clear"))tree.clear();var row=new JsonObject();row.addProperty("action",action);var nodes=new JsonArray();var children=new JsonObject();for(var node:tree.nodes()){nodes.add(node.holder().id().toString());var list=new JsonArray();for(var child:node.children())list.add(child.holder().id().toString());children.add(node.holder().id().toString(),list);}row.add("nodes",nodes);row.add("children",children);var roots=new JsonObject();for(var node:tree.nodes())roots.addProperty(node.holder().id().toString(),node.root().holder().id().toString());row.add("roots",roots);steps.add(row);
        }
        var output=new JsonObject();output.add("inputs",input);output.add("steps",steps);OUT.add("tree",output);
    }
    static void validation() {
        var rows=new JsonArray();
        for(String trigger:List.of("tick","default_block_use","any_block_use")) {
            String field=trigger.equals("tick")?"player":"location";
            var input=new JsonObject();var criteria=new JsonObject();var criterion=new JsonObject();criterion.addProperty("trigger",trigger);var conditions=new JsonObject();conditions.add(field,json("{\"type\":\"match_tool\"}"));criterion.add("conditions",conditions);criteria.add("a",criterion);input.add("criteria",criteria);
            var advancement=Advancement.CODEC.parse(ops,input).getOrThrow();var reporter=new net.minecraft.util.ProblemReporter.Collector();advancement.validate(reporter,lookup);var problems=new JsonArray();reporter.forEach((path,problem)->{var entry=new JsonObject();entry.addProperty("path",path);entry.addProperty("fatal",problem.isFatal());entry.addProperty("message",problem.description());problems.add(entry);});
            var registry=new MappedRegistry<Advancement>(Registries.ADVANCEMENT,Lifecycle.stable());Registry.register(registry,id("test:warning"),advancement);registry.freeze();
            var replacement=HolderLookup.Provider.create(java.util.stream.Stream.concat(lookup.listRegistries().filter(r->r.key()!=Registries.ADVANCEMENT),java.util.stream.Stream.of(registry)));
            var manager=new ServerAdvancementManager(replacement);var row=new JsonObject();row.add("input",input);row.add("problems",problems);row.addProperty("retained",manager.get(id("test:warning"))!=null);rows.add(row);
        }
        OUT.add("validation",rows);
    }
    static void invalidAdvancements() {
        var cases=new JsonArray();
        for(String raw:List.of(
            "{}", "{\"criteria\":{}}", "{\"criteria\":{\"a\":{\"trigger\":\"missing\"}}}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"requirements\":[]}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"requirements\":[[],[\"a\"]]}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"requirements\":[[\"a\"],[\"missing\"]]}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"requirements\":[[\"a\",\"a\"]]}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"T\",\"description\":\"D\"}}",
            "{\"parent\":\"test:parent\",\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"T\",\"description\":\"D\",\"background\":\"test:bg\"}}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\",\"conditions\":null}},\"rewards\":null}",
            "{\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"display\":null,\"requirements\":null,\"sends_telemetry_event\":null}"
        )) {var input=json(raw);var row=new JsonObject();row.add("input",input);row.addProperty("accepted",Advancement.CODEC.parse(ops,input).isSuccess());cases.add(row);}
        OUT.add("advancement_decode",cases);
    }
    static void playerProgress() throws Exception {
        var definitions=new LinkedHashMap<String,String>();
        definitions.put("test:root","{\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"Root\",\"description\":\"Description\",\"background\":\"test:background\"},\"criteria\":{\"a\":{\"trigger\":\"tick\"},\"b\":{\"trigger\":\"tick\"}},\"requirements\":[[\"a\",\"b\"]],\"rewards\":{\"experience\":3}}");
        definitions.put("test:child","{\"parent\":\"test:root\",\"display\":{\"icon\":{\"id\":\"apple\"},\"title\":\"Child\",\"description\":\"Description\",\"frame\":\"challenge\"},\"criteria\":{\"a\":{\"trigger\":\"tick\"},\"b\":{\"trigger\":\"tick\"}}}");
        definitions.put("test:hidden","{\"parent\":\"test:root\",\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"Hidden\",\"description\":\"Description\",\"hidden\":true},\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}");
        definitions.put("test:grandchild","{\"parent\":\"test:child\",\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"Grandchild\",\"description\":\"Description\"},\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}");
        definitions.put("test:great_grandchild","{\"parent\":\"test:grandchild\",\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"Great grandchild\",\"description\":\"Description\"},\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}");
        definitions.put("test:recipe","{\"criteria\":{\"a\":{\"trigger\":\"recipe_unlocked\",\"conditions\":{\"recipes\":\"crafting_table\"}}}}");
        definitions.put("test:invisible","{\"parent\":\"test:hidden\",\"criteria\":{\"a\":{\"trigger\":\"tick\"}}}");
        var holders=new ArrayList<AdvancementHolder>();var inputs=new JsonArray();
        for(var entry:definitions.entrySet()){var definition=json(entry.getValue()).getAsJsonObject();
            if(entry.getKey().equals("test:root")){definition.getAsJsonObject("rewards").add("recipes",json("[\"crafting_table\"]"));definition.getAsJsonObject("criteria").getAsJsonObject("a").add("conditions",json("{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\"}}}"));}
            var holder=new AdvancementHolder(id(entry.getKey()),Advancement.CODEC.parse(ops,definition).getOrThrow());holders.add(holder);var row=new JsonObject();row.addProperty("id",entry.getKey());row.add("value",definition);inputs.add(row);}
        var tree=new AdvancementTree();tree.addAll(holders);tree.repositionNodes();
        var children=new JsonObject();for(var node:tree.nodes()){var keys=new JsonArray();for(var child:node.children())keys.add(child.holder().id().toString());children.add(node.holder().id().toString(),keys);}
        var manager=mock(ServerAdvancementManager.class);doReturn(holders).when(manager).getAllAdvancements();doReturn(tree).when(manager).tree();doAnswer(call->holders.stream().filter(h->h.id().equals(call.getArgument(0))).findFirst().orElse(null)).when(manager).get(any());
        var rules=new net.minecraft.world.level.gamerules.GameRules(net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);doReturn(rules).when(level).getGameRules();
        var packets=new JsonArray();var effects=new JsonArray();var connection=mock(net.minecraft.server.network.ServerGamePacketListenerImpl.class);player.connection=connection;
        doNothing().when(player).giveExperiencePoints(anyInt());doAnswer(call->{effects.add("xp:"+call.getArgument(0));return null;}).when(player).giveExperiencePoints(anyInt());
        var players=mock(net.minecraft.server.players.PlayerList.class);doAnswer(call->{effects.add(net.minecraft.network.chat.ComponentSerialization.CODEC.encodeStart(ops,(Component)call.getArgument(0)).getOrThrow().toString());return null;}).when(players).broadcastSystemMessage(any(Component.class),eq(false));
        doAnswer(call->{Object packet=call.getArgument(0);if(packet instanceof net.minecraft.network.protocol.game.ClientboundUpdateAdvancementsPacket update)packets.add(wire(update));else if(packet instanceof net.minecraft.network.protocol.game.ClientboundSelectAdvancementsTabPacket tab){var b=new RegistryFriendlyByteBuf(Unpooled.buffer(),lookup);net.minecraft.network.protocol.game.ClientboundSelectAdvancementsTabPacket.STREAM_CODEC.encode(b,tab);var bytes=new byte[b.readableBytes()];b.readBytes(bytes);b.release();packets.add(HexFormat.of().formatHex(bytes));}else throw new AssertionError(packet);return null;}).when(connection).send(any(net.minecraft.network.protocol.Packet.class));
        var fixer=mock(com.mojang.datafixers.DataFixer.class);doAnswer(call->call.getArgument(1)).when(fixer).update(any(),any(),eq(5023),eq(5023));
        var directory=Files.createTempDirectory("ferraloom-advancements");var path=directory.resolve("player.json");var advancement=new PlayerAdvancements(fixer,players,manager,path,player);doReturn(advancement).when(player).getAdvancements();
        var book=new net.minecraft.stats.ServerRecipeBook((key,output)->{});
        doAnswer(call->{@SuppressWarnings("unchecked") var keys=(List<ResourceKey<Recipe<?>>>)call.getArgument(0);var rewards=new ArrayList<RecipeHolder<?>>();for(var key:keys){var recipe=lookup.lookupOrThrow(Registries.RECIPE).get(key);if(recipe.isPresent())rewards.add(new RecipeHolder<>(key,recipe.get().value()));}book.addRecipes(rewards,player);return null;}).when(player).awardRecipesByKey(anyList());
        var steps=new JsonArray();
        for(String action:List.of("flush","award:test:root:a","award:test:root:a","award:test:root:b","revoke:test:root:a","revoke:test:root:b","award:test:child:a","award:test:child:b","revoke:test:child:a","award:test:hidden:a","award:test:invisible:a","tab:test:root","tab:test:child","tab:null","trigger:tick")) {
            var row=new JsonObject();row.addProperty("action",action);boolean changed=false;
            if(action.startsWith("award:")||action.startsWith("revoke:")) {var a=action.split(":");var holder=holders.stream().filter(h->h.id().equals(id(a[1]+":"+a[2]))).findFirst().orElseThrow();changed=action.startsWith("award")?advancement.award(holder,a[3]):advancement.revoke(holder,a[3]);
                if(action.startsWith("award")&&changed){var progress=advancement.getOrStartProgress(holder);var f=AdvancementProgress.class.getDeclaredField("criteria");f.setAccessible(true);@SuppressWarnings("unchecked") var map=(Map<String,CriterionProgress>)f.get(progress);map.put(a[3],new CriterionProgress(Instant.ofEpochMilli(123456789)));}
            } else if(action.equals("trigger:tick")) {CriteriaTriggers.TICK.trigger(player);
                for(var holder:holders){var progress=advancement.getOrStartProgress(holder);var f=AdvancementProgress.class.getDeclaredField("criteria");f.setAccessible(true);@SuppressWarnings("unchecked") var map=(Map<String,CriterionProgress>)f.get(progress);for(var entry:map.entrySet())if(entry.getValue().isDone())entry.setValue(new CriterionProgress(Instant.ofEpochMilli(123456789)));}
            } else if(action.startsWith("tab:")) {var name=action.substring(4);advancement.setSelectedTab(holders.stream().filter(h->h.id().toString().equals(name)).findFirst().orElse(null));}
            for(var holder:holders){var progress=advancement.getOrStartProgress(holder);var f=AdvancementProgress.class.getDeclaredField("criteria");f.setAccessible(true);@SuppressWarnings("unchecked") var map=(Map<String,CriterionProgress>)f.get(progress);for(var entry:map.entrySet())if(entry.getValue().isDone())entry.setValue(new CriterionProgress(Instant.ofEpochMilli(123456789)));}
            advancement.flushDirty(player,true);row.addProperty("changed",changed);
            var listeners=new ArrayList<String>();for(var type:BuiltInRegistries.TRIGGER_TYPES){var map=advancement.getTriggerMapForType(type);if(map!=null)for(var key:map.keySet())listeners.add(BuiltInRegistries.TRIGGER_TYPES.getKey(type)+"/"+key.advancement().id()+"/"+key.criterion());}Collections.sort(listeners);row.add("listeners",JSON.toJsonTree(listeners));
            var known=book.pack().known().stream().map(key->key.identifier().toString()).sorted().toList();row.add("known_recipes",JSON.toJsonTree(known));
            row.add("packets",packets.deepCopy());packets.asList().clear();row.add("effects",effects.deepCopy());effects.asList().clear();advancement.save();row.add("saved",json(Files.readString(path)));steps.add(row);
        }
        var output=new JsonObject();output.add("inputs",inputs);output.add("children",children);output.add("steps",steps);OUT.add("player",output);
        var unknown=json(Files.readString(path)).getAsJsonObject();unknown.add("test:missing",json("{\"criteria\":{\"a\":\"2024-01-01 00:00:00 +0000\"},\"done\":true}"));Files.writeString(path,JSON.toJson(unknown));var restored=new PlayerAdvancements(fixer,players,manager,path,player);restored.flushDirty(player,true);output.add("loaded_packets",packets.deepCopy());restored.save();output.add("loaded_saved",json(Files.readString(path)));output.add("load_input",unknown);
        try(var paths=Files.walk(directory)){for(var file:paths.sorted(Comparator.reverseOrder()).toList())Files.delete(file);}
    }
    static void inlineLoot() {
        var rows=new JsonArray();var sequence=id("test:advancement_reward");
        for(boolean named:new boolean[]{false,true}) {
            String raw="{\"pools\":[{\"rolls\":{\"type\":\"uniform\",\"min\":1,\"max\":4},\"entries\":[{\"type\":\"item\",\"name\":\"stone\"}]}]"+(named?",\"random_sequence\":\"test:advancement_reward\"":"")+"}";
            var table=LootTable.DIRECT_CODEC.parse(ops,json(raw)).getOrThrow();var local=RandomSource.create(42);var stream=new net.minecraft.world.RandomSequences().get(sequence,123);doReturn(local).when(level).getRandom();doReturn(stream).when(server).getRandomSequence(sequence);
            var params=new LootParams(level,ContextMap.EMPTY,Map.of(),0);var items=table.getRandomItems(params);var row=new JsonObject();row.add("input",json(raw));row.addProperty("count",items.size());row.addProperty("level_next",local.nextLong());row.addProperty("sequence_next",stream.nextLong());rows.add(row);
        }
        OUT.add("inline_loot",rows);
    }
    public static void main(String[] args) throws Exception {
        TimeZone.setDefault(TimeZone.getTimeZone("UTC"));setup();builtins();triggers();progress();inlineLoot();playerProgress();invalidAdvancements();validation();treeMutation();loadedProgress();
        try(var output=new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))){output.write(JSON.toJson(OUT).getBytes(java.nio.charset.StandardCharsets.UTF_8));}
        for(var entry:OUT.entrySet())System.out.println(entry.getKey()+": "+(entry.getValue().isJsonArray()?entry.getValue().getAsJsonArray().size():1));
    }
}
