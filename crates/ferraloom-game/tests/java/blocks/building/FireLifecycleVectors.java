import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.server.notifications.EmptyNotificationService;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.level.portal.PortalShape;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.ticks.*;
import net.minecraft.world.phys.Vec3;

/** Actual fire/lava virtuals and PortalShape algorithms. The fixture controls
 * chunk availability and weather; native block callbacks and Level RNG run. */
public final class FireLifecycleVectors extends AnvilVectors {
    final List<String> writes = new ArrayList<>();
    net.minecraft.world.attribute.EnvironmentAttributeSystem environment;
    net.minecraft.resources.ResourceKey<Level> dimension;
    boolean observe;
    boolean rainy;
    FireLifecycleVectors(GameTestServer server,LevelStorageSource.LevelStorageAccess storage) { super(server,storage); }
    @Override public void sendBlockUpdated(BlockPos pos,BlockState old,BlockState state,int flags) {
        if(observe)writes.add(pos.getX()+","+pos.getY()+","+pos.getZ()+":"+Block.getId(state));
    }
    @Override public boolean hasChunk(int x,int z) { return chunks.containsKey(new ChunkPos(x,z)); }
    @Override public boolean isLoaded(BlockPos pos) { return isInValidBounds(pos)&&hasChunk(pos.getX()>>4,pos.getZ()>>4); }
    @Override public boolean isRaining() { return rainy; }
    @Override public boolean isRainingAt(BlockPos pos) { return rainy&&super.isRainingAt(pos); }
    @Override public int getBrightness(LightLayer layer,BlockPos pos) { return rainy&&layer==LightLayer.SKY?15:super.getBrightness(layer,pos); }
    @Override public Holder<net.minecraft.world.level.biome.Biome> getBiome(BlockPos pos) { return rainy?registryAccess().lookupOrThrow(net.minecraft.core.registries.Registries.BIOME).getOrThrow(net.minecraft.world.level.biome.Biomes.PLAINS):super.getBiome(pos); }
    @Override public net.minecraft.world.attribute.EnvironmentAttributeSystem environmentAttributes() { return environment==null?super.environmentAttributes():environment; }
    @Override public net.minecraft.resources.ResourceKey<Level> dimension() { return dimension==null?super.dimension():dimension; }
    void begin(BlockState state,BlockState below) {
        observe=false;prepare(state,below);writes.clear();
        getGameRules().set(GameRules.FIRE_SPREAD_RADIUS_AROUND_PLAYER,-1,null);
    }
    void raw(BlockPos pos,BlockState state) { getChunkAt(pos).setBlockState(pos,state,816); }
    static Field field(Class<?> type,String name)throws Exception { var value=type.getDeclaredField(name);value.setAccessible(true);return value; }
    static String shape(PortalShape shape)throws Exception {
        var pos=(BlockPos)field(PortalShape.class,"bottomLeft").get(shape);
        return field(PortalShape.class,"axis").get(shape)+":"+pos.getX()+","+pos.getY()+","+pos.getZ()+":"+field(PortalShape.class,"width").getInt(shape)+","+field(PortalShape.class,"height").getInt(shape)+","+field(PortalShape.class,"numPortalBlocks").getInt(shape);
    }
    void rules()throws Exception {
        begin(Blocks.AIR.defaultBlockState(),Blocks.AIR.defaultBlockState());
        var map=(PlayerMap)field(ChunkMap.class,"playerMap").get(getChunkSource().chunkMap);
        var player=new ServerPlayer(getServer(),this,new com.mojang.authlib.GameProfile(new UUID(1,2),"fire-radius"),ClientInformation.createDefault());
        var mode=field(ServerPlayerGameMode.class,"gameModeForPlayer");
        double[][] offsets={{0,0,0},{.5,.5,.5},{Math.nextDown(9.0)-8,0,0},{1,0,0},{Math.nextUp(9.0)-8,0,0},{0,Math.nextDown(72.0)-64,0},{0,8,0},{0,Math.nextUp(72.0)-64,0},{3,4,0},{3,4,5},{127,0,0},{128,0,0},{Math.nextDown(136.0)-8,0,0},{Math.nextUp(136.0)-8,0,0},{0,0,128},{-128,0,0}};
        for(int radius:new int[]{-1,0,1,5,8,128,Integer.MAX_VALUE})for(int index=0;index<offsets.length;index++)for(boolean member:new boolean[]{false,true})for(boolean ignored:new boolean[]{false,true})for(boolean spectator:new boolean[]{false,true}) {
            var offset=offsets[index];var at=new Vec3(POS.getX()+offset[0],POS.getY()+offset[1],POS.getZ()+offset[2]);
            player.setPos(at);mode.set(player.gameMode,spectator?GameType.SPECTATOR:GameType.SURVIVAL);
            map.removePlayer(player);if(member)map.addPlayer(player,ignored);
            getGameRules().set(GameRules.FIRE_SPREAD_RADIUS_AROUND_PLAYER,radius,null);
            out("radius",""+radius,""+Double.doubleToRawLongBits(at.x),""+Double.doubleToRawLongBits(at.y),""+Double.doubleToRawLongBits(at.z),""+member,""+ignored,""+spectator,""+canSpreadFireAround(POS));
        }
        map.removePlayer(player);
    }
    void portals()throws Exception {
        for(var axis:new Direction.Axis[]{Direction.Axis.X,Direction.Axis.Z})for(int width:new int[]{1,2,3,20,21,22})for(int height:new int[]{2,3,4,20,21,22})for(int pattern=0;pattern<9;pattern++)for(boolean high:new boolean[]{false,true})for(var preferred:new Direction.Axis[]{Direction.Axis.X,Direction.Axis.Z}) {
            begin(Blocks.AIR.defaultBlockState(),Blocks.AIR.defaultBlockState());
            var along=axis==Direction.Axis.X?Direction.EAST:Direction.SOUTH;
            for(int x=-1;x<=width;x++)for(int y=-1;y<=height;y++)if(x==-1||x==width||y==-1||y==height)raw(POS.relative(along,x).above(y),Blocks.OBSIDIAN.defaultBlockState());
            if(pattern==1)raw(POS.relative(along,width/2).below(),Blocks.AIR.defaultBlockState());
            if(pattern==2)raw(POS.relative(along,-1).above(height/2),Blocks.AIR.defaultBlockState());
            if(pattern==3)raw(POS.relative(along,width/2).above(height),Blocks.AIR.defaultBlockState());
            if(pattern>=4&&pattern<=7)raw(POS.relative(along,width/2).above(height/2),List.of(Blocks.STONE,Blocks.NETHER_PORTAL,Blocks.FIRE,Blocks.SOUL_FIRE).get(pattern-4).defaultBlockState());
            if(pattern==8)for(int x:new int[]{-1,width})for(int y:new int[]{-1,height})raw(POS.relative(along,x).above(y),Blocks.AIR.defaultBlockState());
            var start=high?POS.relative(along,width/2).above(height/2):POS;
            var any=PortalShape.findAnyShape(this,start,preferred);var empty=PortalShape.findEmptyPortalShape(this,start,preferred);
            observe=true;if(empty.isPresent())empty.get().createPortalBlocks(this);observe=false;
            out("portal",""+axis,""+width,""+height,""+pattern,""+high,""+preferred,shape(any),empty.isEmpty()?"-":shape(empty.get()),String.join(";",writes));
        }
    }
    String scheduled()throws Exception {
        var all=(it.unimi.dsi.fastutil.longs.Long2ObjectMap<LevelChunkTicks<Block>>)field(LevelTicks.class,"allContainers").get(ticks);
        return all.values().stream().flatMap(LevelChunkTicks::getAll).map(tick->tick.pos().getX()+","+tick.pos().getY()+","+tick.pos().getZ()+":"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getId(tick.type())+":"+tick.triggerTick()).sorted().collect(java.util.stream.Collectors.joining(";"));
    }
    void lava()throws Exception {
        for(int pattern=0;pattern<6;pattern++)for(int radius:new int[]{-1,0})for(int seed=0;seed<48;seed++) {
            begin(Blocks.LAVA.defaultBlockState(),Blocks.STONE.defaultBlockState());
            for(int x=-2;x<=2;x++)for(int z=-2;z<=2;z++) {
                var at=POS.offset(x,0,z);
                if(pattern==1||pattern==2||pattern==4)raw(at,pattern==2?Blocks.OAK_FENCE.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true):Blocks.OAK_PLANKS.defaultBlockState());
                if(pattern==3)raw(at.above(),Blocks.STONE.defaultBlockState());
                if(pattern==4)raw(at.below(),Blocks.SOUL_SOIL.defaultBlockState());
            }
            raw(POS,Blocks.LAVA.defaultBlockState());
            var origin=pattern==5?new BlockPos(0,64,0):POS;
            if(pattern==5){raw(POS,Blocks.AIR.defaultBlockState());raw(POS.below(),Blocks.AIR.defaultBlockState());raw(origin,Blocks.LAVA.defaultBlockState());raw(origin.below(),Blocks.STONE.defaultBlockState());chunks.remove(new ChunkPos(-1,0));chunks.remove(new ChunkPos(0,-1));chunks.remove(new ChunkPos(-1,-1));}
            getGameRules().set(GameRules.FIRE_SPREAD_RADIUS_AROUND_PLAYER,radius,null);
            random.setSeed(seed);observe=true;
            Fluids.LAVA.defaultFluidState().randomTick(this,origin,random);
            observe=false;
            out("lava",""+pattern,""+radius,""+seed,String.join(";",writes),scheduled(),""+random.nextLong());
        }
    }
    void fireTicks(boolean rainy)throws Exception {
        this.rainy=rainy;
        for(var below:List.of(Blocks.AIR,Blocks.STONE,Blocks.OAK_PLANKS,Blocks.NETHERRACK))for(int age:new int[]{0,3,4,14,15})for(boolean neighbors:new boolean[]{false,true})for(int radius:rainy?new int[]{-1}:new int[]{-1,0})for(boolean increased:new boolean[]{false,true})for(int seed=0;seed<12;seed++) {
            var state=Blocks.FIRE.defaultBlockState().setValue(FireBlock.AGE,age);
            begin(state,below.defaultBlockState());
            if(neighbors)for(var side:Direction.values())if(side!=Direction.DOWN)raw(POS.relative(side),Blocks.OAK_PLANKS.defaultBlockState());
            getGameRules().set(GameRules.FIRE_SPREAD_RADIUS_AROUND_PLAYER,radius,null);
            environment=net.minecraft.world.attribute.EnvironmentAttributeSystem.builder().addConstantLayer(net.minecraft.world.attribute.EnvironmentAttributes.INCREASED_FIRE_BURNOUT, old->increased).build();
            random.setSeed(seed);observe=true;state.tick(this,POS,random);observe=false;
            out(rainy?"rain":"tick",""+Block.getId(below.defaultBlockState()),""+age,""+neighbors,""+radius,""+increased,""+seed,""+Block.getId(getBlockState(POS)),String.join(";",writes),scheduled(),""+random.nextLong());
        }
    }
    void placing()throws Exception {
        for(var dimension:List.of(Level.OVERWORLD,Level.NETHER,Level.END))for(var block:List.of(Blocks.FIRE,Blocks.SOUL_FIRE))for(var old:List.of(Blocks.AIR,Blocks.FIRE,Blocks.SOUL_FIRE))for(var below:List.of(Blocks.AIR,Blocks.STONE,Blocks.OAK_PLANKS,Blocks.SOUL_SOIL))for(int frame=0;frame<3;frame++)for(boolean moved:new boolean[]{false,true})for(int seed:new int[]{0,42}) {
            this.dimension=dimension;
            var state=block.defaultBlockState();begin(state,below.defaultBlockState());
            if(frame!=0){var along=frame==1?Direction.EAST:Direction.SOUTH;for(int x=-1;x<=2;x++)for(int y=-1;y<=3;y++)if(x==-1||x==2||y==-1||y==3)raw(POS.relative(along,x).above(y),Blocks.OBSIDIAN.defaultBlockState());}
            random.setSeed(seed);observe=true;state.onPlace(this,POS,old.defaultBlockState(),moved);observe=false;
            out("place",dimension.identifier().toString(),""+Block.getId(state),""+Block.getId(old.defaultBlockState()),""+Block.getId(below.defaultBlockState()),""+frame,""+moved,""+seed,""+Block.getId(getBlockState(POS)),String.join(";",writes),scheduled(),""+random.nextLong());
        }
    }
    static final class ContactActor extends Player {
        String hurt;
        ContactActor(Level level){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"contact"));}
        @Override public GameType gameMode(){return GameType.SURVIVAL;}
        @Override public boolean hurtServer(ServerLevel level,DamageSource source,float amount){hurt=source.typeHolder().unwrapKey().orElseThrow().identifier()+":"+Float.floatToRawIntBits(amount);return true;}
    }
    void contacts()throws Exception {
        begin(Blocks.AIR.defaultBlockState(),Blocks.AIR.defaultBlockState());
        var actor=new ContactActor(this);
        for(var block:List.of(Blocks.FIRE,Blocks.SOUL_FIRE))for(var state:block.getStateDefinition().getPossibleStates())for(boolean precise:new boolean[]{false,true}) {
            var trace=new ArrayList<String>();
            var effects=new InsideBlockEffectApplier(){
                public void apply(InsideBlockEffectType type){trace.add("apply:"+type);}
                public void runBefore(InsideBlockEffectType type,java.util.function.Consumer<Entity> effect){throw new AssertionError(type);}
                public void runAfter(InsideBlockEffectType type,java.util.function.Consumer<Entity> effect){effect.accept(actor);trace.add("after:"+type+":"+actor.hurt);}
            };
            random.setSeed(42);state.entityInside(this,POS,actor,effects,precise);
            out("contact",""+Block.getId(state),""+precise,String.join(";",trace),""+random.nextLong());
        }
    }
    static void run(String family)throws Exception {
        var storage=LevelStorageSource.createDefault(Files.createTempDirectory(Path.of("."),"fire-vectors")).createAccess("world");
        var server=GameTestServer.create(Thread.currentThread(),storage,ServerPacksSource.createVanillaTrustedRepository(),Optional.empty(),false,1);
        server.setPlayerList(new PlayerList(server,server.registries(),storage.createPlayerStorage(),new EmptyNotificationService()){});
        var world=new FireLifecycleVectors(server,storage);
        switch(family){case "radius"->world.rules();case "portal"->world.portals();case "lava"->world.lava();case "tick"->world.fireTicks(false);case "rain"->world.fireTicks(true);case "place"->world.placing();case "contact"->world.contacts();default->throw new AssertionError(family);}
        System.exit(0);
    }
}
final class FireLifecycleCapture {
    public static void main(String[] args)throws Exception {
        var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);FireLifecycleVectors.run(args[0]);
    }
}
