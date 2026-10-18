import java.lang.reflect.Proxy;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.util.AbortableIterationConsumer;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.entity.*;
import net.minecraft.world.phys.*;

/** Runs the real Block.pushEntitiesUp with native section-order selection. */
public final class PushVectors {
  static class Pushed extends PrimedTnt {
    final List<String> calls;
    Pushed(Level level,List<String> calls) {super(EntityTypes.TNT,level);this.calls=calls;}
    @Override public void teleportRelative(double x,double y,double z) {calls.add(getId()+":"+TrackingVectors.vec(new Vec3(x,y,z)));setPos(getX()+x,getY()+y,getZ()+z);}
  }
  static final class Spectator extends net.minecraft.world.entity.player.Player {
    Spectator(Level level) {super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"spectator"));}
    @Override public GameType gameMode() {return GameType.SPECTATOR;}
  }
  public static void run(Level level) {
    Block[][] pairs={{Blocks.DIRT_PATH,Blocks.DIRT},{Blocks.FARMLAND,Blocks.DIRT},{Blocks.AIR,Blocks.STONE},{Blocks.STONE,Blocks.AIR},{Blocks.STONE_SLAB,Blocks.STONE},{Blocks.STONE,Blocks.STONE}};
    var pos=new BlockPos(8,63,8);
    for(int index=0;index<pairs.length;index++) {
      var calls=new ArrayList<String>();var entities=new EntitySectionStorage<Entity>(Entity.class,key->Visibility.TICKING);
      var first=new Pushed(level,calls);first.setId(1);first.setPos(8.5,63.9375,8.5);
      var second=new Pushed(level,calls);second.setId(2);second.setPos(8.75,63.5,8.5);
      var spectator=new Spectator(level);spectator.setId(3);spectator.setPos(8.5,63.9375,8.5);
      for(Entity entity:List.of(second,first,spectator)) entities.getOrCreateSection(SectionPos.asLong(entity.blockPosition())).add(entity);
      int[] queries={0};
      var access=(LevelAccessor)Proxy.newProxyInstance(PushVectors.class.getClassLoader(),new Class<?>[]{LevelAccessor.class},(proxy,method,args)-> {
        if(method.getName().equals("getEntities")) {
          queries[0]++;var found=new ArrayList<Entity>();var box=(AABB)args[1];
          entities.getEntities(box,AbortableIterationConsumer.forConsumer(entity->{if(!entity.isSpectator())found.add(entity);}));return found;
        }
        throw new AssertionError(method.toString());
      });
      BlockState result=Block.pushEntitiesUp(pairs[index][0].defaultBlockState(),pairs[index][1].defaultBlockState(),access,pos);
      System.out.println("push-entities\t"+index+"\t"+Block.getId(result)+"\t"+queries[0]+"\t"+(calls.isEmpty()?"-":String.join(";",calls))+"\t"+TrackingVectors.vec(first.position())+"\t"+TrackingVectors.vec(second.position())+"\t"+TrackingVectors.vec(spectator.position()));
    }
  }
}
