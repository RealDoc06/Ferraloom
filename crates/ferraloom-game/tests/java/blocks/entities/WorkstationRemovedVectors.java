import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.util.Prediction;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real workstation removed, clearContainer and Inventory return methods. The
 * fixture captures connection sends and the entity factory boundary; it never
 * replaces menu or inventory algorithms. */
public final class WorkstationRemovedVectors {
    static int inputCount(String kind) {
        return switch(kind) {
            case "crafting" -> 9;
            case "anvil", "grindstone", "cartography" -> 2;
            case "loom", "smithing" -> 3;
            case "stonecutter" -> 1;
            default -> throw new AssertionError(kind);
        };
    }
    static AbstractContainerMenu menu(String kind, Player player, ContainerLevelAccess access) {
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
    // Controlled initial contents, before any tested callback: native class
    // storage is populated without inventing recipe calculation for the fixture.
    static void initial(Slot slot, ItemStack stack) throws Exception {
        for(Class<?> type=slot.container.getClass();type!=Object.class;type=type.getSuperclass()) {
            for(Field field:type.getDeclaredFields()) if(field.getType()==NonNullList.class) {
                field.setAccessible(true);
                @SuppressWarnings("unchecked") var items=(NonNullList<ItemStack>)field.get(slot.container);
                items.set(slot.container instanceof ResultContainer?0:slot.getContainerSlot(),stack);
                return;
            }
        }
        throw new AssertionError(slot.container.getClass());
    }
    static void scenario(RegistryAccess registries, String kind, boolean bound, String mode) throws Exception {
        var level=mock(ServerLevel.class);
        when(level.registryAccess()).thenReturn(registries);
        var recipes=mock(RecipeManager.class);
        when(recipes.propertySet(any())).thenReturn(RecipePropertySet.EMPTY);
        when(recipes.stonecutterRecipes()).thenReturn(SelectableRecipe.SingleInputSet.empty());
        when(level.recipeAccess()).thenReturn(recipes);
        var player=mock(ServerPlayer.class);
        when(player.level()).thenReturn(level);
        when(player.registryAccess()).thenReturn(registries);
        when(player.getAbilities()).thenReturn(new Abilities());
        var inventory=new Inventory(player,new EntityEquipment());
        when(player.getInventory()).thenReturn(inventory);
        doCallRealMethod().when(player).getEquipmentSlotForItem(any());
        when(player.isRemoved()).thenReturn(mode.equals("removed")||mode.equals("dimension"));
        when(player.getRemovalReason()).thenReturn(mode.equals("dimension")?Entity.RemovalReason.CHANGED_DIMENSION:Entity.RemovalReason.DISCARDED);
        when(player.hasDisconnected()).thenReturn(mode.equals("disconnected"));
        var events=new ArrayList<String>();var drops=new ArrayList<ItemStack>();var droppedReceivers=new ArrayList<ItemStack>();
        player.connection=mock(ServerGamePacketListenerImpl.class);
        doAnswer(call->{var packet=call.getArgument(0);if(packet instanceof ClientboundContainerClosePacket p)events.add("close:"+p.getContainerId());else if(packet instanceof ClientboundSetPlayerInventoryPacket p)events.add("slot:"+p.slot()+":"+InventoryVectors.item(p.contents()));else throw new AssertionError(packet);return null;}).when(player.connection).send(any(Packet.class));
        doAnswer(call->{ItemStack stack=call.getArgument(0);droppedReceivers.add(stack);drops.add(stack.copy());events.add("drop:"+InventoryVectors.item(stack));return null;}).when(player).drop(any(ItemStack.class),eq(false),eq(Prediction.SERVER_ONLY));
        var access=bound?ContainerLevelAccess.create(level,WorkstationAccessVectors.POS):ContainerLevelAccess.NULL;
        var menu=menu(kind,player,access);
        int count=inputCount(kind), offset=kind.equals("crafting")?1:0;
        var inputs=new ArrayList<ItemStack>();
        for(int slot=0;slot<count;slot++) {
            var stack=new ItemStack(slot%2==0?Items.STONE:Items.OAK_LOG,slot+3);
            inputs.add(stack);initial(menu.getSlot(slot+offset),stack);
        }
        int result=kind.equals("crafting")?0:count;
        initial(menu.getSlot(result),new ItemStack(Items.DIAMOND,2));
        var carried=new ItemStack(Items.STONE,5);menu.setCarried(carried);
        if(!mode.equals("space")&&!mode.equals("dimension")) {
            for(int slot=0;slot<36;slot++)inventory.setItem(slot,new ItemStack(Items.DIRT,64));
            if(mode.equals("partial"))inventory.setItem(0,new ItemStack(Items.STONE,62));
        }
        player.containerMenu=menu;
        var inventoryMenu=Player.class.getDeclaredField("inventoryMenu");inventoryMenu.setAccessible(true);inventoryMenu.set(player,new InventoryMenu(inventory,true,player));
        doCallRealMethod().when(player).closeContainer();doCallRealMethod().when(player).doCloseContainer();
        player.closeContainer();
        var visible=new ArrayList<String>();for(int slot=0;slot<36;slot++)if(!inventory.getItem(slot).isEmpty())visible.add(slot+":"+InventoryVectors.item(inventory.getItem(slot)));
        boolean identity=true;
        for(ItemStack stack:droppedReceivers)if(stack!=carried&&inputs.stream().noneMatch(input->input==stack))identity=false;
        System.out.println("removed\t"+kind+"\t"+bound+"\t"+mode+"\t"+String.join(";",events)+"\t"+InventoryVectors.slots(menu)+"\t"+InventoryVectors.item(menu.getCarried())+"\t"+String.join(";",visible)+"\t"+String.join(";",drops.stream().map(InventoryVectors::item).toList())+"\t"+identity+"\t"+(player.containerMenu==player.inventoryMenu));
    }
    static void grid(boolean bound, boolean placing) throws Exception {
        var level=mock(ServerLevel.class);var server=mock(MinecraftServer.class);var recipes=mock(RecipeManager.class);
        when(level.getServer()).thenReturn(server);when(server.getRecipeManager()).thenReturn(recipes);
        var player=mock(ServerPlayer.class);var inventory=new Inventory(player,new EntityEquipment());
        when(player.getInventory()).thenReturn(inventory);when(player.level()).thenReturn(level);
        player.connection=mock(ServerGamePacketListenerImpl.class);var events=new ArrayList<String>();
        doAnswer(call->{var packet=(ClientboundContainerSetSlotPacket)call.getArgument(0);events.add("result:"+packet.getStateId()+":"+packet.getSlot()+":"+InventoryVectors.item(packet.getItem()));return null;}).when(player.connection).send(any(Packet.class));
        var menu=new CraftingMenu(7,inventory,bound?ContainerLevelAccess.create(level,WorkstationAccessVectors.POS):ContainerLevelAccess.NULL);
        initial(menu.getSlot(0),new ItemStack(Items.DIAMOND,2));
        if(placing)menu.beginPlacingRecipe();
        menu.slotsChanged(menu.getSlot(1).container);
        String before=String.join(";",events), result=InventoryVectors.item(menu.getSlot(0).getItem());
        menu.finishPlacingRecipe(level,null);
        System.out.println("grid\t"+bound+"\t"+placing+"\t"+before+"\t"+result+"\t"+String.join(";",events)+"\t"+InventoryVectors.item(menu.getSlot(0).getItem()));
    }
    public static void main(String[] args) throws Exception {
        var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(DataComponentInitializers.PendingComponents::apply);
        for(String kind:List.of("crafting","anvil","loom","stonecutter","grindstone","smithing","cartography"))
            for(boolean bound:new boolean[]{true,false})
                for(String mode:List.of("space","full","partial","removed","dimension","disconnected"))
                    scenario(registries,kind,bound,mode);
        for(boolean bound:new boolean[]{true,false})for(boolean placing:new boolean[]{true,false})grid(bound,placing);
    }
}
