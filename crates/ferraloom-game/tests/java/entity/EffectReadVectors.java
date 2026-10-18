import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.RegistryOps;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.TagValueInput;

/** Real LivingEntity.load and effect-list codec, compared without holder hash
 * iteration: the fixture sorts only its observation by the registry key. */
public final class EffectReadVectors {
  public static void run(Level level)throws Exception {
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),level.registryAccess().listRegistries()));
    var ops=RegistryOps.create(NbtOps.INSTANCE,lookup);
    String[] cases={"{}","{active_effects:[]}",
      "{active_effects:[{id:'minecraft:speed',duration:100,amplifier:2b},{id:'minecraft:slow_falling',duration:-1}]}",
      "{active_effects:[{id:'minecraft:levitation',duration:20,hidden_effect:{duration:100,amplifier:4b}},{id:'minecraft:levitation',duration:40,amplifier:1b}]}",
      "{active_effects:[{id:'minecraft:speed',duration:10},{id:'missing:effect',duration:40},{id:'minecraft:slow_falling',duration:30}]}",
      "{active_effects:[{id:'minecraft:speed',duration:10},{id:'minecraft:levitation',duration:'bad'},{id:'minecraft:slow_falling',duration:30}]}",
      "{active_effects:[{id:'minecraft:levitation',duration:20,hidden_effect:{duration:100,amplifier:4b,show_particles:false}}]}",
      "{active_effects:'bad'}"};
    for(int i=0;i<cases.length;i++){
      var player=new LivingVectors(level);var input=TagParser.parseCompoundFully(cases[i]);
      player.load(TagValueInput.create(ProblemReporter.DISCARDING,lookup,input));
      var sorted=player.getActiveEffects().stream().sorted(Comparator.comparing(effect->BuiltInRegistries.MOB_EFFECT.getKey(effect.getEffect().value()).toString())).toList();
      var result=new ListTag();var states=new ArrayList<String>();
      for(var effect:sorted){
        result.add(MobEffectInstance.CODEC.encodeStart(ops,effect).getOrThrow());
        states.add(BuiltInRegistries.MOB_EFFECT.getKey(effect.getEffect().value())+"="+EffectVectors.state(effect));
      }
      System.out.println("mob-effect-load\t"+i+"\t"+input+"\t"+String.join(",",states)+"\t"+result);
    }
  }
}
