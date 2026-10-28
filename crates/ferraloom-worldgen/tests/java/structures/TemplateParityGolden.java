import com.google.gson.*;
import java.nio.file.*;
import java.io.*;
import java.util.*;
import java.util.zip.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.WorldGenLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Exhaustive transient vectors, streamed one template/case at a time. No assets are committed. */
public final class TemplateParityGolden {
    static final Gson JSON = new GsonBuilder().disableHtmlEscaping().create();
    static JsonArray ints(int... values) {var a=new JsonArray();for(int v:values)a.add(v);return a;}
    static JsonArray block(StructureTemplate.StructureBlockInfo b) {
        var a=ints(b.pos().getX(),b.pos().getY(),b.pos().getZ(),Block.getId(b.state()));
        if(b.nbt()==null)a.add(JsonNull.INSTANCE);else a.add(b.nbt().toString());return a;
    }
    static void line(Writer out,JsonObject value)throws Exception {JSON.toJson(value,out);out.write('\n');}
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var paletteField=StructureTemplate.class.getDeclaredField("palettes");paletteField.setAccessible(true);
        var entityField=StructureTemplate.class.getDeclaredField("entityInfoList");entityField.setAccessible(true);
        var jar=Path.of(Block.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        int count=0;
        try(var out=new BufferedWriter(new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(Path.of(args[0]))),java.nio.charset.StandardCharsets.UTF_8));var zip=new ZipFile(jar.toFile())) {
            for(var entry:zip.stream().filter(e->e.getName().startsWith("data/minecraft/structure/")&&e.getName().endsWith(".nbt")).sorted(Comparator.comparing(e->e.getName())).toList()) {
                CompoundTag tag;try(var in=zip.getInputStream(entry)){tag=NbtIo.readCompressed(in,NbtAccounter.unlimitedHeap());}
                var template=new StructureTemplate();template.load(BuiltInRegistries.BLOCK,tag);
                var item=new JsonObject();item.addProperty("id",entry.getName().substring(25,entry.getName().length()-4));
                var size=template.getSize();item.add("size",ints(size.getX(),size.getY(),size.getZ()));
                @SuppressWarnings("unchecked") var palettes=(List<StructureTemplate.Palette>)paletteField.get(template);
                var values=new JsonArray();for(var palette:palettes){var blocks=new JsonArray();for(var b:palette.blocks())blocks.add(block(b));values.add(blocks);}item.add("palettes",values);
                @SuppressWarnings("unchecked") var entities=(List<StructureTemplate.StructureEntityInfo>)entityField.get(template);
                var entityValues=new JsonArray();for(var e:entities){var v=new JsonObject();var pos=new JsonArray();for(double d:new double[]{e.pos.x,e.pos.y,e.pos.z})pos.add(Long.toString(Double.doubleToRawLongBits(d)));v.add("pos",pos);v.add("block_pos",ints(e.blockPos.getX(),e.blockPos.getY(),e.blockPos.getZ()));v.addProperty("nbt",e.nbt.toString());entityValues.add(v);}item.add("entities",entityValues);line(out,item);
                for(int scenario=0;scenario<12;scenario++) {
                    var settings=new StructurePlaceSettings().setMirror(Mirror.values()[scenario%3]).setRotation(Rotation.values()[scenario%4]).setRotationPivot(new BlockPos(1,0,-2)).setKnownShape(true).setIgnoreEntities(true).setLiquidSettings(LiquidSettings.IGNORE_WATERLOGGING);
                    if(scenario%2==1)settings.setBoundingBox(new BoundingBox(-16,63,-16,16,90,16));
                    if(scenario%3==1)settings.addProcessor(new BlockIgnoreProcessor(List.of(Blocks.AIR,Blocks.STRUCTURE_BLOCK)));
                    if(scenario%3==2)settings.addProcessor(BlackstoneReplaceProcessor.INSTANCE).addProcessor(new BlockRotProcessor(0.8f));
                    var supplied=new LegacyRandomSource(91);var random=new WorldgenRandom(new XoroshiroRandomSource(17));
                    if(scenario%3==1)settings.setRandom(supplied);if(scenario%3==2)settings.setRandom(random);
                    var writes=new JsonArray();
                    var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(TemplateParityGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,arguments)->switch(method.getName()) {
                        case "setBlock" -> {var p=(BlockPos)arguments[0];writes.add(ints(p.getX(),p.getY(),p.getZ(),Block.getId((net.minecraft.world.level.block.state.BlockState)arguments[1]),(int)arguments[2]));yield true;}
                        case "getBlockEntity" -> null;
                        default -> throw new UnsupportedOperationException(method.toString());
                    });
                    boolean result=template.placeInWorld(world,new BlockPos(7,64,7),new BlockPos(3,65,4),settings,random,18);
                    var placed=new JsonObject();placed.addProperty("scenario",scenario);placed.addProperty("result",result);placed.add("writes",writes);placed.addProperty("next",random.nextInt());placed.addProperty("settings_next",supplied.nextInt());line(out,placed);
                }
                count++;
            }
        }
        System.out.println("Compared input coverage: "+count+" templates, "+count*12+" placements (known shape, no entity instantiation or block-entity receivers)");
    }
}
