// Reference scenarios authored for Ferraloom; runs against the unmodified 26.3 jar.
import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.level.chunk.*;

public class ChunkVectors {
    static void longs(long[] values) {
        System.out.print("[");
        for (long v : values) System.out.print(Long.toUnsignedString(v)+",");
        System.out.print("]");
    }
    static long[] sample(long[] values) {
        long[] result = new long[6];
        for (int i=0;i<4 && i<values.length;i++) result[i]=values[i];
        if(values.length>=2) result[4]=values[values.length-2];
        if(values.length>=1) result[5]=values[values.length-1];
        return result;
    }
    static BitStorage storage(PalettedContainer<?> container) throws Exception {
        Field field=PalettedContainer.class.getDeclaredField("data"); field.setAccessible(true);
        Object data=field.get(container);
        Method method=data.getClass().getDeclaredMethod("storage"); method.setAccessible(true);
        return (BitStorage)method.invoke(data);
    }
    static void prefix(List<Integer> values) {
        System.out.print("[");
        for(int i=0;i<8;i++) System.out.print((i<values.size()?values.get(i):65535)+",");
        System.out.print("]");
    }
    static void palette(boolean biome) throws Exception {
        IdMapper<Integer> registry=new IdMapper<>(); Integer[] values=new Integer[600];
        for(int i=0;i<values.length;i++) { values[i]=Integer.valueOf(i); registry.add(values[i]); }
        Strategy<Integer> strategy=biome?Strategy.createForBiomes(registry):Strategy.createForBlockStates(registry);
        PalettedContainer<Integer> container=new PalettedContainer<>(values[599],strategy);
        int[] checkpoints=biome?new int[]{0,1,2,3,4,7,8,9,16,32,60}:new int[]{0,1,2,15,16,17,31,32,63,64,127,128,255,256,300,550};
        int next=0;
        for(int written:checkpoints) {
            while(next<written) {
                int index=(next*13)%strategy.entryCount(); int axis=biome?2:4, mask=(1<<axis)-1;
                container.set(index&mask,index>>(axis*2),(index>>axis)&mask,values[next]); next++;
            }
            List<Integer> entries=new ArrayList<>(); container.forEachInPalette(entries::add);
            var packed=container.pack(strategy);
            System.out.print("("+biome+","+written+","+container.bitsPerEntry()+","+entries.size()+",");
            prefix(entries); System.out.print(","); longs(sample(storage(container).getRaw()));
            System.out.print(","+packed.bitsPerEntry()+","+packed.paletteEntries().size()+","); prefix(packed.paletteEntries());
            System.out.print(","); longs(sample(packed.storage().map(s->s.toArray()).orElse(new long[0]))); System.out.println("),");
            // Also exercise vanilla's mandatory global reencoding when disk and
            // memory widths happen to match (550 writes in the block scenario).
            var restored=PalettedContainer.unpack(strategy,container.pack(strategy)).getOrThrow();
            for(int i=0;i<strategy.entryCount();i++) {
                int axis=biome?2:4, mask=(1<<axis)-1;
                if(!Objects.equals(restored.get(i&mask,i>>(axis*2),(i>>axis)&mask),container.get(i&mask,i>>(axis*2),(i>>axis)&mask))) throw new AssertionError();
            }
        }
    }
    static void section() throws Exception {
        var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        Holder<Biome> plains=lookup.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
        IdMapper<Holder<Biome>> biomes=new IdMapper<>(); biomes.add(plains);
        var section=new LevelChunkSection(new PalettedContainer<>(Blocks.AIR.defaultBlockState(),Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY)),new PalettedContainer<>(plains,Strategy.createForBiomes(biomes)));
        BlockState[] states={Blocks.STONE.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.LAVA.defaultBlockState(),Blocks.LAVA.defaultBlockState().setValue(LiquidBlock.LEVEL,5),Blocks.OAK_LEAVES.defaultBlockState(),Blocks.WHEAT.defaultBlockState(),Blocks.WHEAT.defaultBlockState().setValue(BlockStateProperties.AGE_7,7),Blocks.OAK_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true),Blocks.CAVE_AIR.defaultBlockState(),Blocks.VOID_AIR.defaultBlockState()};
        System.out.print("pub const SECTION_STATES: &[u16] = &[");
        for(BlockState state:states) System.out.print(Block.getId(state)+","); System.out.println("];");
        System.out.println("pub const SECTION_COUNTS: &[[u16;4]] = &[");
        for(int i=0;i<states.length;i++) { section.setBlockState(i,0,0,states[i]); counts(section); }
        section.setBlockState(2,0,0,Blocks.AIR.defaultBlockState()); counts(section);
        section.setBlockState(7,0,0,Blocks.STONE.defaultBlockState()); counts(section);
        section.recalcBlockCounts(); counts(section); System.out.println("];");
    }
    static void counts(LevelChunkSection section) throws Exception {
        System.out.print("[");
        for(String name:new String[]{"nonEmptyBlockCount","fluidCount","tickingBlockCount","tickingFluidCount"}) {
            Field field=LevelChunkSection.class.getDeclaredField(name); field.setAccessible(true); System.out.print(field.getShort(section)+",");
        }
        System.out.println("],");
    }
    static void uniformSections() throws Exception {
        var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        Holder<Biome> plains=lookup.lookupOrThrow(Registries.BIOME).getOrThrow(Biomes.PLAINS);
        IdMapper<Holder<Biome>> biomes=new IdMapper<>(); biomes.add(plains);
        System.out.println("pub const UNIFORM_SECTIONS: &[(u16,[u16;4],u8,bool)] = &[");
        for(Block block:new Block[]{Blocks.AIR,Blocks.STONE,Blocks.WATER,Blocks.LAVA}) {
            BlockState state=block.defaultBlockState();
            var strategy=Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
            var states=new PalettedContainer<>(state,strategy);
            var section=new LevelChunkSection(states,new PalettedContainer<>(plains,Strategy.createForBiomes(biomes)));
            var packed=states.pack(strategy);
            if(packed.paletteEntries().size()!=1 || packed.paletteEntries().getFirst()!=state) throw new AssertionError();
            states.count((value,count)->{if(value!=state || count!=4096) throw new AssertionError();});
            System.out.print("("+Block.getId(state)+","); counts(section);
            System.out.println(packed.bitsPerEntry()+","+packed.storage().isPresent()+"),");
        }
        System.out.println("];");
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        var output=System.out;
        Bootstrap.bootStrap();
        System.setOut(output);
        System.out.println("// BEGIN CHUNK VECTORS");
        System.out.println("pub const BITS: &[(u8, &[u64], u64)] = &[");
        for(int bits=1;bits<=32;bits++) {
            int[] values=new int[37]; long mask=(1L<<bits)-1;
            for(int i=0;i<values.length;i++) values[i]=(int)((i*0x9e3779b9L+17)&mask);
            SimpleBitStorage storage=new SimpleBitStorage(bits,values.length,values);
            System.out.print("("+bits+",&"); longs(storage.getRaw());
            storage.fill((int)(mask/3)); System.out.println(","+Long.toUnsignedString(storage.getRaw()[0])+"),");
        }
        System.out.println("];\npub const PALETTES: &[(bool,usize,u8,usize,[u16;8],[u64;6],u8,usize,[u16;8],[u64;6])] = &[");
        palette(false); palette(true); System.out.println("];");
        section();
        uniformSections();
    }
}
