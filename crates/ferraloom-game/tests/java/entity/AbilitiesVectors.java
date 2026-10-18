import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.world.entity.player.Abilities;
import net.minecraft.world.level.GameType;

/** Actual Abilities.Packed CODEC defaults/errors and GameType updates. */
public final class AbilitiesVectors {
  static String packed(Abilities.Packed a){return a.invulnerable()+","+a.flying()+","+a.mayFly()+","+a.instabuild()+","+a.mayBuild()+","+TravelVectors.f(a.flyingSpeed())+","+TravelVectors.f(a.walkingSpeed());}
  public static void run()throws Exception {
    var inputs=new ArrayList<String>(List.of("{}","{invulnerable:1b}","{invulnerable:1b,flying:1b,mayfly:1b,instabuild:1b,mayBuild:0b,flySpeed:0.125f,walkSpeed:-0.25f}","{invulnerable:256,flying:-0.0f,mayfly:-7,flySpeed:0.15d,walkSpeed:2}","{flySpeed:NaNf,walkSpeed:Infinityf}"));
    for(String key:List.of("invulnerable","flying","mayfly","instabuild","mayBuild","flySpeed","walkSpeed"))inputs.add("{"+key+":'invalid',other:1}");
    inputs.add("[]");inputs.add("{invulnerable:1b,flySpeed:'invalid',walkSpeed:0.25f}");
    int index=0;
    for(String input:inputs) {
      var tag=TagParser.create(NbtOps.INSTANCE).parseFully(input);var decoded=Abilities.Packed.CODEC.parse(NbtOps.INSTANCE,tag);
      var value=decoded.result();
      String encoded=value.map(a->Abilities.Packed.CODEC.encodeStart(NbtOps.INSTANCE,a).getOrThrow().toString()).orElse("-");
      System.out.println("abilities-packed\t"+(index++)+"\t"+input+"\t"+value.map(AbilitiesVectors::packed).orElse("error")+"\t"+encoded);
    }
    for(GameType type:GameType.values())for(boolean flying:List.of(false,true))for(boolean edited:List.of(false,true)) {
      var a=new Abilities();a.invulnerable=edited;a.flying=flying;a.mayfly=edited;a.instabuild=edited;a.mayBuild=!edited;a.setFlyingSpeed(0.125f);a.setWalkingSpeed(-0.25f);type.updatePlayerAbilities(a);
      System.out.println("abilities-game-type\t"+type.getId()+"\t"+flying+"\t"+edited+"\t"+packed(a.pack()));
    }
  }
}
