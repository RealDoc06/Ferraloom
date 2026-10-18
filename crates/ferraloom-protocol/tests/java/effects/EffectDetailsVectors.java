import com.mojang.serialization.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;

/** Executes the actual recursive MobEffectInstance.Details.MAP_CODEC. */
public final class EffectDetailsVectors {
  @SuppressWarnings("unchecked")
  public static void main(String[] args)throws Exception {
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
    var details=Class.forName("net.minecraft.world.effect.MobEffectInstance$Details");
    var field=details.getDeclaredField("MAP_CODEC");field.setAccessible(true);
    var codec=((MapCodec<Object>)field.get(null)).codec();
    String[] cases={"{}","{amplifier:7b}","{duration:-1}","{ambient:1b}","{show_particles:0b}","{show_icon:0b}",
      "{amplifier:'bad'}","{duration:'bad'}","{ambient:'bad'}","{show_particles:'bad'}","{show_icon:'bad'}","{hidden_effect:'bad'}",
      "{show_particles:0b,show_icon:'bad'}","{show_particles:'bad',show_icon:0b}",
      "{hidden_effect:{duration:'bad',amplifier:2b,show_particles:0b}}",
      "{hidden_effect:{hidden_effect:{duration:10},duration:-1}}",
      "{amplifier:-2.9f}","{amplifier:4294967297L}","{amplifier:1000}",
      "{duration:-2.9d}","{duration:4294967297L}","{ambient:256}","{show_particles:0.5f}",
      "{show_icon:0.0d,show_particles:1b}","{hidden_effect:[]}","'bad'","[]"};
    for(int i=0;i<cases.length;i++){
      var input=TagParser.create(NbtOps.INSTANCE).parseFully(cases[i]);
      var parsed=codec.parse(NbtOps.INSTANCE,input).resultOrPartial(error->{});
      System.out.println("effect-details\t"+i+"\t"+input+"\t"+(parsed.isPresent()?codec.encodeStart(NbtOps.INSTANCE,parsed.get()).getOrThrow():"ERROR"));
    }
  }
}
