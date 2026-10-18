import java.util.UUID;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;

/** Actual LivingEntity climb queries, retained caches and player pushability. */
public final class ClimbVectors extends Player {
  GameType mode=GameType.SURVIVAL;
  ClimbVectors(Level level) {super(level,new GameProfile(new UUID(5,6),"climb"));}
  @Override public GameType gameMode() {return mode;}
  void glide(boolean value) {setSharedFlag(7,value);}
  static void put(Level level,BlockPos pos,BlockState state) {
    level.getChunkAt(pos).getSection(level.getSectionIndex(pos.getY())).setBlockState(pos.getX()&15,pos.getY()&15,pos.getZ()&15,state);
  }
  static String last(ClimbVectors player) {
    return player.getLastClimbablePos().map(pos->pos.getX()+","+pos.getY()+","+pos.getZ()).orElse("none");
  }
  public static void run(Level level) {
    var pos=new BlockPos(8,64,8);var air=Blocks.AIR.defaultBlockState();
    var ladder=Blocks.LADDER.defaultBlockState().setValue(LadderBlock.FACING,Direction.NORTH);
    var open=Blocks.OAK_TRAPDOOR.defaultBlockState().setValue(TrapDoorBlock.OPEN,true).setValue(TrapDoorBlock.FACING,Direction.NORTH);
    BlockState[][] cases={{air,air},{ladder,air},{Blocks.VINE.defaultBlockState(),air},{open,ladder},
      {open.setValue(TrapDoorBlock.OPEN,false),ladder},{open.setValue(TrapDoorBlock.FACING,Direction.EAST),ladder},
      {open,air},{Blocks.SCAFFOLDING.defaultBlockState(),air},{Blocks.WEEPING_VINES.defaultBlockState(),air},
      {Blocks.VINE.defaultBlockState(),air},{ladder,air},{air,air}};
    for(int i=0;i<cases.length;i++) {
      put(level,pos,cases[i][0]);put(level,pos.below(),cases[i][1]);
      var player=new ClimbVectors(level);player.setPos(8.5,64,8.5);player.glide(i==9);
      if(i==10)player.mode=GameType.SPECTATOR;if(i==11)player.setHealth(Float.NaN);
      boolean climb=player.onClimbable();boolean pushable=player.isPushable();
      System.out.println("living-climb\t"+i+"\t"+Block.getId(cases[i][0])+"\t"+Block.getId(cases[i][1])+"\t"+player.mode.getId()+"\t"+(i==9)+"\t"+
        Integer.toHexString(Float.floatToRawIntBits(player.getHealth()))+"\t"+climb+"\t"+pushable+"\t"+last(player));
    }
    put(level,pos,ladder);var player=new ClimbVectors(level);player.setPos(8.5,64,8.5);
    boolean first=player.onClimbable();put(level,pos,air);boolean cached=player.onClimbable();
    player.setPos(8.75,64,8.5);boolean sameBlock=player.onClimbable();player.setPos(9.5,64,8.5);player.setPos(8.5,64,8.5);
    System.out.println("living-climb-cache\t"+first+","+cached+","+sameBlock+","+player.onClimbable()+"\t"+last(player));
  }
}
