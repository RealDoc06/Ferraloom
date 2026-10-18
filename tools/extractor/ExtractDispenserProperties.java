import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.dispenser.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.Vec3;

/** Pure bootstrapped item membership and dispenser constructor parameters. */
public final class ExtractDispenserProperties {
    static boolean position(ProjectileItem.DispenseConfig config,double scale,double offsetY) {
        for(var direction:Direction.values()) {
            var source=new BlockSource(null,BlockPos.ZERO,Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING,direction),null);
            var actual=config.positionFunction().getDispensePosition(source,direction);
            var expected=source.center().add(direction.getStepX()*scale,direction.getStepY()*scale+offsetY,direction.getStepZ()*scale);
            if(Double.doubleToRawLongBits(actual.x())!=Double.doubleToRawLongBits(expected.x)||Double.doubleToRawLongBits(actual.y())!=Double.doubleToRawLongBits(expected.y)||Double.doubleToRawLongBits(actual.z())!=Double.doubleToRawLongBits(expected.z))return false;
        }
        return true;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var entries=new JsonArray();
        for(var item:BuiltInRegistries.ITEM) {
            var out=new JsonObject();out.addProperty("id",BuiltInRegistries.ITEM.getId(item));out.addProperty("spawn_egg",item instanceof SpawnEggItem);
            var behavior=DispenserBlock.DISPENSER_REGISTRY.get(item);
            if(behavior==null)out.add("behavior",JsonNull.INSTANCE);
            else if(behavior==DispenseItemBehavior.NOOP)out.addProperty("behavior","Noop");
            else {
                var name=behavior.getClass().getName();
                if(name.contains("$$Lambda"))throw new IllegalStateException("Unclassified dispenser lambda "+item);
                out.addProperty("behavior",name);
            }
            if(item instanceof ProjectileItem projectile) {
                var config=projectile.createDispenseConfig();var p=new JsonObject();
                p.addProperty("power",config.power());p.addProperty("uncertainty",config.uncertainty());
                if(config.overrideDispenseEvent().isPresent())p.addProperty("event",config.overrideDispenseEvent().get());else p.add("event",JsonNull.INSTANCE);
                String kind;
                if(position(config,0.7,0.1))kind="Default";
                else if(position(config,1.0,0.0))kind="BlockFace";
                else if(position(config,0.5000099999997474,0.0))kind="Firework";
                else throw new IllegalStateException("Unclassified projectile position "+item.getClass().getName());
                p.addProperty("position",kind);out.add("projectile",p);
            } else out.add("projectile",JsonNull.INSTANCE);
            entries.add(out);
        }
        Files.writeString(Path.of(args[0]),new Gson().toJson(entries)+"\n");
    }
}
