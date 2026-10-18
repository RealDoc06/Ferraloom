import com.google.gson.*;
import com.mojang.serialization.*;
import net.minecraft.util.*;
import net.minecraft.world.attribute.LerpFunction;
import java.util.*;

/** Actual KeyframeTrackSampler and EasingType; no handwritten expected numbers. */
public final class TimelineVectors {
    public static void main(String[]args) {
        for(String ease:List.of("\"linear\"","\"constant\"","{\"cubic_bezier\":[0.362,0.241,0.638,0.759]}","{\"cubic_bezier\":[0.0,1.0,1.0,0.0]}"))
        for(String frames:List.of("[{\"ticks\":0,\"value\":3.0}]","[{\"ticks\":133,\"value\":1.0},{\"ticks\":11867,\"value\":1.0},{\"ticks\":13670,\"value\":0.26666668},{\"ticks\":22330,\"value\":0.26666668}]","[{\"ticks\":6000,\"value\":360.0},{\"ticks\":6000,\"value\":0.0}]"))
        for(int period:new int[]{0,24000}) {
            String json="{\"ease\":"+ease+",\"keyframes\":"+frames+"}";
            var track=KeyframeTrack.mapCodec(Codec.FLOAT).codec().parse(JsonOps.INSTANCE,JsonParser.parseString(json)).getOrThrow();
            var sampler=track.bakeSampler(period==0?Optional.empty():Optional.of(period),LerpFunction.ofFloat());
            for(long tick:new long[]{Long.MIN_VALUE,-48001,-24001,-1,0,1,133,730,5999,6000,6001,11867,13670,22330,23999,24000,48000,Long.MAX_VALUE}) {
                System.out.println(period+"\t"+tick+"\t"+json+"\t"+Integer.toUnsignedString(Float.floatToRawIntBits(sampler.sample(tick))));
            }
        }
    }
}
