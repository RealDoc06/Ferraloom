import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.*;
import net.minecraft.server.commands.data.DataCommands;
import net.minecraft.server.permissions.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.saveddata.*;
import net.minecraft.world.level.storage.*;
import static org.mockito.Mockito.*;

/** Executes actual registered DataCommands against the actual CommandStorage aliasing semantics. */
public final class DataCommandVector {
    static final String INITIAL="{n:-1.5d,i:7,text:'A😀BC',list:[1,2,3],objects:[{a:1},{a:2}],mixed:[{a:1},2],nested:{Aa:1,BB:2},bytes:[B;1b,2b],ints:[I;3,4],longs:[L;5L,6L],empty:[],flag:1b}";
    static final String SOURCE="{n:2.5f,text:'W😀XYZ',values:[4,5],objects:[{b:3},{c:4}],mixed:[{b:3},2],flag:1b}";
    static String safeSnbt(String text) {
        var result=new StringBuilder();
        for(int i=0;i<text.length();i++) {
            char value=text.charAt(i);
            if(Character.isHighSurrogate(value)&&i+1<text.length()&&Character.isLowSurrogate(text.charAt(i+1))) {
                result.append(value).append(text.charAt(++i));
            } else if(Character.isSurrogate(value)) result.append(String.format(Locale.ROOT,"\\u%04x",(int)value));
            else result.append(value);
        }
        return result.toString();
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        com.mojang.brigadier.exceptions.CommandSyntaxException.BUILT_IN_EXCEPTIONS=new BrigadierExceptions();
        var context=CommandBuildContext.simple(net.minecraft.data.registries.VanillaRegistries.createWorldLookup(),FeatureFlags.DEFAULT_FLAGS);
        var dispatcher=new CommandDispatcher<CommandSourceStack>();DataCommands.register(dispatcher,context);
        var commands=new ArrayList<String>();
        commands.add("data get storage test:root");
        for(String path:List.of("n","i","text","list","objects","nested","bytes","ints","longs","empty","flag","absent","objects[].a")) {
            commands.add("data get storage test:root "+path);
            commands.add("data get storage test:root "+path+" 2.5");
        }
        for(String value:List.of("{}","{i:7}","{i:8}","{nested:{Aa:9},new:42}","{list:[7,8]}","{nested:3}"))commands.add("data merge storage test:root "+value);
        for(String path:List.of("absent","i","list[0]","list[-1]","list[]","objects[].a","objects[].absent","{i:7}","nested"))commands.add("data remove storage test:root "+path);
        for(String operation:List.of("set","append","prepend","insert 0","insert -1","insert 2","insert -4","insert 100","merge")) {
            for(String path:List.of("list","absent.children","objects[].next","mixed[]")) {
                commands.add("data modify storage test:root "+path+" "+operation+" value {added:9}");
                commands.add("data modify storage test:root "+path+" "+operation+" from storage test:source values[]");
            }
        }
        for(String path:List.of("objects[]","mixed[]","absent.child","list[]")) {
            commands.add("data modify storage test:root "+path+" merge from storage test:source objects[]");
            commands.add("data modify storage test:root "+path+" merge from storage test:source mixed[]");
        }
        for(String source:List.of("test:source","test:source text","test:source n","test:source values[]","test:source flag","test:source objects[]","test:source absent","test:root list[]","test:absent")) {
            commands.add("data modify storage test:root result set from storage "+source);
            commands.add("data modify storage test:root result set string storage "+source);
        }
        for(String range:List.of("0","1","-1","-20","6","0 0","0 3","1 3","-3 -1","4 1","0 10","-20 -1"))commands.add("data modify storage test:root result set string storage test:source text "+range);
        commands.add("data modify storage test:root list append from storage test:root list[]");
        commands.add("data modify storage test:absent created.items insert 100 value 1");
        commands.add("data get storage missing:root");
        for(double scale:new double[]{1.005,2.675,-0.0,0.005,0.0000001,-1.005,1.0e23,1.0e100,Double.MAX_VALUE,Double.MIN_VALUE}) {
            commands.add("data get storage test:root i "+java.math.BigDecimal.valueOf(scale).toPlainString());
        }
        commands.add("data get storage test:root i -0.0");
        for(String range:List.of("1 2","2 3","-4 -1"))commands.add("data modify storage test:root result set string storage test:source text "+range);
        var rows=new JsonArray();
        for(String command:commands) {
            var containers=new HashMap<Identifier,SavedData>();
            var saved=mock(SavedDataStorage.class);
            doAnswer(call->containers.get(((SavedDataType<?>)call.getArgument(0)).id())).when(saved).get(any());
            doAnswer(call->{SavedDataType<?> type=call.getArgument(0);return containers.computeIfAbsent(type.id(),key->type.constructor().get());}).when(saved).computeIfAbsent(any());
            var storage=new CommandStorage(saved);var id=Identifier.parse("test:root");
            storage.set(id,TagParser.parseCompoundFully(INITIAL));storage.set(Identifier.parse("test:source"),TagParser.parseCompoundFully(SOURCE));
            containers.values().forEach(value->value.setDirty(false));
            var source=mock(CommandSourceStack.class);var server=mock(MinecraftServer.class);
            doReturn(server).when(source).getServer();doReturn(storage).when(server).getCommandStorage();
            doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4))).when(source).permissions();
            var feedback=new JsonArray();
            doAnswer(call->{var entry=new JsonObject();entry.add("message",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,((Supplier<Component>)call.getArgument(0)).get()).getOrThrow());entry.addProperty("broadcast",(boolean)call.getArgument(1));feedback.add(entry);return null;}).when(source).sendSuccess(any(),anyBoolean());
            var row=new JsonObject();row.addProperty("command",command);row.addProperty("initial",INITIAL);row.addProperty("source",SOURCE);
            var nodes=dispatcher.parse(command,source).getContext().getNodes();
            var commandPath=new ArrayList<String>();
            for(var node:nodes) {
                var name=node.getNode().getName().replaceAll("([a-z0-9])([A-Z])","$1-$2").toLowerCase(Locale.ROOT);
                commandPath.add((node.getNode() instanceof com.mojang.brigadier.tree.ArgumentCommandNode<?,?>?"arg-":"")+name);
            }
            row.addProperty("path","commands.path."+String.join(".",commandPath));
            try {row.addProperty("value",dispatcher.execute(command,source));}
            catch(com.mojang.brigadier.exceptions.CommandSyntaxException error) {row.add("error",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,(Component)error.getRawMessage()).getOrThrow());}
            row.add("feedback",feedback);row.addProperty("root",safeSnbt(storage.get(id).toString()));row.addProperty("absent",safeSnbt(storage.get(Identifier.parse("test:absent")).toString()));
            if(storage.get(id).get("result") instanceof StringTag result && result.value().chars().anyMatch(value->Character.isSurrogate((char)value))) {
                boolean unpaired=false;String value=result.value();
                for(int i=0;i<value.length();i++)if(Character.isSurrogate(value.charAt(i))) {
                    if(Character.isHighSurrogate(value.charAt(i))&&i+1<value.length()&&Character.isLowSurrogate(value.charAt(i+1)))i++;
                    else unpaired=true;
                }
                if(unpaired) {
                    row.addProperty("unpaired",true);var units=new JsonArray();value.chars().forEach(unit->units.add(unit));row.add("utf16",units);
                    var componentTag=ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE,NbtUtils.toPrettyComponent(result)).getOrThrow();
                    var bytes=new java.io.ByteArrayOutputStream();NbtIo.writeAnyTag(componentTag,new java.io.DataOutputStream(bytes));
                    row.addProperty("pretty_nbt",HexFormat.of().formatHex(bytes.toByteArray()));
                }
            }
            row.addProperty("dirty",containers.get(Identifier.parse("test:command_storage")).isDirty());
            var keys=new JsonArray();storage.keys().forEach(key->keys.add(key.toString()));row.add("keys",keys);rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows)+"\n");
    }
}
