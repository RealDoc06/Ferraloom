import java.nio.file.*;
import java.util.*;
import java.util.stream.Collectors;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.server.level.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.effect.*;
import net.minecraft.world.item.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real Cow pairing and ongoing ServerEntity synchronization, including effects and leash state. */
public final class LivingTrackingVectors {
    static String packet(Packet<?> packet) {
        if (packet instanceof ClientboundSetEntityDataPacket data)
            return "data:"+data.packedItems().stream().map(v->v.id()+":"+
                (v.value() instanceof Holder<?> h?h.getRegisteredName():v.value().toString())).collect(Collectors.joining(","));
        if (packet instanceof ClientboundSetPassengersPacket p)
            return "passengers:"+p.getVehicle()+":"+Arrays.stream(p.getPassengers()).mapToObj(Integer::toString).collect(Collectors.joining(","));
        if (packet instanceof ClientboundSetEntityLinkPacket p) return "leash:"+p.getSourceId()+":"+p.getDestId();
        // UpdateMobEffect deliberately has no branch: pairing must not emit it.
        return TrackingVectors.packet(packet);
    }
    static String packets(List<String> values) { return values.isEmpty()?"-":String.join("|",values); }
    static final class Subject extends Cow {
        Subject(ServerLevel level) { super(EntityTypes.COW,level);setId(9);setPos(8.5,64,8.5);setYRot(0);setXRot(0);setYHeadRot(10); }
        void flying() {setSharedFlag(7,true);}
    }
    static ServerEntity tracker(Subject cow,List<String> output,boolean delta) {
        return new ServerEntity((ServerLevel)cow.level(),cow,UpdateInterval.periodic(3),delta,new ServerEntity.Synchronizer() {
            public void sendToTrackingPlayers(Packet<? super ClientGamePacketListener> p){output.add("T:"+packet(p));}
            public void sendToTrackingPlayersAndSelf(Packet<? super ClientGamePacketListener> p){output.add("S:"+packet(p));}
            public void sendToTrackingPlayersFiltered(Packet<? super ClientGamePacketListener> p,java.util.function.Predicate<ServerPlayer> f){throw new AssertionError();}
        });
    }
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();var level=AdvancementVectors.level;
        doReturn(new net.minecraft.util.debug.ServerDebugSubscribers(AdvancementVectors.server)).when(AdvancementVectors.server).debugSubscribers();
        doReturn(null).when(level).getEntity(anyInt());
        doReturn(Optional.empty()).when(level).findSupportingBlock(any(),any());
        doReturn(false).when(level).isClientSide();doReturn(123).when(level).getNextEntityId();
        var rows=new ArrayList<String>();
        for(String name:List.of("default","equipment","passengers","mounted","leash","all","delayed","effect","variant")) {
            var cow=new Subject(level);var other=new Subject(level);other.setId(12);
            if(name.equals("equipment")||name.equals("all")) {
                cow.setItemSlot(EquipmentSlot.MAINHAND,new ItemStack(Items.STONE,3));
                cow.setItemSlot(EquipmentSlot.OFFHAND,new ItemStack(Items.SHIELD));
                cow.setItemSlot(EquipmentSlot.CHEST,new ItemStack(Items.IRON_CHESTPLATE));
                cow.getAttribute(Attributes.MAX_HEALTH).setBaseValue(30);
            }
            if(name.equals("passengers")||name.equals("all")) other.startRiding(cow,true,false);
            if(name.equals("mounted")) cow.startRiding(other,true,false);
            if(name.equals("leash")||name.equals("all")) cow.setLeashedTo(other,false);
            if(name.equals("delayed")) cow.setDelayedLeashHolderId(12);
            if(name.equals("effect")) cow.getActiveEffectsMap().put(MobEffects.SPEED,new MobEffectInstance(MobEffects.SPEED,200,1));
            if(name.equals("variant")) cow.setVariant(AdvancementVectors.lookup.lookupOrThrow(Registries.COW_VARIANT).getOrThrow(net.minecraft.world.entity.animal.cow.CowVariants.COLD));
            var output=new ArrayList<String>();var tracker=tracker(cow,output,true);
            tracker.sendPairingData(null,p->output.add(packet(p)));
            rows.add("pairing\t"+name+"\t"+packets(output));
        }
        for(boolean delta:List.of(false,true)) {
            var cow=new Subject(level);var output=new ArrayList<String>();var tracker=tracker(cow,output,delta);
            for(int tick=0;tick<16;tick++) {
                if(tick==1){cow.setPos(8.625,64,8.5);cow.syncPosition=true;}
                if(tick==2){cow.setPos(8.75,64.125,8.5);cow.syncPosition=true;}
                if(tick==3)cow.setYHeadRot(23.9f);
                if(tick==4)cow.setHealth(7.5f);
                if(tick==5)cow.getAttribute(Attributes.MAX_HEALTH).setBaseValue(35);
                if(tick==6){cow.flying();cow.setDeltaMovement(0.125,0,0);}
                if(tick==7){cow.setPos(8.875,64.125,8.5);cow.syncPosition=true;}
                if(tick==8){cow.recordMovement(MoverType.PISTON,new Vec3(0.25,0,0));cow.setPos(9.125,64.125,8.5);}
                if(tick==10){cow.setOnGround(true);cow.needsSync=true;}
                if(tick==11){cow.setRequiresPrecisePosition(true);cow.setPos(9.25,64.125,8.5);cow.needsSync=true;}
                if(tick==12){cow.setRequiresPrecisePosition(false);cow.setDeltaMovement(Vec3.ZERO);cow.syncVelocity=true;}
                output.clear();tracker.sendChanges();
                rows.add("updates\t"+delta+"\t"+tick+"\t"+packets(output)+"\t"+cow.syncPosition);
            }
        }
        Files.write(Path.of(args[0]),rows);System.out.println(rows.size()+" living tracker observations");
    }
}
