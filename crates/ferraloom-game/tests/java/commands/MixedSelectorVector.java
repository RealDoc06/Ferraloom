import com.google.gson.*;
import com.mojang.brigadier.StringReader;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.selector.EntitySelectorParser;
import net.minecraft.core.SectionPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.entity.*;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Uses native entity lookup and section storage for mixed selector evaluation. */
public final class MixedSelectorVector {
    @SuppressWarnings({"unchecked","rawtypes"})
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var server=mock(MinecraftServer.class);var level=mock(ServerLevel.class);var players=mock(PlayerList.class);
        doReturn(players).when(server).getPlayerList();doReturn(List.of(level)).when(server).getAllLevels();
        var lookup=new EntityLookup<Entity>();var sections=new EntitySectionStorage<Entity>(Entity.class,k->Visibility.TICKING);
        var getter=new LevelEntityGetterAdapter<Entity>(lookup,sections);
        var getEntities=ServerLevel.class.getDeclaredMethod("getEntities");getEntities.setAccessible(true);
        doReturn(getter).when(level);getEntities.invoke(level);
        doCallRealMethod().when(level).getEntities(any(EntityTypeTest.class),any(java.util.function.Predicate.class),anyList(),anyInt());
        doCallRealMethod().when(level).getEntities(any(EntityTypeTest.class),any(AABB.class),any(java.util.function.Predicate.class),anyList(),anyInt());
        doAnswer(call->getter.get((UUID)call.getArgument(0))).when(level).getEntity(any(UUID.class));
        var scoreboard=new net.minecraft.server.ServerScoreboard(server);
        doReturn(scoreboard).when(server).getScoreboard();doReturn(server).when(level).getServer();
        var team=scoreboard.addPlayerTeam("mixed");
        var objective=scoreboard.addObjective("selection",net.minecraft.world.scores.criteria.ObjectiveCriteria.DUMMY,Component.literal("selection"),net.minecraft.world.scores.criteria.ObjectiveCriteria.RenderType.INTEGER,false,null);
        var definitions=new JsonArray();var entities=new ArrayList<Entity>();var identities=new IdentityHashMap<Entity,Integer>();
        String[] names={"Alice","NearCow","FarItem","DeadCow","Bob"};
        double[] positions={32,1,17,2,0.25};
        for(int index=0;index<names.length;index++) {
            Entity entity=switch(index){case 0,4->mock(ServerPlayer.class);case 1,3->mock(Cow.class);default->mock(ItemEntity.class);};
            EntityType type=switch(index){case 0,4->EntityTypes.PLAYER;case 1,3->EntityTypes.COW;default->EntityTypes.ITEM;};
            UUID uuid=new UUID(0,index+1);double x=positions[index];
            var box=new AABB(x-0.3,0,-0.3,x+0.3,1.8,0.3);
            doReturn(type).when(entity).getType();doReturn(uuid).when(entity).getUUID();doReturn(index+1).when(entity).getId();
            doReturn(Component.literal(names[index])).when(entity).getName();doReturn(names[index]).when(entity).getPlainTextName();
            doReturn(Component.literal(names[index])).when(entity).getDisplayName();doReturn(index!=3).when(entity).isAlive();
            doReturn(Set.of(index==1?"pet":"other")).when(entity).entityTags();
            doReturn(box).when(entity).getBoundingBox();doReturn(new Vec3(x,0,0)).when(entity).position();
            doAnswer(call->{Vec3 p=call.getArgument(0);return (x-p.x)*(x-p.x)+p.y*p.y+p.z*p.z;}).when(entity).distanceToSqr(any(Vec3.class));
            String holder=entity instanceof ServerPlayer?names[index]:uuid.toString();
            doReturn(holder).when(entity).getScoreboardName();doReturn(level).when(entity).level();
            boolean member=index==1||index==4;
            if(member){scoreboard.addPlayerToTeam(holder,team);doReturn(team).when(entity).getTeam();}
            Integer score=index==1?Integer.valueOf(7):index==4?Integer.valueOf(0):null;
            if(score!=null)scoreboard.getOrCreatePlayerScore(entity,objective).set(score);
            entities.add(entity);identities.put(entity,index+1);lookup.add(entity);sections.getOrCreateSection(SectionPos.of(new net.minecraft.core.BlockPos((int)Math.floor(x),0,0)).asLong()).add(entity);
            var definition=new JsonObject();definition.addProperty("name",names[index]);definition.addProperty("kind",net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getKey(type).toString());definition.addProperty("uuid",uuid.toString());definition.addProperty("x",x);definition.addProperty("alive",index!=3);definition.addProperty("tag",index==1?"pet":"other");definition.addProperty("team",member);if(score!=null)definition.addProperty("score",score);definitions.add(definition);
        }
        var levelPlayers=ServerLevel.class.getDeclaredField("players");levelPlayers.setAccessible(true);
        levelPlayers.set(level,List.of((ServerPlayer)entities.get(0),(ServerPlayer)entities.get(4)));
        doCallRealMethod().when(level).getPlayers(any(java.util.function.Predicate.class),anyInt());
        doReturn(List.of((ServerPlayer)entities.get(4),(ServerPlayer)entities.get(0))).when(players).getPlayers();
        doAnswer(call->entities.stream().filter(e->e instanceof ServerPlayer&&e.getPlainTextName().equalsIgnoreCase(call.getArgument(0))).findFirst().orElse(null)).when(players).getPlayerByName(anyString());
        doAnswer(call->entities.stream().filter(e->e instanceof ServerPlayer&&e.getUUID().equals(call.getArgument(0))).findFirst().orElse(null)).when(players).getPlayer(any(UUID.class));
        var vectors=new JsonArray();
        var inputs=List.of("@e","@n","@a","@p","@e[type=player]","@e[type=!player]","@e[type=cow]","@e[tag=pet]","@e[name=NearCow]","@e[sort=nearest,limit=3]","@e[sort=furthest,limit=2]","@e[limit=2]","@e[dx=64,dy=2,dz=2]","@e[distance=..64]","@e[distance=1..20,sort=arbitrary]","@e[x=20,dx=-20,dy=2,dz=2]","@e[team=mixed]","@e[team=]","@e[team=!mixed]","@e[scores={selection=7..}]","@e[scores={selection=0}]","@e[scores={missing=0..}]","@e[team=mixed,scores={selection=7}]","@e[scores={}]","@s","Alice","00000000-0000-0000-0000-000000000003","00000000-0000-0000-0000-000000000004","00000000-0000-0000-0000-000000000099");
        for(int sourceIndex:new int[]{0,1})for(String input:inputs)for(boolean playerOnly:new boolean[]{false,true}) {
            var source=mock(CommandSourceStack.class);doReturn(server).when(source).getServer();doReturn(level).when(source).getLevel();doReturn(entities.get(sourceIndex)).when(source).getEntity();
            doReturn(Vec3.ZERO).when(source).getPosition();doReturn(PermissionSet.ALL_PERMISSIONS).when(source).permissions();doReturn(FeatureFlags.VANILLA_SET).when(source).enabledFeatures();
            var selector=new EntitySelectorParser(new StringReader(input),true).parse();
            var selected=playerOnly?selector.findPlayers(source):selector.findEntities(source);
            var row=new JsonObject();row.addProperty("selector",input);row.addProperty("source_entity",sourceIndex+1);row.addProperty("players_only",playerOnly);row.add("selected",new Gson().toJsonTree(selected.stream().map(identities::get).toList()));vectors.add(row);
        }
        var output=new JsonObject();output.add("entities",definitions);output.add("vectors",vectors);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(output)+"\n");
    }
}
