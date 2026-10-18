import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import com.mojang.brigadier.*;
import com.mojang.brigadier.context.*;
import com.mojang.brigadier.suggestion.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.*;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.commands.TickCommand;
import net.minecraft.server.permissions.*;
import static org.mockito.Mockito.*;

/** Captures cursor ranges and translated tooltips, as well as completion text. */
public final class SuggestionVector {
    static JsonObject capture(String kind,String input,int level,Suggestions suggestions) {
        var vector = new JsonObject();vector.addProperty("kind",kind);vector.addProperty("input",input);vector.addProperty("level",level);
        vector.addProperty("start",suggestions.getRange().getStart());vector.addProperty("end",suggestions.getRange().getEnd());
        var entries = new JsonArray();
        suggestions.getList().forEach(s -> {
            var entry = new JsonObject();entry.addProperty("text",s.getText());
            if (s.getTooltip() instanceof Component tooltip) entry.add("tooltip",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,tooltip).getOrThrow());
            entries.add(entry);
        });
        vector.add("suggestions",entries);return vector;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var vectors = new JsonArray();
        var dispatcher = new CommandDispatcher<CommandSourceStack>();TickCommand.register(dispatcher);
        for (int level : new int[]{0,4}) {
            var source=mock(CommandSourceStack.class);
            doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(level))).when(source).permissions();
            doReturn(List.of("Alex","Alice","Doc")).when(source).getOnlinePlayerNames();
            doReturn(List.of()).when(source).getSelectedEntities();
            for (var input : List.of("","A","Alex","@","@z","@p","@p ","@pfoo","@a","@n","@a[","@a[lim","@a[limit","@a[limit=","@a[limit=1","@a[limit=1,","@s[","@a[name=Doc,","@a[name=!Doc,","@a[sort=","@a[sort=n","@a[gamemode=","@a[gamemode=!s","@a[gamemode=!creative,gamemode=","@e[type=minecraft:pl","@e[type=","@e[type=!minecraft:player,type=","@a[x=1 ,","@a[x=1!]","@a[]")) {
                var context = new CommandContextBuilder<>(dispatcher,source,dispatcher.getRoot(),0).build(input);
                vectors.add(capture("entity",input,level,EntityArgument.entities().listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
                vectors.add(capture("game_profile",input,level,GameProfileArgument.gameProfile().listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
            }
            if (level==4) for (var input : List.of("tick ","tick rate ","tick rate 2","tick step ","tick step 1","tick sprint ","tick sprint 1")) {
                vectors.add(capture("command",input,level,dispatcher.getCompletionSuggestions(dispatcher.parse(input,source)).join()));
            }
        }
        var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p->p.apply());
        var buildContext=net.minecraft.commands.CommandBuildContext.simple(registries,net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);
        var item=net.minecraft.commands.arguments.item.ItemArgument.item(buildContext);
        var itemSource=mock(CommandSourceStack.class);
        for(var input:List.of("","sto","stone","stone ","stone[","stone[dam","stone[damage","stone[damage=","stone[damage=1","stone[damage=1,","stone[!","stone[!damage","stone[!damage]","stone[damage=1]","stone[creative_slot_lock","stone[unknown","stone[custom_name='x'","stone[damage=1, damage")) {
            var itemContext=new CommandContextBuilder<>(dispatcher,itemSource,dispatcher.getRoot(),0).build(input);
            vectors.add(capture("item",input,4,item.listSuggestions(itemContext,new SuggestionsBuilder(input,0)).join()));
        }
        // Bound tags are explicit: the datagen provider otherwise invents unknown tag holders.
        var blockKey=net.minecraft.tags.TagKey.create(net.minecraft.core.registries.Registries.BLOCK,net.minecraft.resources.Identifier.parse("example:mixed"));
        var parentBlocks=registries.lookupOrThrow(net.minecraft.core.registries.Registries.BLOCK);
        var members=List.of("minecraft:oak_log","minecraft:chest").stream().map(name->parentBlocks.getOrThrow(net.minecraft.resources.ResourceKey.create(net.minecraft.core.registries.Registries.BLOCK,net.minecraft.resources.Identifier.parse(name)))).toList();
        net.minecraft.core.HolderSet.Named<net.minecraft.world.level.block.Block> mixed=mock(net.minecraft.core.HolderSet.Named.class);
        doReturn(blockKey).when(mixed).key();
        doAnswer(call->members.iterator()).when(mixed).iterator();
        doAnswer(call->members.stream()).when(mixed).stream();
        var boundRegistries = new net.minecraft.core.HolderLookup.Provider() {
            public java.util.stream.Stream<net.minecraft.resources.ResourceKey<? extends net.minecraft.core.Registry<?>>> listRegistryKeys() { return registries.listRegistryKeys(); }
            public <T> Optional<net.minecraft.core.HolderLookup.RegistryLookup<T>> lookup(net.minecraft.resources.ResourceKey<? extends net.minecraft.core.Registry<? extends T>> key) {
                return registries.lookup(key).map(parent -> new net.minecraft.core.HolderLookup.RegistryLookup.Delegate<T>() {
                    public net.minecraft.core.HolderLookup.RegistryLookup<T> parent() { return parent; }
                    public java.util.stream.Stream<net.minecraft.core.HolderSet.Named<T>> listTags() {
                        return key.equals(net.minecraft.core.registries.Registries.BLOCK) ? java.util.stream.Stream.of((net.minecraft.core.HolderSet.Named<T>)(Object)mixed) : java.util.stream.Stream.empty();
                    }
                    public Optional<net.minecraft.core.HolderSet.Named<T>> get(net.minecraft.tags.TagKey<T> tag) {
                        return tag.equals(blockKey) ? Optional.of((net.minecraft.core.HolderSet.Named<T>)(Object)mixed) : Optional.empty();
                    }
                });
            }
        };
        var blocks=net.minecraft.commands.CommandBuildContext.simple(boundRegistries,net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);
        var blockState=net.minecraft.commands.arguments.blocks.BlockStateArgument.block(blocks);
        var blockPredicate=net.minecraft.commands.arguments.blocks.BlockPredicateArgument.blockPredicate(blocks);
        for(var input:List.of("sto","stone","stone ","stone[","oak_log","oak_log[","oak_log[a","oak_log[axis","oak_log[axis=","oak_log[axis=q","oak_log[axis=x","oak_log[axis=x,","oak_log[axis=x]","oak_log[axis=x,axis=","chest","chest[","chest[facing=","chest[facing=north,","chest[facing=north]","chest[facing=north]{","wheat[age=","wheat[age=2","wheat[age=2]","stone{","#","#example:missing","#example:mixed","#example:mixed[","#example:mixed[a","#example:mixed[axis","#example:mixed[axis=","#example:mixed[axis=x","#example:mixed[axis=x,","#example:mixed[axis=x]","#example:mixed[facing=n","#example:mixed[unknown=x","#example:mixed[axis=x,axis=")) {
            var context=new CommandContextBuilder<>(dispatcher,itemSource,dispatcher.getRoot(),0).build(input);
            vectors.add(capture("block",input,4,blockState.listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
            vectors.add(capture("block_predicate",input,4,blockPredicate.listSuggestions(context,new SuggestionsBuilder(input,0)).join()));
        }
        var resources = List.of(net.minecraft.resources.Identifier.parse("example:thing"),net.minecraft.resources.Identifier.parse("minecraft:thing"),net.minecraft.resources.Identifier.parse("example:other"));
        for (var input : List.of("","!","#","!#","!#th","thing","example:")) {
            vectors.add(capture("resource_prefix",input,4,net.minecraft.commands.SharedSuggestionProvider.suggestResource(resources,new SuggestionsBuilder(input,0),"!#").join()));
            vectors.add(capture("resource_plain",input,4,net.minecraft.commands.SharedSuggestionProvider.suggestResource(resources,new SuggestionsBuilder(input,0)).join()));
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
