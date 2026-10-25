import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.stats.*;
import net.minecraft.world.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.phys.*;

/** Actual block providers and menus. Only Player's external menu/stat consumers
 * are observed; native provider factories, slots and captured access run. */
public final class WorkstationVectors extends RedstoneIdentityVectors {
    WorkstationVectors(GameTestServer server,LevelStorageSource.LevelStorageAccess storage) { super(server,storage); }
    static void out(String... fields) { System.out.println("BUILDING\t"+String.join("\t",fields)); }
    static String title(MenuProvider provider) { return ((TranslatableContents)provider.getDisplayName().getContents()).getKey(); }
    static String describe(MenuProvider provider,AbstractContainerMenu menu,Level level) throws Exception {
        ContainerLevelAccess access=null;
        for(Class<?> type=menu.getClass();type!=null&&access==null;type=type.getSuperclass()) for(var field:type.getDeclaredFields()) if(field.getType()==ContainerLevelAccess.class) {
            field.setAccessible(true); access=(ContainerLevelAccess)field.get(menu); break;
        }
        if(access==null)throw new AssertionError("missing native captured access: "+menu.getClass());
        var captured=access.evaluate((original,pos)->original.dimension().identifier()+"@"+pos.toShortString(),"absent");
        return title(provider)+"\t"+BuiltInRegistries.MENU.getId(menu.getType())+"\t"+menu.slots.size()+"\t"+captured;
    }
    static final class Actor extends Player {
        String opened; int stat=-1,amount;
        Actor(Level level) { super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"workstation")); setPos(Vec3.atCenterOf(POS)); }
        @Override public GameType gameMode() { return GameType.SURVIVAL; }
        @Override public OptionalInt openMenu(MenuProvider provider) {
            try { opened=describe(provider,provider.createMenu(7,getInventory(),this),level()); }
            catch(Exception exception) { throw new RuntimeException(exception); }
            return OptionalInt.of(7);
        }
        @Override public void awardStat(Stat<?> stat,int amount) {
            if(opened==null)throw new AssertionError("stat before openMenu");
            if(stat.getType()!=Stats.CUSTOM)throw new AssertionError(stat);
            this.stat=BuiltInRegistries.CUSTOM_STAT.getId((Identifier)stat.getValue()); this.amount=amount;
        }
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"workstation-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var world=new WorkstationVectors(server,storage);
        for(var block:List.of(Blocks.CRAFTING_TABLE,Blocks.ANVIL,Blocks.CHIPPED_ANVIL,Blocks.DAMAGED_ANVIL,Blocks.LOOM,Blocks.STONECUTTER,Blocks.GRINDSTONE,Blocks.SMITHING_TABLE,Blocks.CARTOGRAPHY_TABLE)) for(var state:block.getStateDefinition().getPossibleStates()) {
            world.reset(); world.getChunkAt(POS).setBlockState(POS,state,816);
            var actor=new Actor(world); var provider=state.getMenuProvider(world,POS);
            out("provider",""+Block.getId(state),describe(provider,provider.createMenu(7,actor.getInventory(),actor),world));
            for(var face:Direction.values()) {
                actor=new Actor(world);
                var result=state.useWithoutItem(world,actor,new BlockHitResult(Vec3.atCenterOf(POS),face,POS,false));
                out("use",""+Block.getId(state),""+face.get3DDataValue(),""+(result==InteractionResult.SUCCESS),actor.opened,""+actor.stat,""+actor.amount);
            }
        }
        System.exit(0);
    }
}
final class WorkstationCapture {
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors);
        WorkstationVectors.run();
    }
}
