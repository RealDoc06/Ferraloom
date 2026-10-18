import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.stats.ServerRecipeBook;

/** Packed.CODEC and loadUntrusted observations, with identity-set output sorted. */
public final class RecipeBookVectors {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        var cases = new JsonArray();
        for (String raw : List.of(
            "{}", "{recipes:[]}", "{toBeDisplayed:[]}", "{recipes:[],toBeDisplayed:[]}",
            "{recipes:[B;],toBeDisplayed:[I;]}", "{recipes:[L;],toBeDisplayed:[]}",
            "{recipes:[B;1b],toBeDisplayed:[]}", "{recipes:[':bad'],toBeDisplayed:[]}",
            "{recipes:['..:unknown'],toBeDisplayed:[]}", "{recipes:['minecraft:bad id'],toBeDisplayed:[]}",
            "{recipes:[],toBeDisplayed:[],isGuiOpen:'true'}",
            "{recipes:[],toBeDisplayed:[],isGuiOpen:256,isFilteringCraftable:0.5d}",
            "{recipes:[],toBeDisplayed:[],isGuiOpen:0.0d,isFurnaceGuiOpen:-0.5f}",
            "{recipes:['minecraft:oak_planks','minecraft:missing','minecraft:oak_planks'],toBeDisplayed:['minecraft:crafting_table']}",
            "{recipes:[],toBeDisplayed:['minecraft:oak_planks'],isBlastingFurnaceGuiOpen:1b,isSmokerFilteringCraftable:1b}"
        )) {
            var row = new JsonObject(); row.addProperty("input",raw);
            var decoded = ServerRecipeBook.Packed.CODEC.parse(NbtOps.INSTANCE,TagParser.parseCompoundFully(raw));
            row.addProperty("accepted",decoded.isSuccess());
            if (decoded.isSuccess()) {
                var book = new ServerRecipeBook((id,output)->{});
                book.loadUntrusted(decoded.getOrThrow(), id -> Set.of("minecraft:oak_planks","minecraft:crafting_table").contains(id.identifier().toString()));
                var packed=book.pack();
                var known=new ArrayList<>(packed.known());known.sort(Comparator.comparing(id->id.identifier().toString()));
                var highlight=new ArrayList<>(packed.highlight());highlight.sort(Comparator.comparing(id->id.identifier().toString()));
                row.addProperty("expected",ServerRecipeBook.Packed.CODEC.encodeStart(NbtOps.INSTANCE,new ServerRecipeBook.Packed(packed.settings(),known,highlight)).getOrThrow().toString());
            }
            cases.add(row);
        }
        Files.writeString(Path.of(args[0]),new Gson().toJson(cases)+"\n");
        System.out.println("Recipe book vectors: "+cases.size());
    }
}
