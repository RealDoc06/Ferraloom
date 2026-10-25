import java.util.*;
import net.minecraft.core.registries.*;

/** Exact effect-component presence from the real 26.3 loaded registry. */
public final class RegistryVectors {
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var registry=NeighborVectors.load().lookupOrThrow(Registries.ENCHANTMENT);
        for(var entry:registry.listElements().sorted(Comparator.comparing(e->e.key().identifier().toString())).toList()) {
            var components=entry.value().effects().keySet().stream()
                .map(type->BuiltInRegistries.ENCHANTMENT_EFFECT_COMPONENT_TYPE.getKey(type).toString())
                .sorted().toList();
            System.out.println("enchantment-registry\t"+entry.key().identifier()+"\t"+String.join(",",components));
        }
    }
}
