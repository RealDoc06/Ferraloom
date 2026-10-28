import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.functions.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.*;
import net.minecraft.server.permissions.*;

/** Actual CommandFunction#fromLines and Resource#openAsReader's UTF-8 reader. */
public class PlainVectors {
 static final Gson JSON=new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
 record Case(List<String> lines,int permission) {Case(String... lines){this(List.of(lines),2);}}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var dispatcher=new CommandDispatcher<CommandSourceStack>();SayCommand.register(dispatcher);ReturnCommand.register(dispatcher);FunctionCommand.register(dispatcher);
  var cases=List.of(new Case(),new Case("  # comment\t",""," say hello ","return 7"),new Case("say one\\","   two"),new Case("#ignored\\","return 9","return fail"),new Case("say A\\"," \\ ","B"),new Case("say ends\\"),new Case("// comment"),new Case("/say hello"),new Case("/minecraft:say hello"),new Case("/"),new Case("unknown"),new Case("return"),new Case("say"),new Case("return 2147483648"),new Case("return run say nested"),new Case("function test:other"),new Case("\u0000\u000bsay trimmed\u001f"),new Case("\u00a0say not_trimmed"),new Case("\u2003say not_trimmed"),new Case(List.of("return 7"),1),new Case("# only comment"),new Case("say hash # is not a comment"),new Case("say 😀 hello"));
  var rows=new JsonArray();
  for(var test:cases){var row=JSON.toJsonTree(test).getAsJsonObject();try {
    var function=CommandFunction.fromLines(Identifier.parse("test:plain"),dispatcher,Commands.createCompilationContext(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(test.permission))),test.lines);
    var plain=(PlainTextFunction<CommandSourceStack>)function;row.add("entries",JSON.toJsonTree(plain.entries().stream().map(Object::toString).toList()));
   }catch(Exception error){row.addProperty("error",error.getMessage());row.addProperty("error_type",error.getClass().getSimpleName());}rows.add(row);}
  var readers=new JsonArray();
  for(byte[] input:List.of(new byte[0],"a\rb\nc\r\nd\n".getBytes(StandardCharsets.UTF_8),"\n\n".getBytes(StandardCharsets.UTF_8),"say 😀\rreturn 7".getBytes(StandardCharsets.UTF_8),new byte[]{(byte)0xed,(byte)0xa0,(byte)0x80},new byte[]{(byte)0xed,(byte)0xa0},new byte[]{(byte)0xed,(byte)0xa0,65},new byte[]{(byte)0xed,(byte)0x9f,(byte)0xbf},new byte[]{(byte)0xed,(byte)0xbf,(byte)0xbf},new byte[]{(byte)0xee,(byte)0xa0,(byte)0x80},new byte[]{(byte)0xe0,(byte)0x80,(byte)0x80},new byte[]{(byte)0xf0,(byte)0x9f},new byte[]{(byte)0xf4,(byte)0x90,(byte)0x80,(byte)0x80})){
   var row=new JsonObject();row.add("bytes",JSON.toJsonTree(java.util.stream.IntStream.range(0,input.length).map(i->input[i]&255).toArray()));try(var reader=new BufferedReader(new InputStreamReader(new ByteArrayInputStream(input),StandardCharsets.UTF_8))){row.add("lines",JSON.toJsonTree(reader.lines().toList()));}readers.add(row);
  }
  Files.writeString(Path.of(args[0]),JSON.toJson(rows)+"\n");Files.writeString(Path.of(args[1]),JSON.toJson(readers)+"\n");System.out.println("Captured "+rows.size()+" plain compilation and "+readers.size()+" file-reader cases");
 }
}
