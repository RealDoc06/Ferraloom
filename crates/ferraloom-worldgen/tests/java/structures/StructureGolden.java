import com.google.gson.*;
import java.nio.file.*;
import java.io.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.levelgen.Beardifier;
import net.minecraft.world.level.levelgen.densityfunction.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pools.*;

/** Authored scenarios run against vanilla; no decompiled sources or assets. */
public final class StructureGolden {
    static final Gson JSON = new GsonBuilder().disableHtmlEscaping().create();
    static JsonArray ints(int... values) { var a=new JsonArray(); for(int v:values)a.add(v); return a; }
    static void write(Path path, JsonElement value) throws Exception {
        try(var out=new GZIPOutputStream(Files.newOutputStream(path))) { out.write(JSON.toJson(value).getBytes(java.nio.charset.StandardCharsets.UTF_8)); }
    }
    static void beardifier(Path directory) throws Exception {
        var out = new JsonArray();
        var b = new BoundingBox(-3,61,2,11,76,14);
        var affected = b.inflatedBy(24);
        for (var adjustment:TerrainAdjustment.values()) for(int delta: new int[]{0,3,-2}) {
            var junctions = List.of(new JigsawJunction(4,65,-2,1,StructureTemplatePool.Projection.RIGID), new JigsawJunction(-9,62,7,-3,StructureTemplatePool.Projection.TERRAIN_MATCHING));
            var rigids = List.of(new Beardifier.Rigid(b,adjustment,delta),new Beardifier.Rigid(new BoundingBox(9,58,3,17,68,18),TerrainAdjustment.BEARD_BOX,2));
            var beard = new Beardifier(rigids,junctions,affected);
            var item = new JsonObject(); item.addProperty("adjustment",adjustment.getSerializedName()); item.addProperty("delta",delta);
            var scalars = new JsonArray();
            // Kernel ends, ground offsets, edges and all quadrants.
            for(int z=-16;z<=30;z+=2)for(int x=-18;x<=30;x+=2)for(int y=45;y<=87;y++)scalars.add(Integer.toUnsignedLong(Float.floatToRawIntBits(beard.sampleValue(SamplerContext.EMPTY_UNCACHED,x,y,z))));
            item.add("scalars",scalars);
            var volumes = new JsonArray();
            for(var volume:List.of(new DensityVolume(10,15,9,-28,35,-22,3,4,5),new DensityVolume(5,6,7,1,62,5),new DensityVolume(3,4,2,100,99,102,5,7,3))) {
                var v = new JsonObject(); v.add("size",ints(volume.sizeX(),volume.sizeY(),volume.sizeZ())); v.add("min",ints(volume.minBlockX(),volume.minBlockY(),volume.minBlockZ())); v.add("step",ints(volume.stepBlockX(),volume.stepBlockY(),volume.stepBlockZ()));
                var values = new JsonArray();var ctx=SamplerContext.builder().build();
                try(var buffer=ctx.acquireBuffer(volume)) {beard.sampleVolume(ctx,buffer,volume);for(int i=0;i<volume.size();i++)values.add(Integer.toUnsignedLong(Float.floatToRawIntBits(buffer.get(i))));}
                v.add("values",values);volumes.add(v);
            }
            item.add("volumes",volumes);out.add(item);
        }
        write(directory.resolve("beardifier.json.gz"),out);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var directory=Path.of(args[0]);Files.createDirectories(directory);beardifier(directory);
    }
}
