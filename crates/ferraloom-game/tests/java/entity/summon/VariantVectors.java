import java.util.*;
import com.mojang.serialization.JsonOps;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.entity.animal.cow.CowVariant;
import net.minecraft.world.entity.variant.PriorityProvider;
import net.minecraft.util.RandomSource;

/** Bootstrap registry and actual PriorityProvider selection observations. */
public final class VariantVectors {
  record Condition(int id,boolean result,List<Integer> calls) implements PriorityProvider.SelectorCondition<Integer> {
    public boolean test(Integer ignored){calls.add(id);return result;}
  }
  record Entry(String name,List<PriorityProvider.Selector<Integer,Condition>> selectors) implements PriorityProvider<Integer,Condition> {}
  static void run(RegistryAccess registries,HolderLookup.Provider context) {
    var ops=context.createSerializationContext(JsonOps.INSTANCE);
    for(var holder:registries.lookupOrThrow(Registries.COW_VARIANT).listElements().toList()) {
      var json=CowVariant.DIRECT_CODEC.encodeStart(ops,holder.value()).getOrThrow();
      System.out.println("mob-variant\t"+holder.getRegisteredName()+"\t"+json.getAsJsonObject().get("spawn_conditions"));
    }
    for(var holder:registries.lookupOrThrow(Registries.COW_SOUND_VARIANT).listElements().toList())
      System.out.println("mob-sound\t"+holder.getRegisteredName());
    for(int scenario=0;scenario<10;scenario++) {
      var calls=new ArrayList<Integer>();var entries=new ArrayList<Entry>();
      entries.add(new Entry("a",List.of(new PriorityProvider.Selector<>(new Condition(0,(scenario&1)!=0,calls),5),new PriorityProvider.Selector<>(new Condition(1,(scenario&2)!=0,calls),5))));
      entries.add(new Entry("b",List.of(new PriorityProvider.Selector<>(new Condition(2,(scenario&4)!=0,calls),5))));
      entries.add(new Entry("c",List.of(new PriorityProvider.Selector<>(new Condition(3,true,calls),0))));
      if(scenario==8)entries.clear();
      if(scenario==9){entries.clear();entries.add(new Entry("a",List.of(new PriorityProvider.Selector<>(new Condition(0,false,calls),5))));}
      var selected=PriorityProvider.select(entries.stream(),e->e,0).map(Entry::name).toList();
      for(long seed:List.of(0L,1L,12345L)) {
        var random=RandomSource.create(seed);var pick=net.minecraft.util.Util.getRandomSafe(selected,random);
        System.out.println("variant-select\t"+scenario+"\t"+seed+"\t"+String.join(",",selected)+"\t"+calls+"\t"+pick.orElse("-")+"\t"+random.nextLong());
      }
    }
  }
}
