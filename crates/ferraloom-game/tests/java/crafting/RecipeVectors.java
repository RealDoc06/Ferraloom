import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.ClientboundUpdateRecipesPacket;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.item.crafting.display.*;

/** Real resource-registry loading and RecipeManager calls. No copied recipe code. */
public final class RecipeVectors {
    static final Gson JSON=new GsonBuilder().disableHtmlEscaping().create();
    static final Random RNG=new Random(263777L);
    static RegistryAccess.Frozen registries;
    static RegistryOps<JsonElement> ops;
    static RecipeManager manager;
    static final JsonArray cases=new JsonArray();
    static ItemStack choose(Ingredient ingredient,int variant) {
        var items=ingredient.items().toList();
        if(items.isEmpty())return ItemStack.EMPTY;
        var stack=new ItemStack(items.get(Math.floorMod(variant,items.size())));
        if(variant!=0) { stack.setCount(1+RNG.nextInt(stack.getMaxStackSize())); stack.set(DataComponents.CUSTOM_NAME,Component.literal("recipe component probe")); }
        return stack;
    }
    static JsonElement encode(ItemStack stack) {return ItemStack.OPTIONAL_CODEC.encodeStart(ops,stack).getOrThrow();}
    @SuppressWarnings({"rawtypes","unchecked"})
    static void record(String label,RecipeType type,RecipeInput input,int width,int height) { record(label,type,input,width,height,null); }
    @SuppressWarnings({"rawtypes","unchecked"})
    static void record(String label,RecipeType type,RecipeInput input,int width,int height,net.minecraft.world.level.Level level) {
        var test=new JsonObject();test.addProperty("label",label);test.addProperty("type",type.toString());test.addProperty("width",width);test.addProperty("height",height);
        var items=new JsonArray();for(int i=0;i<input.size();i++)items.add(encode(input.getItem(i)));test.add("items",items);
        // CraftingInput is intentionally trimmed by the same public constructor menus use.
        var found=manager.getRecipeFor(type,input,level);
        if(found.isPresent()) {
            RecipeHolder holder=(RecipeHolder)found.get();test.addProperty("recipe",holder.id().identifier().toString());
            var assembled=holder.value().assemble(input);test.add("result",encode(assembled));var mapProcessing=assembled.get(DataComponents.MAP_POST_PROCESSING);if(mapProcessing!=null)test.addProperty("map_post_processing",mapProcessing.name());
            if(holder.value() instanceof CraftingRecipe crafting) {var remaining=new JsonArray();for(var stack:crafting.getRemainingItems((CraftingInput)input))remaining.add(encode(stack));test.add("remaining",remaining);}
        } else test.add("recipe",JsonNull.INSTANCE);
        cases.add(test);
    }
    static void crafting(String label,int width,int height,List<ItemStack> grid) {
        var input=CraftingInput.ofPositioned(width,height,grid);
        int before=cases.size();record(label,RecipeType.CRAFTING,input.input(),width,height);
        var test=cases.get(before).getAsJsonObject();var full=new JsonArray();for(var stack:grid)full.add(encode(stack));test.add("items",full);test.addProperty("left",input.left());test.addProperty("top",input.top());
    }
    static Ingredient ingredient(Recipe<?> recipe,String field) throws Exception {var f=recipe.getClass().getDeclaredField(field);f.setAccessible(true);return (Ingredient)f.get(recipe);}
    static List<ItemStack> seed(Recipe<?> recipe,int variant) throws Exception {
        var placement=recipe.placementInfo();var grid=new ArrayList<ItemStack>();
        for(int index:placement.slotsToIngredientIndex())grid.add(index<0?ItemStack.EMPTY:choose(placement.ingredients().get(index),variant));
        if(!grid.isEmpty())return grid;
        if(recipe instanceof BookCloningRecipe) {
            var source=choose(ingredient(recipe,"source"),variant);
            source.set(DataComponents.WRITTEN_BOOK_CONTENT,new net.minecraft.world.item.component.WrittenBookContent(net.minecraft.server.network.Filterable.passThrough("test"),"Doc",variant%3,List.of(),false));
            return List.of(source,choose(ingredient(recipe,"material"),variant));
        }
        if(recipe instanceof BannerDuplicateRecipe) {
            var source=choose(ingredient(recipe,"banner"),variant);
            var pattern=registries.lookupOrThrow(Registries.BANNER_PATTERN).listElements().findFirst().orElseThrow();
            source.set(DataComponents.BANNER_PATTERNS,new net.minecraft.world.level.block.entity.BannerPatternLayers(List.of(new net.minecraft.world.level.block.entity.BannerPatternLayers.Layer(pattern,DyeColor.RED))));
            return List.of(source,choose(ingredient(recipe,"banner"),0));
        }
        if(recipe instanceof ShieldDecorationRecipe)return List.of(choose(ingredient(recipe,"banner"),variant),choose(ingredient(recipe,"target"),variant));
        if(recipe instanceof FireworkRocketRecipe)return List.of(choose(ingredient(recipe,"shell"),variant),choose(ingredient(recipe,"fuel"),variant),choose(ingredient(recipe,"star"),variant));
        if(recipe instanceof FireworkStarRecipe)return List.of(choose(ingredient(recipe,"fuel"),variant),choose(ingredient(recipe,"dye"),variant));
        if(recipe instanceof FireworkStarFadeRecipe)return List.of(choose(ingredient(recipe,"target"),variant),choose(ingredient(recipe,"dye"),variant));
        if(recipe instanceof DecoratedPotRecipe)return List.of(ItemStack.EMPTY,choose(ingredient(recipe,"backPattern"),variant),ItemStack.EMPTY,choose(ingredient(recipe,"leftPattern"),variant),ItemStack.EMPTY,choose(ingredient(recipe,"rightPattern"),variant),ItemStack.EMPTY,choose(ingredient(recipe,"frontPattern"),variant),ItemStack.EMPTY);
        if(recipe instanceof MapExtendingRecipe)return List.of(new ItemStack(Items.PAPER),new ItemStack(Items.FILLED_MAP));
        if(recipe instanceof RepairItemRecipe)return List.of(new ItemStack(Items.DIAMOND_PICKAXE),new ItemStack(Items.DIAMOND_PICKAXE));
        return List.of();
    }
    static String wireDisplay(RecipeDisplay display) {
        var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);RecipeDisplay.STREAM_CODEC.encode(buf,display);var bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);buf.release();return HexFormat.of().formatHex(bytes);
    }
    @SuppressWarnings({"rawtypes","unchecked"})
    static JsonArray placementVectors() throws Exception {
        var tests=new JsonArray();
        var selected=manager.getRecipes().stream().filter(h->List.of("crafting_table","stick","oak_planks","furnace").contains(h.id().identifier().getPath()) || h.value() instanceof SmeltingRecipe && h.id().identifier().getPath().contains("iron_ingot")).toList();
        for(var holder:selected) {
            var recipe=holder.value();int width=recipe instanceof CraftingRecipe?3:1;int height=width;
            var grid=new net.minecraft.world.SimpleContainer(width*height);
            var inventory=new net.minecraft.world.entity.player.Inventory(null,new net.minecraft.world.entity.EntityEquipment());
            int next=0;for(var ingredient:recipe.placementInfo().ingredients()) {var stack=choose(ingredient,0);stack.setCount(12);inventory.setItem(next++,stack);}
            var slots=new ArrayList<net.minecraft.world.inventory.Slot>();for(int i=0;i<width*height;i++)slots.add(new net.minecraft.world.inventory.Slot(grid,i,0,0));
            var access=new net.minecraft.recipebook.ServerPlaceRecipe.CraftingMenuAccess() {
                public void fillCraftSlotsStackedContents(net.minecraft.world.entity.player.StackedItemContents contents){grid.fillStackedContents(contents);}
                public void clearCraftingContent(){grid.clearContent();}
                public boolean recipeMatches(RecipeHolder candidate){return candidate.value().matches(width==1?new SingleRecipeInput(grid.getItem(0)):CraftingInput.of(width,height,IntStream.range(0,width*height).mapToObj(grid::getItem).toList()),null);}
            };
            for(int step=0;step<3;step++) {
                var test=new JsonObject();test.addProperty("recipe",holder.id().identifier().toString());test.addProperty("width",width);test.addProperty("height",height);test.addProperty("max",step==2);
                var before=new JsonArray();for(var stack:inventory.getNonEquipmentItems())before.add(encode(stack));test.add("inventory",before);
                before=new JsonArray();for(int i=0;i<width*height;i++)before.add(encode(grid.getItem(i)));test.add("grid",before);
                var action=net.minecraft.recipebook.ServerPlaceRecipe.placeRecipe(access,width,height,slots,slots,inventory,(RecipeHolder)holder,step==2,false);test.addProperty("action",action.name());
                var after=new JsonArray();for(var stack:inventory.getNonEquipmentItems())after.add(encode(stack));test.add("after_inventory",after);
                after=new JsonArray();for(int i=0;i<width*height;i++)after.add(encode(grid.getItem(i)));test.add("after_grid",after);tests.add(test);
            }
        }
        return tests;
    }
    static void mapEdges() throws Exception {
        var level=org.mockito.Mockito.mock(net.minecraft.world.level.Level.class);
        for(int scale:new int[]{0,3,4})org.mockito.Mockito.when(level.getMapData(new net.minecraft.world.level.saveddata.maps.MapId(scale))).thenReturn(net.minecraft.world.level.saveddata.maps.MapItemSavedData.createFresh(0,0,(byte)scale,false,false,net.minecraft.world.level.Level.OVERWORLD));
        for(var holder:manager.getRecipes())if(holder.value() instanceof MapExtendingRecipe recipe){
            for(int id:new int[]{0,3,4,99}){
                var material=ingredient(recipe,"material");var map=choose(ingredient(recipe,"map"),0);map.set(DataComponents.MAP_ID,new net.minecraft.world.level.saveddata.maps.MapId(id));
                var grid=new ArrayList<ItemStack>();for(int i=0;i<9;i++)grid.add(i==4?map:choose(material,0));
                int index=cases.size();record(holder.id().identifier()+"/saved-map-"+id,RecipeType.CRAFTING,CraftingInput.of(3,3,grid),3,3,level);var test=cases.get(index).getAsJsonObject();test.addProperty("left",0);test.addProperty("top",0);
            }
        }
    }
    static void specialEdges() throws Exception {
        for(var holder:manager.getRecipes()) {
            var recipe=holder.value();String id=holder.id().identifier().toString();
            if(recipe instanceof CraftingRecipe && (recipe.isSpecial() || recipe instanceof DyeRecipe || recipe instanceof ImbueRecipe)) {
                var seed=seed(recipe,0);if(seed.isEmpty())continue;var grid=new ArrayList<ItemStack>();for(var stack:seed)grid.add(stack.copy());while(grid.size()<9)grid.add(ItemStack.EMPTY);
                for(int variant=0;variant<4;variant++) {
                    var changed=new ArrayList<ItemStack>();for(var stack:grid)changed.add(stack.copy());
                    if(recipe instanceof RepairItemRecipe) {
                        var first=changed.get(0);var second=changed.get(1);first.setDamageValue(1000);second.setDamageValue(1000);second.set(DataComponents.MAX_DAMAGE,2000);
                        if(variant==1)first.setCount(2);
                        if(variant==2)first.remove(DataComponents.DAMAGE);
                        if(variant==3){var enchant=registries.lookupOrThrow(Registries.ENCHANTMENT).listElements().filter(h->h.is(net.minecraft.tags.EnchantmentTags.CURSE)).findFirst().orElseThrow();first.enchant(enchant,1);}
                    } else if(recipe instanceof BannerDuplicateRecipe) {
                        var source=changed.get(0);var layer=source.get(DataComponents.BANNER_PATTERNS).layers().getFirst();source.set(DataComponents.BANNER_PATTERNS,new net.minecraft.world.level.block.entity.BannerPatternLayers(Collections.nCopies(variant==0?6:variant==1?7:0,layer)));
                        if(variant==3){changed.set(1,new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace("white_banner"))));}
                    } else if(recipe instanceof ShieldDecorationRecipe && variant>0) {
                        var pattern=registries.lookupOrThrow(Registries.BANNER_PATTERN).listElements().findFirst().orElseThrow();changed.get(1).set(DataComponents.BANNER_PATTERNS,new net.minecraft.world.level.block.entity.BannerPatternLayers(List.of(new net.minecraft.world.level.block.entity.BannerPatternLayers.Layer(pattern,DyeColor.RED))));
                    } else if(recipe instanceof DyeRecipe) {
                        changed.get(0).set(DataComponents.DYED_COLOR,new net.minecraft.world.item.component.DyedItemColor(0x133755));
                        if(variant==0){changed.set(2,new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace("red_dye"))));changed.set(3,new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace("blue_dye"))));}
                        if(variant==1)changed.get(1).remove(DataComponents.DYE);
                        if(variant==2)changed.set(2,changed.get(0).copy());
                        if(variant==3)changed.get(0).set(DataComponents.CUSTOM_NAME,Component.literal("retained"));
                    } else if(recipe instanceof ImbueRecipe){
                        var potion=registries.lookupOrThrow(Registries.POTION).listElements().filter(h->h.key().identifier().getPath().equals("swiftness")).findFirst().orElseThrow();changed.get(4).set(DataComponents.POTION_CONTENTS,new net.minecraft.world.item.alchemy.PotionContents(potion));if(variant==1)changed.get(4).remove(DataComponents.POTION_CONTENTS);if(variant==2)changed.set(0,ItemStack.EMPTY);
                    } else if(recipe instanceof FireworkRocketRecipe){
                        var fuel=choose(ingredient(recipe,"fuel"),0);if(variant>0){changed.set(3,fuel.copy());changed.set(4,fuel.copy());}if(variant==2)changed.set(5,fuel.copy());if(variant==3){changed.get(2).set(DataComponents.FIREWORK_EXPLOSION,new net.minecraft.world.item.component.FireworkExplosion(net.minecraft.world.item.component.FireworkExplosion.Shape.STAR,new it.unimi.dsi.fastutil.ints.IntArrayList(new int[]{1,2}),new it.unimi.dsi.fastutil.ints.IntArrayList(),true,true));}
                    } else if(recipe instanceof FireworkStarRecipe || recipe instanceof FireworkStarFadeRecipe) {
                        if(variant==1)changed.get(1).remove(DataComponents.DYE);if(variant==2)changed.set(2,new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace("red_dye"))));if(variant==3)changed.set(3,changed.get(0).copy());
                    } else {continue;}
                    crafting(id+"/edge"+variant,3,3,changed);
                }
            }
            if(recipe instanceof SmithingTrimRecipe) {
                var items=seed(recipe,0);var input=new SmithingRecipeInput(items.get(0),items.get(1),items.get(2));var result=((SmithingRecipe)recipe).assemble(input);if(!result.isEmpty())record(id+"/already_trimmed",RecipeType.SMITHING,new SmithingRecipeInput(items.get(0),result,items.get(2)),3,1);
            }
        }
    }
    static void generate() throws Exception {
        var ids=new JsonArray();var displays=new JsonArray();
        for(var holder:manager.getRecipes()) {
            ids.add(holder.id().identifier().toString());var recipe=holder.value();String id=holder.id().identifier().toString();
            var displayCase=new JsonObject();displayCase.addProperty("id",id);var list=new JsonArray();for(var display:recipe.display())list.add(wireDisplay(display));displayCase.add("displays",list);
            var placementCases=new JsonArray();
            for(int size=1;size<=3;size++){var placed=new JsonArray();net.minecraft.recipebook.PlaceRecipeHelper.placeRecipe(size,size,recipe,recipe.placementInfo().slotsToIngredientIndex(),(ingredient,index,x,y)->{var pair=new JsonArray();pair.add(index);pair.add(ingredient);placed.add(pair);});placementCases.add(placed);}
            displayCase.add("placements",placementCases);
            var entries=new JsonArray();manager.listDisplaysForRecipe(holder.id(),entry->{var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);RecipeDisplayEntry.STREAM_CODEC.encode(buf,entry);var bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);buf.release();entries.add(HexFormat.of().formatHex(bytes));});displayCase.add("entries",entries);displays.add(displayCase);
            for(int variant=0;variant<3;variant++) {
                if(recipe instanceof BrewingRecipe brewing) {var input=choose(brewing.getInput().ingredient(),variant);brewing.getInput().potions().flatMap(p->p.potions()).ifPresent(p->p.stream().findFirst().ifPresent(potion->input.set(DataComponents.POTION_CONTENTS,new net.minecraft.world.item.alchemy.PotionContents(potion))));record(id+"/"+variant,RecipeType.BREWING,new BrewingInput(input,choose(brewing.getReagent().ingredient(),variant)),2,1);continue;}
                var items=seed(recipe,variant);
                if(recipe instanceof CraftingRecipe) {
                    int width=recipe instanceof ShapedRecipe shaped?shaped.getWidth():recipe instanceof ImbueRecipe||recipe instanceof DecoratedPotRecipe?3:Math.min(3,items.size());
                    int height=width==0?0:(items.size()+width-1)/width;
                    var grid=new ArrayList<ItemStack>(items);while(grid.size()<width*height)grid.add(ItemStack.EMPTY);
                    if(variant==1) {var mirrored=new ArrayList<ItemStack>();for(int y=0;y<height;y++)for(int x=0;x<width;x++)mirrored.add(grid.get(width-1-x+y*width));grid=mirrored;}
                    if(variant==2) {var padded=new ArrayList<ItemStack>(Collections.nCopies((width+2)*(height+2),ItemStack.EMPTY));for(int y=0;y<height;y++)for(int x=0;x<width;x++)padded.set(x+1+(y+1)*(width+2),grid.get(x+y*width));width+=2;height+=2;grid=padded;}
                    crafting(id+"/"+variant,width,height,grid);
                    if(variant==0 && !grid.isEmpty()) {
                        var missing=new ArrayList<>(grid);int removed=RNG.nextInt(missing.size());missing.set(removed,ItemStack.EMPTY);crafting(id+"/missing",width,height,missing);
                        var changed=new ArrayList<>(grid);changed.set(RNG.nextInt(changed.size()),new ItemStack(Items.BARRIER));crafting(id+"/extra",width,height,changed);
                    }
                } else if(recipe instanceof SingleItemRecipe)record(id+"/"+variant,recipe.getType(),new SingleRecipeInput(items.getFirst()),1,1);
                else if(recipe instanceof SmithingRecipe)record(id+"/"+variant,RecipeType.SMITHING,new SmithingRecipeInput(items.get(0),items.get(1),items.get(2)),3,1);
            }
        }
        for(int trial=0;trial<2000;trial++) {int width=1+RNG.nextInt(3),height=1+RNG.nextInt(3);var grid=new ArrayList<ItemStack>();for(int i=0;i<width*height;i++){int id=RNG.nextInt(BuiltInRegistries.ITEM.size());grid.add(RNG.nextInt(3)==0?ItemStack.EMPTY:new ItemStack(BuiltInRegistries.ITEM.byId(id)));}crafting("random/"+trial,width,height,grid);}
        specialEdges();mapEdges();
        var codecCases=new JsonArray();
        for(String raw:List.of(
            "{type:'minecraft:smelting',ingredient:'minecraft:stone',result:'minecraft:diamond',cookingtime:4294967296}",
            "{type:'minecraft:smelting',ingredient:'minecraft:stone',result:'minecraft:diamond',cookingtime:true}",
            "{type:'minecraft:smelting',ingredient:'minecraft:stone',result:'minecraft:diamond',cookingtime:3.9}",
            "{type:'minecraft:smelting',ingredient:'minecraft:stone',result:{id:'minecraft:diamond',count:2.9},cookingtime:2}",
            "{type:'minecraft:smelting',ingredient:'minecraft:stone',result:'minecraft:diamond',cookingtime:2,show_notification:1}",
            "{type:'minecraft:smelting',ingredient:'minecraft:stone',result:'minecraft:diamond',cookingtime:2,experience:true}",
            "{type:'minecraft:crafting_shaped',pattern:['  '],key:{},result:'minecraft:diamond'}",
            "{type:'minecraft:crafting_shapeless',ingredients:[],result:'minecraft:diamond'}",
            "{type:'minecraft:crafting_shapeless',ingredients:['minecraft:air'],result:'minecraft:diamond'}",
            "{type:'minecraft:crafting_shapeless',ingredients:['minecraft:stone'],result:{id:'minecraft:diamond',count:0}}",
            "{type:'minecraft:crafting_transmute',input:'minecraft:stone',material:'minecraft:dirt',material_count:{},result:{}}",
            "{type:'minecraft:crafting_transmute',input:'minecraft:stone',material:'minecraft:dirt',result:{id:'minecraft:air'}}"
        )) {
            var recipeJson=JsonParser.parseString(raw);var test=new JsonObject();test.add("input",recipeJson);
            try { var decoded=Recipe.DIRECT_CODEC.parse(ops,recipeJson);test.addProperty("accepted",decoded.isSuccess());if(decoded.isSuccess() && decoded.getOrThrow() instanceof AbstractCookingRecipe cooking) {test.addProperty("cookingtime",cooking.cookingTime());test.addProperty("experience",cooking.experience());test.add("assembled",encode(cooking.assemble(new SingleRecipeInput(new ItemStack(Items.STONE)))));} }
            catch(RuntimeException failure){test.addProperty("accepted",false);}
            codecCases.add(test);
        }
        var output=new JsonObject();var resourceOrder=new JsonArray();List.of("recipe/a.json","recipe/a-b.json","recipe/a.c.json","recipe/a/a.json").stream().map(Identifier::withDefaultNamespace).sorted().forEach(id->resourceOrder.add(id.getPath()));output.add("resource_order",resourceOrder);output.add("placement_cases",placementVectors());var curses=new JsonArray();registries.lookupOrThrow(Registries.ENCHANTMENT).listElements().filter(h->h.is(net.minecraft.tags.EnchantmentTags.CURSE)).forEach(h->curses.add(h.key().identifier().toString()));output.add("curses",curses);output.add("codec_cases",codecCases);output.add("ids",ids);output.add("cases",cases);output.add("displays",displays);
        var packet=new ClientboundUpdateRecipesPacket(manager.getSynchronizedItemProperties(),manager.getSynchronizedStonecutterRecipes());
        var buffer=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);ClientboundUpdateRecipesPacket.STREAM_CODEC.encode(buffer,packet);var bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);buffer.release();output.addProperty("update_packet",HexFormat.of().formatHex(bytes));
        // Numeric dynamic ids/tag membership are independently supplied for wire decoding.
        var network=new JsonObject();for(var entry:registries.registries().toList()){var names=new JsonArray();for(var value:entry.value().listElements().toList())names.add(value.key().identifier().toString());network.add(entry.key().identifier().toString(),names);}output.add("registries",network);
        try(var out=new java.util.zip.GZIPOutputStream(Files.newOutputStream(Path.of(System.getProperty("vectors.output"))))) { out.write((JSON.toJson(output)+"\n").getBytes(java.nio.charset.StandardCharsets.UTF_8)); }System.out.println("Crafting reference: "+ids.size()+" recipes, "+cases.size()+" scenarios");
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        try(var resources=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack))) {
            var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);TagLoader.loadTagsForExistingRegistries(resources,builtins).forEach(Registry.PendingTags::apply);
            var world=RegistryDataLoader.load(resources,builtins.listRegistries().toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
            var context=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),world.listRegistries()));BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(pending->pending.apply());
            var recipes=RegistryDataLoader.load(resources,context.listRegistries().toList(),List.of(RegistryDataLoader.RELOADABLE_REGISTRIES.stream().filter(r->r.key()==Registries.RECIPE).findFirst().orElseThrow()),Runnable::run).join();
            registries=new RegistryAccess.ImmutableRegistryAccess(Stream.of(builtins,world,recipes).flatMap(RegistryAccess::registries)).freeze();ops=registries.createSerializationContext(JsonOps.INSTANCE);manager=new RecipeManager(registries);manager.finalizeRecipeLoading(FeatureFlags.DEFAULT_FLAGS);generate();
        }
    }
}
