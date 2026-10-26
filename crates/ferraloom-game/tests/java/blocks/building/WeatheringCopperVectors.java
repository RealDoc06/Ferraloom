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
import net.minecraft.world.item.HoneycombItem;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Actual shared aging methods and per-class random ticks. Fixtures contain
 * real blocks and real block entities; chest opener behavior is not replaced. */
final class WeatheringCopperVectors extends AnvilVectors {
    final List<String> trace = new ArrayList<>();
    boolean observe;
    WeatheringCopperVectors(GameTestServer server, LevelStorageSource.LevelStorageAccess storage) { super(server,storage); }
    @Override public void sendBlockUpdated(BlockPos pos,BlockState old,BlockState state,int flags) {
        if(observe)trace.add(at(pos)+":"+Block.getId(old)+">"+Block.getId(state));
    }
    static String at(BlockPos pos) { return pos.getX()+","+pos.getY()+","+pos.getZ(); }
    static List<Block> copper() {
        var result=new ArrayList<Block>();
        for(var block:BuiltInRegistries.BLOCK)if(block instanceof WeatheringCopper)result.add(block);
        return result;
    }
    static Block[] stages() {
        var stages=new Block[4];
        for(var block:copper())if(block instanceof WeatheringCopperFullBlock copper&&stages[copper.getAge().ordinal()]==null)stages[copper.getAge().ordinal()]=block;
        return stages;
    }
    static LinkedHashMap<BlockPos,BlockState> footprint(BlockState state,int pattern) {
        var footprint=new LinkedHashMap<BlockPos,BlockState>();
        footprint.put(POS.below(),Blocks.STONE.defaultBlockState());footprint.put(POS,state);
        if(state.getBlock() instanceof DoorBlock) {
            if(state.getValue(DoorBlock.HALF)==DoubleBlockHalf.LOWER)footprint.put(POS.above(),state.setValue(DoorBlock.HALF,DoubleBlockHalf.UPPER));
            else {footprint.put(POS.below(),state.setValue(DoorBlock.HALF,DoubleBlockHalf.LOWER));footprint.put(POS.below(2),Blocks.STONE.defaultBlockState());}
        }
        if(state.getBlock() instanceof LanternBlock&&state.getValue(LanternBlock.HANGING))footprint.put(POS.above(),Blocks.STONE.defaultBlockState());
        int own=((WeatheringCopper)state.getBlock()).getAge().ordinal();var stages=stages();
        var offsets=List.of(new BlockPos(2,0,0),new BlockPos(-2,0,0),new BlockPos(0,2,0),new BlockPos(0,-2,0),new BlockPos(0,0,2),new BlockPos(0,0,-2));
        for(int i=0;i<offsets.size();i++) {
            var delta=offsets.get(i);Block neighbor=null;
            switch(pattern) {
                case 0 -> {}
                case 1 -> neighbor=stages[own];
                case 2 -> neighbor=stages[Math.min(3,own+1)];
                case 3 -> neighbor=stages[Math.max(0,own-1)];
                case 4 -> neighbor=stages[i%2==0?own:Math.min(3,own+1)];
                case 5 -> neighbor=HoneycombItem.WAXABLES.get().get(stages[Math.max(0,own-1)]);
                case 6 -> {}
                case 7 -> {}
                case 8 -> neighbor=copper().get((i*11+own)%copper().size());
                default -> throw new AssertionError(pattern);
            }
            if(neighbor!=null)footprint.put(POS.offset(delta),neighbor.defaultBlockState());
        }
        if(pattern==6)footprint.put(POS.east(4),stages[Math.max(0,own-1)].defaultBlockState());
        if(pattern==7){footprint.put(POS.east(5),stages[Math.max(0,own-1)].defaultBlockState());footprint.put(POS.west(4),stages[Math.min(3,own+1)].defaultBlockState());}
        return footprint;
    }
    void begin(LinkedHashMap<BlockPos,BlockState> footprint) {
        observe=false;prepare(Blocks.AIR.defaultBlockState(),Blocks.AIR.defaultBlockState());
        for(var entry:footprint.entrySet())getChunkAt(entry.getKey()).setBlockState(entry.getKey(),entry.getValue(),816);
        trace.clear();observe=true;
    }
    boolean present() { return ((LevelChunk)getChunkAt(POS)).getBlockEntities().containsKey(POS); }
    String world(Collection<BlockPos> positions) {
        var result=new ArrayList<String>();
        for(var pos:positions)result.add(at(pos)+":"+Block.getId(getBlockState(pos)));
        return String.join(";",result);
    }
    void sample(String mode,BlockState state,int pattern,long seed,BlockPos callPos,int guard) {
        var setup=footprint(state,pattern);
        if(guard==1)setup.put(POS,Blocks.STONE.defaultBlockState());
        if(guard==2)setup.put(POS,Blocks.FURNACE.defaultBlockState());
        begin(setup);
        if(guard==3)removeBlockEntity(POS);
        var initial=world(setup.keySet());var before=((LevelChunk)getChunkAt(POS)).getBlockEntities().get(POS);
        trace.clear();random.setSeed(seed);int result=-1;
        var receiver=(WeatheringCopper)state.getBlock();
        switch(mode) {
            case "next" -> result=receiver.getNextState(state,this,callPos,random).map(Block::getId).orElse(-1);
            case "change" -> receiver.changeOverTime(state,this,callPos,random);
            case "random", "guard" -> state.randomTick(this,callPos,random);
            default -> throw new AssertionError(mode);
        }
        observe=false;int chunksAfter=chunks.size();
        var positions=new LinkedHashSet<>(setup.keySet());
        for(var write:trace){var xyz=write.split(":")[0].split(",");positions.add(new BlockPos(Integer.parseInt(xyz[0]),Integer.parseInt(xyz[1]),Integer.parseInt(xyz[2])));}
        var after=((LevelChunk)getChunkAt(POS)).getBlockEntities().get(POS);
        out(mode,""+Block.getId(state),at(callPos),""+seed,""+guard,initial,""+result,
            world(positions),String.join(";",trace),""+present(),""+(before==after),""+chunksAfter,""+random.nextLong());
    }
    @Override void capture() {
        for(var block:copper())for(var state:block.getStateDefinition().getPossibleStates()) {
            for(long seed:new long[]{0,4096})sample("next",state,0,seed,POS,0);
            if(!(block instanceof WeatheringCopperChestBlock)) {
                sample("change",state,0,107048004364969L,POS,0);
                sample("random",state,0,107048004364969L,POS,0);
            }
        }
        for(var block:copper())for(int pattern=0;pattern<=8;pattern++)for(int index=0;index<12;index++) {
            long seed=index*0x9e3779b97f4a7c15L;var state=block.defaultBlockState();
            sample("next",state,pattern,seed,POS,0);
            if(!(block instanceof WeatheringCopperChestBlock)) {
                sample("change",state,pattern,seed,POS,0);
                sample("random",state,pattern,seed,POS,0);
            }
        }
        for(var block:copper())for(var state:block.getStateDefinition().getPossibleStates()) {
            if(block instanceof WeatheringCopperDoorBlock&&state.getValue(DoorBlock.HALF)==DoubleBlockHalf.UPPER)
                sample("guard",state,0,0,new BlockPos(82,64,-24),0);
            if(block instanceof WeatheringCopperChestBlock) {
                if(state.getValue(ChestBlock.TYPE)==ChestType.RIGHT) {
                    sample("guard",state,0,0,POS,3);
                    sample("guard",state,0,0,new BlockPos(82,64,-24),3);
                } else {
                    sample("guard",state,0,0,POS,1);
                    sample("guard",state,0,0,POS,2);
                }
            }
        }
    }
    static void run()throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"weathering-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        new WeatheringCopperVectors(server,storage).capture();System.exit(0);
    }
}
final class WeatheringCopperCapture {
    public static void main(String[] args)throws Exception {
        var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);WeatheringCopperVectors.run();
    }
}
