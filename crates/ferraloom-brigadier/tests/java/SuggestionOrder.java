import com.mojang.brigadier.context.StringRange;
import com.mojang.brigadier.suggestion.*;
import java.nio.file.*;
import java.util.*;
import java.util.stream.Collectors;

/** JDK bucket traversal precedes Brigadier's stable case-insensitive sort. */
public final class SuggestionOrder {
    static Suggestion suggestion(StringRange range,String token) {return token.startsWith("i:")?new IntegerSuggestion(range,Integer.parseInt(token.substring(2))):new Suggestion(range,token);}
    static String token(Suggestion value) {return value instanceof IntegerSuggestion integer?"i:"+integer.getValue():value.getText();}
    public static void main(String[] args) throws Exception {
        var out=new ArrayList<String>();
        for(int end:List.of(0,1,2,3,4,9,17,31)) for(var tokens:List.of(
            List.of("e","E","f","F","d","D"),
            List.of("b","B","s","S","i","I","l","L","u","U","x","X"),
            List.of(".","e","E","f","F","d","D","b","B","s","S","i","I","l","L","u","U","b"),
            List.of("a","A","aa","AA","Aa","aA","ä","Ä","Ω","ω","🦀","z","Z"),
            List.of("i:11","11","i:2","2","i:-1","a","A","i:0"))) {
            var range=StringRange.at(end);var input="x".repeat(end);var suggestions=tokens.stream().map(value->suggestion(range,value)).toList();
            var hashes=suggestions.stream().map(value->Integer.toString(value.hashCode())).collect(Collectors.joining(","));
            var sorted=Suggestions.create(input,suggestions).getList().stream().map(SuggestionOrder::token).collect(Collectors.joining(";"));
            out.add(end+"\t"+String.join(";",tokens)+"\t"+hashes+"\t"+sorted);
        }
        Files.writeString(Path.of(args[0]),String.join("\n",out)+"\n");
    }
}
