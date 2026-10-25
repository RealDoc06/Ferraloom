import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.inventory.*;
import net.minecraft.world.level.block.*;

/** Real menus retain their original ContainerLevelAccess while the player walks
 * and the captured workstation cell changes underneath the same menu. */
public final class WorkstationAccessVectors {
    static final BlockPos POS = new BlockPos(8,64,8);
    static AbstractContainerMenu menu(String kind, InventoryVectors.MenuPlayer player, ContainerLevelAccess access) {
        var inventory=player.getInventory();
        return switch(kind) {
            case "crafting" -> new CraftingMenu(7,inventory,access);
            case "anvil" -> new AnvilMenu(7,inventory,access);
            case "loom" -> new LoomMenu(7,inventory,access);
            case "stonecutter" -> new StonecutterMenu(7,inventory,access);
            case "grindstone" -> new GrindstoneMenu(7,inventory,access);
            case "smithing" -> new SmithingMenu(7,inventory,access);
            case "cartography" -> new CartographyTableMenu(7,inventory,access);
            default -> throw new AssertionError(kind);
        };
    }
    static Block block(String kind) {
        return switch(kind) {
            case "crafting" -> Blocks.CRAFTING_TABLE;
            case "anvil" -> Blocks.ANVIL;
            case "loom" -> Blocks.LOOM;
            case "stonecutter" -> Blocks.STONECUTTER;
            case "grindstone" -> Blocks.GRINDSTONE;
            case "smithing" -> Blocks.SMITHING_TABLE;
            case "cartography" -> Blocks.CARTOGRAPHY_TABLE;
            default -> throw new AssertionError(kind);
        };
    }
    static void query(String kind, boolean bound, double range, double x, Block block,
                      InventoryVectors.MenuLevel level, InventoryVectors.MenuPlayer player, AbstractContainerMenu menu) {
        var state=block.defaultBlockState();
        level.getChunkAt(POS).getSection(level.getSectionIndex(POS.getY())).setBlockState(8,0,8,state);
        player.setPos(x,63.0,8.5); // Eyes lie within the block's vertical interval.
        System.out.println("access\t"+kind+"\t"+bound+"\t"+Long.toHexString(Double.doubleToRawLongBits(range))
            +"\t"+Long.toHexString(Double.doubleToRawLongBits(x))+"\t"+Block.getId(state)+"\t"+menu.stillValid(player));
    }
    public static void main(String[] args) {
        var output=System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(DataComponentInitializers.PendingComponents::apply);
        var level=new InventoryVectors.MenuLevel(registries);
        var player=new InventoryVectors.MenuPlayer(level,false);
        for(String kind:List.of("crafting","anvil","loom","stonecutter","grindstone","smithing","cartography")) {
            for(double range:new double[]{1.0,4.5}) {
                player.getAttribute(Attributes.BLOCK_INTERACTION_RANGE).setBaseValue(range);
                var menu=menu(kind,player,ContainerLevelAccess.create(level,POS));
                double boundary=9.0+range+4.0;
                for(Block cell:List.of(block(kind),Blocks.AIR,Blocks.STONE))
                    for(double x:new double[]{8.5,Math.nextDown(boundary),boundary,Math.nextUp(boundary),boundary+10.0})
                        query(kind,true,range,x,cell,level,player,menu);
                if(kind.equals("anvil"))for(Block cell:List.of(Blocks.CHIPPED_ANVIL,Blocks.DAMAGED_ANVIL))
                    for(double x:new double[]{8.5,Math.nextDown(boundary),boundary,Math.nextUp(boundary),boundary+10.0})
                        query(kind,true,range,x,cell,level,player,menu);
            }
            var unbound=menu(kind,player,ContainerLevelAccess.NULL);
            for(Block cell:List.of(block(kind),Blocks.AIR))for(double x:new double[]{8.5,100.0})
                query(kind,false,4.5,x,cell,level,player,unbound);
        }
    }
}
