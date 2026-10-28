import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;

/** Independent native API capture, separate from the generated NEXT table. */
public final class WeatheringCopperFacts {
    public static void main(String[] args) {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        for(var block:BuiltInRegistries.BLOCK) {
            int age=block instanceof WeatheringCopper copper?copper.getAge().ordinal():-1;
            System.out.println("WEATHERING\tblock\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+age+"\t"
                +WeatheringCopper.getNext(block).map(BuiltInRegistries.BLOCK::getId).orElse(-1)+"\t"
                +WeatheringCopper.getPrevious(block).map(BuiltInRegistries.BLOCK::getId).orElse(-1)+"\t"
                +BuiltInRegistries.BLOCK.getId(WeatheringCopper.getFirst(block)));
            if(block instanceof WeatheringCopper copper)for(var state:block.getStateDefinition().getPossibleStates())
                System.out.println("WEATHERING\tstate\t"+Block.getId(state)+"\t"
                    +copper.getNext(state).map(Block::getId).orElse(-1)+"\t"
                    +WeatheringCopper.getPrevious(state).map(Block::getId).orElse(-1)+"\t"
                    +Block.getId(WeatheringCopper.getFirst(state))+"\t"+state.isRandomlyTicking());
        }
    }
}
