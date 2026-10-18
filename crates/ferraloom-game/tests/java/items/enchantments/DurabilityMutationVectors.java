import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Unit;
import net.minecraft.world.item.*;

/** Native ItemStack.applyDamage with null player and synchronous break consumers. */
public final class DurabilityMutationVectors {
    static ItemStack fixture(int kind,int count) {
        var stack=new ItemStack(kind==7?Items.STONE:Items.DIAMOND_PICKAXE,Math.max(count,1));
        if(kind!=0) { stack.set(DataComponents.MAX_DAMAGE,7);stack.set(DataComponents.DAMAGE,2); }
        if(kind==2) stack.set(DataComponents.UNBREAKABLE,Unit.INSTANCE);
        if(kind==3) stack.remove(DataComponents.DAMAGE);
        if(kind==4) stack.remove(DataComponents.MAX_DAMAGE);
        if(kind==5) stack.set(DataComponents.MAX_DAMAGE,-3);
        if(kind==6) stack.set(DataComponents.MAX_DAMAGE,0);
        stack.setCount(count);return stack;
    }
    static String snapshot(ItemStack stack) {
        return BuiltInRegistries.ITEM.getId(stack.getItem())+","+stack.getCount()+","+stack.getDamageValue()+","+stack.getMaxDamage()+","+stack.isDamageableItem()+","+stack.isBroken()+","+stack.nextDamageWillBreak();
    }
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var world=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var apply=ItemStack.class.getDeclaredMethod("applyDamage",int.class,ServerPlayer.class,java.util.function.Consumer.class);apply.setAccessible(true);
        for(int kind=0;kind<8;kind++) for(int count:new int[]{0,1,2}) for(int damage:new int[]{Integer.MIN_VALUE,-1,0,1,6,7,8,2031,Integer.MAX_VALUE}) for(boolean reenter:new boolean[]{false,true}) {
            var stack=fixture(kind,count);var before=snapshot(stack);var callbacks=new ArrayList<String>();
            java.util.function.Consumer<ItemStack> callback=broken->{
                var copyBefore=snapshot(broken);var liveBefore=snapshot(stack);
                if(reenter) { stack.setCount(2);stack.setDamageValue(1); }
                callbacks.add(copyBefore+"#"+liveBefore+"#"+snapshot(broken)+"#"+snapshot(stack)+"#"+(broken==stack));
            };
            apply.invoke(stack,damage,null,callback);
            System.out.println("damage\t"+kind+"\t"+count+"\t"+damage+"\t"+reenter+"\t"+before+"\t"+snapshot(stack)+"\t"+String.join(";",callbacks));
        }
    }
}
