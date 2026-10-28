import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.tags.*;
import net.minecraft.world.Container;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Actual ListBackedContainer admission and HopperBlockEntity's admission
 * helper across live tag reloads, including retained empty stack payloads. */
public final class HopperAdmissionVectors {
    static void capture() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"hopper-admission")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var level=new RedstoneIdentityVectors(server,storage);
        var shelf=new ChiseledBookShelfBlockEntity(BlockPos.ZERO,Blocks.CHISELED_BOOKSHELF.defaultBlockState());shelf.setLevel(level);
        var registry=BuiltInRegistries.ITEM;
        Map<TagKey<Item>,List<Holder<Item>>> original=new HashMap<>();
        registry.getTags().forEach(tag->original.put(tag.key(),tag.stream().toList()));
        var hopper=HopperBlockEntity.class.getDeclaredMethod("canPlaceItemInContainer",Container.class,ItemStack.class,int.class,Direction.class);hopper.setAccessible(true);
        var stored=List.of(new ItemStack(Items.AIR,1),new ItemStack(Items.BOOK,1),new ItemStack(Items.BOOK,1),new ItemStack(Items.BOOK,1),new ItemStack(Items.BOOK,2),new ItemStack(Items.STONE,1),new ItemStack(Items.STONE,1),new ItemStack(Items.STONE,1));
        int[] storedCounts={0,-1,0,1,2,1,0,-2};
        for(int i=0;i<stored.size();i++)stored.get(i).setCount(storedCounts[i]);
        for(var item:List.of(Items.BOOK,Items.WRITABLE_BOOK,Items.WRITTEN_BOOK,Items.ENCHANTED_BOOK,Items.KNOWLEDGE_BOOK,Items.STONE,Items.AIR,Items.NETHER_STAR))for(int count:new int[]{-2,0,1,2,64}) {
            // Keep this same receiver alive while its registry holder's tag set changes.
            var incoming=new ItemStack(item,1);incoming.setCount(count);
            for(int held=0;held<stored.size();held++)for(int slot=-1;slot<=6;slot++)for(int mode=0;mode<6;mode++) {
                var tags=new HashMap<>(original);
                if(mode!=0&&mode!=5)tags.put(ItemTags.BOOKSHELF_BOOKS,switch(mode){
                    case 1->List.of(Items.STONE.builtInRegistryHolder());
                    case 2->List.of();
                    case 3->List.of(Items.AIR.builtInRegistryHolder(),Items.BOOK.builtInRegistryHolder());
                    case 4->List.of(Items.STONE.builtInRegistryHolder(),Items.BOOK.builtInRegistryHolder());
                    default->throw new AssertionError(mode);
                });
                registry.prepareTagReload(new TagLoader.LoadResult<>(Registries.ITEM,tags)).apply();
                Collections.fill(shelf.getItems(),ItemStack.EMPTY);
                shelf.getItems().set(slot>=0&&slot<6?slot:0,stored.get(held));
                String direct,throughHopper;
                try{direct=""+shelf.canPlaceItem(slot,incoming);}catch(IndexOutOfBoundsException error){direct="slot-error";}
                try{throughHopper=""+hopper.invoke(null,shelf,incoming,slot,null);}catch(java.lang.reflect.InvocationTargetException error){if(error.getCause() instanceof IndexOutOfBoundsException)throughHopper="slot-error";else throw error;}
                System.out.println("ADMISSION\t"+BuiltInRegistries.ITEM.getId(item)+"\t"+count+"\t"+held+"\t"+slot+"\t"+mode+"\t"+direct+"\t"+throughHopper);
            }
        }
        registry.prepareTagReload(new TagLoader.LoadResult<>(Registries.ITEM,original)).apply();
        System.exit(0);
    }
    public static void main(String[] args)throws Exception {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);capture();
    }
}
