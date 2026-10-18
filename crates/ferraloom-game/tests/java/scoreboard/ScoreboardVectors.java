import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import net.minecraft.network.chat.numbers.*;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.Level;
import org.mockito.Mockito;

/** Exercises vanilla's actual model, predicate and persistence codec without a running server. */
public class ScoreboardVectors extends Scoreboard {
  Packed packed() { return new Packed(packObjectives(), packPlayerScores(), packDisplaySlots(), packPlayerTeams()); }
  record Packed(List<Objective.Packed> objectives, List<Scoreboard.PackedScore> scores, Map<DisplaySlot,String> slots, List<PlayerTeam.Packed> teams) {
    ScoreboardSaveData.Packed value() { return new ScoreboardSaveData.Packed(objectives,scores,slots,teams); }
  }
  static final java.io.PrintStream OUTPUT = new java.io.PrintStream(new java.io.FileOutputStream(java.io.FileDescriptor.out));
  static void row(Object... values) { OUTPUT.println(String.join("\t", Arrays.stream(values).map(Object::toString).toList())); }
  static Entity entity(PlayerTeam team, Level level) {
    Entity entity = Mockito.mock(Entity.class);
    Mockito.when(entity.getTeam()).thenReturn(team);
    Mockito.when(entity.level()).thenReturn(level);
    Mockito.when(entity.isPushable()).thenReturn(true);
    return entity;
  }
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    ScoreboardVectors board = new ScoreboardVectors();
    PlayerTeam a = board.addPlayerTeam("a"), b = board.addPlayerTeam("b");
    row("membership", "first", board.addPlayerToTeam("Doc",a), a.getPlayers().size());
    row("membership", "same", board.addPlayerToTeam("Doc",a), a.getPlayers().size());
    row("membership", "move", board.addPlayerToTeam("Doc",b), a.getPlayers().size(), b.getPlayers().size());
    try { board.removePlayerFromTeam("Doc",a); throw new AssertionError(); } catch (IllegalStateException expected) { row("membership","wrong-team",true); }
    row("membership","remove",board.removePlayerFromTeam("Doc"),board.removePlayerFromTeam("Doc"));
    row("membership","existing",board.addPlayerTeam("a") == a);
    board.addPlayerToTeam("Doc",a); board.removePlayerTeam(a);
    row("membership","remove-team",board.getPlayersTeam("Doc") == null);
    a = board.addPlayerTeam("a");
    row("allied", "none", a.isAlliedTo(null));
    row("allied", "self", a.isAlliedTo(a));
    row("allied", "other", a.isAlliedTo(b));
    row("allied", "foreign", a.isAlliedTo(new Scoreboard().addPlayerTeam("a")));
    Level level = Mockito.mock(Level.class);
    for (int mode = 0; mode < 4; mode++) {
      for (Team.CollisionRule own : Team.CollisionRule.values()) for (Team.CollisionRule other : Team.CollisionRule.values()) {
        a.setCollisionRule(own); b.setCollisionRule(other);
        PlayerTeam ownTeam = mode == 2 || mode == 3 ? null : a;
        PlayerTeam otherTeam = mode == 0 ? a : mode == 3 ? null : b;
        // Same-team rules necessarily read the same rule from both entities.
        boolean value = EntitySelector.pushableBy(entity(ownTeam,level)).test(entity(otherTeam,level));
        row("collision",mode,own.name,other.name,value);
      }
    }
    Objective objective = board.addObjective("points",ObjectiveCriteria.DUMMY,Component.literal("Points"),ObjectiveCriteria.RenderType.INTEGER,true,new FixedFormat(Component.literal("fixed")));
    board.setDisplayObjective(DisplaySlot.SIDEBAR,objective);
    ScoreAccess score = board.getOrCreatePlayerScore(ScoreHolder.forNameOnly("Doc"),objective);
    score.set(42); score.unlock(); score.display(Component.literal("Display")); score.numberFormatOverride(new StyledFormat(Style.EMPTY.withBold(true)));
    b.setPlayerPrefix(Component.literal("[")); b.setPlayerSuffix(Component.literal("]")); b.setColor(Optional.of(TeamColor.RED));
    b.setAllowFriendlyFire(false); b.setSeeFriendlyInvisibles(false); b.setNameTagVisibility(Team.Visibility.HIDE_FOR_OTHER_TEAMS);
    b.setDeathMessageVisibility(Team.Visibility.NEVER); b.setCollisionRule(Team.CollisionRule.PUSH_OWN_TEAM);
    board.addPlayerToTeam("Doc",b); board.addPlayerToTeam("00000000-0000-0000-0000-000000000001",b);
    CompoundTag data = (CompoundTag)ScoreboardSaveData.Packed.CODEC.encodeStart(NbtOps.INSTANCE,board.packed().value()).getOrThrow();
    CompoundTag root = new CompoundTag(); root.put("data",data); root.putInt("DataVersion",5023);
    Path path = Path.of(args[0]); NbtIo.writeCompressed(root,path);
    CompoundTag loaded = NbtIo.readCompressed(path,NbtAccounter.unlimitedHeap());
    var decoded = ScoreboardSaveData.Packed.CODEC.parse(NbtOps.INSTANCE,loaded.get("data")).getOrThrow();
    if (!decoded.equals(board.packed().value())) throw new AssertionError("disk round trip");
    row("saved",loaded.toString());
    for (String minimal : List.of("{}", "{Teams:[{Name:'defaults'}]}","{Objectives:[{Name:'minimal',DisplayName:'Minimal'}],PlayerScores:[{Name:'Doc',Objective:'minimal'}]}")) {
      Tag input = TagParser.parseCompoundFully(minimal);
      Tag output = ScoreboardSaveData.Packed.CODEC.encodeStart(NbtOps.INSTANCE,ScoreboardSaveData.Packed.CODEC.parse(NbtOps.INSTANCE,input).getOrThrow()).getOrThrow();
      row("codec",minimal,output);
    }
    row("criterion", "minecraft.mined:minecraft.stone",ObjectiveCriteria.byName("minecraft.mined:minecraft.stone").orElseThrow().isReadOnly());
  }
}
