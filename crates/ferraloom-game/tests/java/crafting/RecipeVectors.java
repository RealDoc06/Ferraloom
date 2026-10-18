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
    static void record(String label,RecipeType type,RecipeInput input,int width,int height) {
        var test=new JsonObject();test.addProperty("label",label);test.addProperty("type",type.toString());test.addProperty("width",width);test.addProperty("height",height);
        var items=new JsonArray();for(int i=0;i<input.size();i++)items.add(encode(input.getItem(i)));test.add("items",items);
        // CraftingInput is intentionally trimmed by the same public constructor menus use.
        var found=manager.getRecipeFor(type,input,null);
        if(found.isPresent()) {
            RecipeHolder holder=(RecipeHolder)found.get();test.addProperty("recipe",holder.id().identifier().toString());
            test.add("result",encode(holder.value().assemble(input)));
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
    static void generate() throws Exception {
        var ids=new JsonArray();var displays=new JsonArray();
        for(var holder:manager.getRecipes()) {
            ids.add(holder.id().identifier().toString());var recipe=holder.value();String id=holder.id().identifier().toString();
            var displayCase=new JsonObject();displayCase.addProperty("id",id);var list=new JsonArray();for(var display:recipe.display())list.add(wireDisplay(display));displayCase.add("displays",list);displays.add(displayCase);
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
                } else if(recipe instanceof SingleItemRecipe)record(id+"/"+variant,recipe.getType(),new SingleRecipeInput(items.getFirst()),1,1);
                else if(recipe instanceof SmithingRecipe)record(id+"/"+variant,RecipeType.SMITHING,new SmithingRecipeInput(items.get(0),items.get(1),items.get(2)),3,1);
            }
        }
        for(int trial=0;trial<2000;trial++) {int width=1+RNG.nextInt(3),height=1+RNG.nextInt(3);var grid=new ArrayList<ItemStack>();for(int i=0;i<width*height;i++){int id=RNG.nextInt(BuiltInRegistries.ITEM.size());grid.add(RNG.nextInt(3)==0?ItemStack.EMPTY:new ItemStack(BuiltInRegistries.ITEM.byId(id)));}crafting("random/"+trial,width,height,grid);}
        var output=new JsonObject();output.add("ids",ids);output.add("cases",cases);output.add("displays",displays);
        var packet=new ClientboundUpdateRecipesPacket(manager.getSynchronizedItemProperties(),manager.getSynchronizedStonecutterRecipes());
        var buffer=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);ClientboundUpdateRecipesPacket.STREAM_CODEC.encode(buffer,packet);var bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);buffer.release();output.addProperty("update_packet",HexFormat.of().formatHex(bytes));
        // Numeric dynamic ids/tag membership are independently supplied for wire decoding.
        var network=new JsonObject();for(var entry:registries.registries().toList()){var names=new JsonArray();for(var value:entry.value().listElements().toList())names.add(value.key().identifier().toString());network.add(entry.key().identifier().toString(),names);}output.add("registries",network);
        Files.writeString(Path.of(System.getProperty("vectors.output")),JSON.toJson(output)+"\n");System.out.println("Crafting reference: "+ids.size()+" recipes, "+cases.size()+" scenarios");
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
