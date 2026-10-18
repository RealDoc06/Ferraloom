import java.nio.file.*;
import java.util.*;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.tags.TagKey;
import net.minecraft.world.level.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.PiecesContainer;
import net.minecraft.world.level.levelgen.structure.structures.BuriedTreasurePieces.BuriedTreasurePiece;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Native StructureManager against real chunk maps, registries and datapack tags. */
class StructureManagerVectors {
    static StructureStart start(Structure structure, int x, int z, int marker, boolean valid) {
        var pieces=new ArrayList<StructurePiece>();
        if(valid) for(int[] box:List.of(new int[]{0,60,0,2,62,2},new int[]{8,60,0,10,62,2})) {
            var tag=new CompoundTag(); tag.putIntArray("BB",box); tag.putInt("GD",0); tag.putInt("O",-1);
            pieces.add(new BuriedTreasurePiece(tag));
        }
        return new StructureStart(structure,new ChunkPos(x,z),marker,new PiecesContainer(pieces));
    }
    static void run() throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory("structure-manager")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,
            ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()) {});
        var level=new DestroyBlockVectors(server,storage);
        var registry=server.registryAccess().lookupOrThrow(Registries.STRUCTURE);
        var structure=registry.getValue(Identifier.parse("minecraft:buried_treasure"));
        var holder=registry.getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.STRUCTURE,Identifier.parse("minecraft:buried_treasure")));
        var manager=level.structureManager();
        long[] sources={ChunkPos.pack(0,0),ChunkPos.pack(1,0),ChunkPos.pack(-1,1),ChunkPos.pack(2,-2),ChunkPos.pack(-3,-1)};
        for(int i=0;i<sources.length;i++) {
            var pos=ChunkPos.unpack(sources[i]);
            var chunk=level.getChunk(pos.x(),pos.z(),ChunkStatus.FULL,true);
            if(i!=3) chunk.setAllStarts(Map.of(structure,start(structure,pos.x(),pos.z(),10+i,i!=2)));
        }
        var refs=new LongOpenHashSet(sources);
        level.getChunk(0,0,ChunkStatus.FULL,true).setAllReferences(Map.of(structure,refs));
        var referenceOrder=new ArrayList<String>();
        var iterator=refs.iterator(); while(iterator.hasNext()) referenceOrder.add(Long.toString(iterator.nextLong()));
        System.out.println("VECTOR\trefs\t"+String.join(",",referenceOrder));
        System.out.println("VECTOR\tstarts\t"+String.join(",",manager.startsForStructure(0,0,structure).stream().map(s->""+s.getReferences()).toList()));
        System.out.println("VECTOR\tstart-nbt\t"+start(structure,0,0,10,true).createTag(net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext.fromLevel(level),new ChunkPos(0,0)));
        var serialization=net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext.fromLevel(level);
        var pack=net.minecraft.world.level.chunk.storage.SerializableChunkData.class.getDeclaredMethod("packStructureData",serialization.getClass(),ChunkPos.class,Map.class,Map.class);
        var unpackStarts=net.minecraft.world.level.chunk.storage.SerializableChunkData.class.getDeclaredMethod("unpackStructureStart",serialization.getClass(),CompoundTag.class,long.class);
        var unpackReferences=net.minecraft.world.level.chunk.storage.SerializableChunkData.class.getDeclaredMethod("unpackStructureReferences",RegistryAccess.class,ChunkPos.class,CompoundTag.class);
        pack.setAccessible(true); unpackStarts.setAccessible(true); unpackReferences.setAccessible(true);
        var origin=new ChunkPos(0,0);
        var originChunk=level.getChunk(0,0,ChunkStatus.FULL,true);
        var saved=(CompoundTag)pack.invoke(null,serialization,origin,originChunk.getAllStarts(),originChunk.getAllReferences());
        var restoredStarts=unpackStarts.invoke(null,serialization,saved,0L);
        var restoredReferences=unpackReferences.invoke(null,server.registryAccess(),origin,saved);
        System.out.println("VECTOR\tsave-nbt\t"+saved);
        System.out.println("VECTOR\treload-nbt\t"+pack.invoke(null,serialization,origin,restoredStarts,restoredReferences));
        var direct=HolderSet.direct(holder);
        var treasure=registry.getOrThrow(TagKey.create(Registries.STRUCTURE,Identifier.parse("minecraft:on_treasure_maps")));
        var other=registry.getOrThrow(TagKey.create(Registries.STRUCTURE,Identifier.parse("minecraft:on_mineshaft_maps")));
        for(int[] p:List.of(new int[]{0,60,0},new int[]{2,62,2},new int[]{3,61,1},new int[]{7,61,1},new int[]{8,60,0},new int[]{10,62,2},new int[]{11,61,1},new int[]{1,59,1},new int[]{1,63,1},new int[]{-1,61,1})) {
            var pos=new BlockPos(p[0],p[1],p[2]);
            System.out.println("VECTOR\tquery\t"+p[0]+","+p[1]+","+p[2]+"\t"+
                manager.getStructureAt(pos,structure).isValid()+","+manager.getStructureWithPieceAt(p[0],p[1],p[2],structure).isValid()+","+
                manager.getStructureAt(pos,direct).isValid()+","+manager.getStructureWithPieceAt(pos,direct).isValid()+","+
                manager.getStructureWithPieceAt(pos,treasure).isValid()+","+manager.getStructureWithPieceAt(pos,other).isValid());
        }
        // A reference-map key with an empty value is still present in getAllStructuresAt.
        var chunk=level.getChunk(0,0,ChunkStatus.FULL,true);
        chunk.setAllReferences(Map.of(structure,new LongOpenHashSet()));
        System.out.println("VECTOR\tempty-value\t"+!manager.getAllStructuresAt(new BlockPos(0,60,0)).isEmpty()+","+manager.startsForStructure(0,0,structure).size());
        chunk.setAllReferences(Map.of());
        System.out.println("VECTOR\tempty-map\t"+!manager.getAllStructuresAt(new BlockPos(0,60,0)).isEmpty()+","+manager.getStructureWithPieceAt(new BlockPos(0,60,0),treasure).isValid());
        // A terrain-adapted start's inflated aggregate box never inflates its pieces.
        structure=registry.getValue(Identifier.parse("minecraft:pillager_outpost"));
        chunk.setAllStarts(Map.of(structure,start(structure,0,0,20,true)));
        chunk.setAllReferences(Map.of(structure,new LongOpenHashSet(new long[]{0})));
        var pos=new BlockPos(4,50,1);
        System.out.println("VECTOR\tadapted\t"+manager.getStructureAt(pos,structure).isValid()+","+manager.getStructureWithPieceAt(4,50,1,structure).isValid());
        System.out.flush(); System.exit(0);
    }
    public static void main(String[] args) throws Exception {
        var output=System.out; var errors=System.err;
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        System.setOut(output); System.setErr(errors); run();
    }
}
