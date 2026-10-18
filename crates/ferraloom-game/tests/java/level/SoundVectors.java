import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.sounds.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

/** Real Level#playSound draws, SoundPacket quantization and particle center tests. */
public final class SoundVectors {
    public static void main(String[] args) throws Exception {
        var output=System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
        var registries=NeighborVectors.load();
        var sound=BuiltInRegistries.SOUND_EVENT.getValue(Identifier.parse("minecraft:entity.generic.extinguish_fire"));
        var field=net.minecraft.world.level.Level.class.getDeclaredField("soundSeedGenerator");field.setAccessible(true);
        for(long seed:new long[]{0,1,12345}) {
            var level=new SoundFixture(registries);((RandomSource)field.get(level)).setSeed(seed);level.getRandom().setSeed(987);
            for(int i=0;i<4;i++)level.playSound(null,0,0,0,sound,SoundSource.NEUTRAL,1,1);
            System.out.println("sound-stream:"+seed+"\t"+String.join(",",level.seeds)+"|"+level.getRandom().nextInt());
        }
        for(double x:new double[]{-32.5,-0.9,-0.2,0,0.2,0.9,32.5}) {
            var packet=new net.minecraft.network.protocol.game.ClientboundSoundPacket(BuiltInRegistries.SOUND_EVENT.wrapAsHolder(sound),SoundSource.NEUTRAL,x,0,0,1,1,0);
            System.out.println("sound-position:"+x+"\t"+(int)(packet.getX()*8));
        }
        for(double px:new double[]{-0.1,0.49,1.01})for(double x:new double[]{31.5,32.5,511.5,512.5}) {
            var center=BlockPos.containing(px,0,0);var position=new Vec3(x,0.5,0.5);
            System.out.println("particle-range:"+px+":"+x+"\t"+center.closerToCenterThan(position,32)+","+center.closerToCenterThan(position,512));
        }
    }
}
class SoundFixture extends NeighborFixture {
    final List<String> seeds=new ArrayList<>();
    SoundFixture(RegistryAccess access){super(access,100);}
    @Override public void playSeededSound(Entity except,double x,double y,double z,Holder sound,SoundSource source,float volume,float pitch,long seed){seeds.add(Long.toString(seed));}
    @Override public void playSeededSound(Entity except,Entity entity,Holder sound,SoundSource source,float volume,float pitch,long seed){seeds.add(Long.toString(seed));}
}
