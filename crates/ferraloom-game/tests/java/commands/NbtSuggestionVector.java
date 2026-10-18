import com.google.gson.*;
import com.mojang.brigadier.*;
import com.mojang.brigadier.context.*;
import com.mojang.brigadier.suggestion.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;

/** Captures SnbtGrammar's tied longest-error suggestions, even after a successful parse. */
public final class NbtSuggestionVector {
    static JsonObject capture(String input,Suggestions suggestions) {
        var vector=new JsonObject();vector.addProperty("kind","nbt");vector.addProperty("input",input);vector.addProperty("level",4);
        vector.addProperty("start",suggestions.getRange().getStart());vector.addProperty("end",suggestions.getRange().getEnd());
        var entries=new JsonArray();suggestions.getList().forEach(s->{var entry=new JsonObject();entry.addProperty("text",s.getText());entries.add(entry);});vector.add("suggestions",entries);return vector;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var vectors=new JsonArray();var dispatcher=new CommandDispatcher<Object>();
        var nbt=net.minecraft.commands.arguments.NbtTagArgument.nbtTag();
        for(var input:List.of("", "a", "true", "uuid", "bool", "0", "1", "1b", "1l", "1.0", "1e", "1e+", "0x", "0b", "0xF", "-", "+", "{", "{a", "{a:", "{a:1", "{a:1,", "{a:1}", "{a:[", "{a:[]", "{a:\"x", "{a:\"x\"", "{a:\"x\",", "[", "[B", "[L", "[I", "[B;", "[B;1", "[B;1b", "[B;1b,", "[B;1b]", "[1", "[1,", "[1]", "[true", "[true,", "\"", "\"abc", "'abc", "\"abc\"", "'abc'", "\"\\", "\"\\q", "\"\\N", "\"\\N{", "\"\\N{LATIN CAPITAL LETTER A", "\"\\x", "\"\\x0", "\"\\x00", "\"\\u0000", "bool(", "bool(1", "bool(1,", "bool(1)", "uuid(", " { a : 1 , ")) {
            var context=new CommandContextBuilder<>(dispatcher,new Object(),dispatcher.getRoot(),0).build(input);
            vectors.add(capture(input,nbt.listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
