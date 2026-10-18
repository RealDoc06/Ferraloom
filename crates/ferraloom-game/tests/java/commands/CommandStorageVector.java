import com.google.gson.*;
import com.mojang.datafixers.DataFixer;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.saveddata.SavedData;
import net.minecraft.world.level.storage.*;
import static org.mockito.Mockito.*;

/** Writes real 26.3 SavedDataStorage files and records lazy namespace discovery. */
public final class CommandStorageVector {
    static final JsonArray steps=new JsonArray();
    @SuppressWarnings("unchecked")
    static void record(CommandStorage commands,String op,String id,String input,String result) throws Exception {
        var row=new JsonObject();row.addProperty("op",op);if(id!=null)row.addProperty("id",id);if(input!=null)row.addProperty("input",input);if(result!=null)row.addProperty("result",result);
        row.add("keys",new Gson().toJsonTree(commands.keys().map(Object::toString).toList()));
        var field=CommandStorage.class.getDeclaredField("namespaces");field.setAccessible(true);
        var dirty=new JsonObject();for(var entry:((Map<String,? extends SavedData>)field.get(commands)).entrySet())dirty.addProperty(entry.getKey(),entry.getValue().isDirty());
        row.add("dirty",dirty);steps.add(row);
    }
    static void get(CommandStorage commands,String id) throws Exception {record(commands,"get",id,null,commands.get(Identifier.parse(id)).toString());}
    static void set(CommandStorage commands,String id,String value) throws Exception {commands.set(Identifier.parse(id),TagParser.parseCompoundFully(value));record(commands,"set",id,value,null);}
    static DataFixer fixer() {
        var fixer=mock(DataFixer.class);
        doAnswer(call->{
            int oldVersion=call.getArgument(2),newVersion=call.getArgument(3);
            if(oldVersion!=newVersion)throw new AssertionError("fixture unexpectedly needs data fixing");
            return call.getArgument(1);
        }).when(fixer).update(any(),any(),anyInt(),anyInt());
        return fixer;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        Path output=Path.of(args[0]);Files.createDirectories(output);Path folder=Files.createTempDirectory("ferraloom-native-command-storage");
        var storage=new SavedDataStorage(folder,fixer(),RegistryAccess.EMPTY);
        var commands=new CommandStorage(storage);
        record(commands,"start",null,null,null);get(commands,"missing:key");set(commands,"test:a","{answer:42,nested:{message:'hello'}}");
        set(commands,"other:root","{values:[1b,2L,'three']}");set(commands,"test:an","{collision:1}");set(commands,"test:c0","{collision:2}");get(commands,"test:a");
        set(commands,"test:a","{}");set(commands,"empty:absent","{}");storage.saveAndJoin();record(commands,"save",null,null,null);
        Path file=folder.resolve("test/command_storage.dat");Files.copy(file,output.resolve("native-command-storage.dat"),StandardCopyOption.REPLACE_EXISTING);
        var saved=NbtIo.readCompressed(file,NbtAccounter.unlimitedHeap());
        Files.writeString(output.resolve("native-command-storage.snbt"),saved.toString()+"\n");
        storage.close();
        storage=new SavedDataStorage(folder,fixer(),RegistryAccess.EMPTY);commands=new CommandStorage(storage);
        record(commands,"reload",null,null,null);get(commands,"missing:key");get(commands,"test:absent");get(commands,"other:root");get(commands,"test:an");set(commands,"test:c0","{collision:2}");
        storage.saveAndJoin();record(commands,"save",null,null,null);storage.close();
        Files.writeString(output.resolve("command-storage-vectors.json"),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(steps)+"\n");
        try(var paths=Files.walk(folder)){for(Path path:paths.sorted(Comparator.reverseOrder()).toList())Files.delete(path);}
        net.minecraft.util.Util.shutdownExecutors();
    }
}
