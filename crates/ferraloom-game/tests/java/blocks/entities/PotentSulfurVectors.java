import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;

/** Native countdown persistence/reset and one-time setLevel clock capture. */
public final class PotentSulfurVectors {
  static final BlockPos POS=new BlockPos(8,5,8);
  static final class World extends NeighborFixture {
    long time;
    World(RegistryAccess access){super(access,100);}
    @Override public long getGameTime(){return time;}
    void seed(){random.setSeed(263777);}
    int nextRandom(){return random.nextInt();}
  }
  public static void main(String[] args)throws Exception{
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
    var access=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var registries=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),access.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(DataComponentInitializers.PendingComponents::apply);
    var world=new World(access);
    var inputs=List.of("{}","{countdown:'wrong'}","{countdown:{}}","{countdown:-1}","{countdown:0}","{countdown:37}","{countdown:-2147483648}","{countdown:2147483647}","{countdown:-7b}","{countdown:123s}","{countdown:4294967301L}","{countdown:-1.9f}","{countdown:2.9d}");
    for(var state:Blocks.POTENT_SULFUR.getStateDefinition().getPossibleStates()){
      var fresh=new PotentSulfurBlockEntity(POS,state);
      System.out.println("default\t"+Block.getId(state)+"\t"+fresh.saveWithoutMetadata(registries)+"\t"+fresh.eruptionTick+"\t"+(((PotentSulfurBlock)Blocks.POTENT_SULFUR).getTicker(world,state,BlockEntityTypes.POTENT_SULFUR)!=null)+"\t"+fresh.getUpdateTag(registries)+"\t"+(fresh.getUpdatePacket()!=null));
      for(int initial:new int[]{-17,-1,0,7,Integer.MAX_VALUE})for(var input:inputs){
        var entity=new PotentSulfurBlockEntity(POS,state);entity.waitingCountdown=initial;entity.eruptionTick=123;
        entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,registries,TagParser.parseCompoundFully(input)));
        var saved=entity.saveWithoutMetadata(registries);int loaded=entity.waitingCountdown;world.seed();entity.resetCountdown();
        System.out.println("load-reset\t"+Block.getId(state)+"\t"+initial+"\t"+input+"\t"+loaded+"\t"+saved+"\t"+entity.waitingCountdown+"\t"+entity.eruptionTick+"\t"+world.nextRandom());
      }
      for(long initial:new long[]{-1,0,5})for(var times:List.of(new long[]{-1,0},new long[]{0,5},new long[]{42,99},new long[]{-10,-1},new long[]{Long.MAX_VALUE,Long.MIN_VALUE})){
        var entity=new PotentSulfurBlockEntity(POS,state);entity.waitingCountdown=7;entity.eruptionTick=initial;
        world.time=times[0];entity.setLevel(world);long first=entity.eruptionTick;world.time=times[1];entity.setLevel(world);
        System.out.println("set-level\t"+Block.getId(state)+"\t"+initial+"\t"+times[0]+"\t"+times[1]+"\t"+first+"\t"+entity.eruptionTick+"\t"+entity.waitingCountdown+"\t"+(entity.getLevel()==world));
      }
    }
  }
}
