import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.server.Bootstrap;

/** Executes the native rich visitor used by every DataAccessor feedback component. */
public final class DataTextVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var inputs=new ArrayList<String>(List.of(
            "0b","-128b","32767s","-2147483648","9223372036854775807L",
            "1.5f","-0.0f","1.0E-10f","1.25d","-0.0d","1.0E30d",
            "''","'quote\" double'","\"quote' single\"","'line\\ncontrol\\x00'",
            "{}","{Aa:1,BB:2,a:3,'a b':4,'1':5,'a:b':6}",
            "[]","[1,2,3]","['a']","['a','b','c','d','e','f','g','h']",
            "[{a:1},{b:[3,4]}]","[B;]","[B;-128b,0b,127b]","[I;-1,0,2147483647]","[L;-9223372036854775808L,0L]"
        ));
        for(String type:List.of("B","I","L"))for(int count:List.of(128,129)) {
            var values=new ArrayList<String>();for(int i=0;i<count;i++)values.add((i%100)+(type.equals("I")?"":type));
            inputs.add("["+type+";"+String.join(",",values)+"]");
        }
        for(int depth:List.of(63,64,65)) {
            inputs.add("[".repeat(depth)+"1"+"]".repeat(depth));
            inputs.add("{a:".repeat(depth)+"1"+"}".repeat(depth));
        }
        var rows=new JsonArray();
        for(int index=-1;index<inputs.size();index++) {
            String text=index==-1?null:inputs.get(index);
            Tag tag=index==-1?EndTag.INSTANCE:TagParser.create(NbtOps.INSTANCE).parseFully(text);
            var row=new JsonObject();row.add("input",text==null?JsonNull.INSTANCE:new JsonPrimitive(text));
            row.add("component",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,NbtUtils.toPrettyComponent(tag)).getOrThrow());rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().serializeNulls().create().toJson(rows)+"\n");
    }
}
