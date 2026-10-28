import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.ai.village.poi.*;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.chunk.storage.SectionStorage;
import static org.mockito.Mockito.*;

/** Actual section/record and query bodies. Only SectionStorage#getOrLoad is
 * supplied in-memory; no query, sorting, occupation or ticket method is mocked. */
public final class PoiRuntimeVectors {
    static String position(BlockPos pos) { return pos.getX()+","+pos.getY()+","+pos.getZ(); }
    static String positions(Stream<BlockPos> values) {
        String result=values.map(PoiRuntimeVectors::position).collect(Collectors.joining(";"));
        return result.isEmpty()?"-":result;
    }
    static String optional(Optional<BlockPos> pos) { return pos.map(PoiRuntimeVectors::position).orElse("-"); }
    static void row(StringBuilder out,String key,Object value) {out.append(key).append('\t').append(value).append('\n');}
    static void section(StringBuilder out,String key,PoiSection section) {
        var packed=section.pack();
        row(out,key,packed.isValid()+"|"+packed.records().stream().map(record->position(record.pos())+"="+record.poiType().getRegisteredName()+"/"+record.freeTickets()).collect(Collectors.joining(";")));
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var home=BuiltInRegistries.POINT_OF_INTEREST_TYPE.getOrThrow(PoiTypes.HOME);
        var meeting=BuiltInRegistries.POINT_OF_INTEREST_TYPE.getOrThrow(PoiTypes.MEETING);
        var output=new StringBuilder();
        var section=new PoiSection(()->{});
        for(BlockPos pos:List.of(BlockPos.ZERO,new BlockPos(1,0,1),new BlockPos(15,15,15),new BlockPos(2,1,3))) section.add(pos,home);
        section(output,"section.add",section);
        row(output,"section.duplicate",section.add(BlockPos.ZERO,home)==null);
        section.remove(new BlockPos(1,0,1));section(output,"section.remove",section);
        row(output,"section.release.full",section.release(BlockPos.ZERO));
        var invalid=new PoiSection.Packed(false,List.of(new PoiRecord.Packed(BlockPos.ZERO,home,0),new PoiRecord.Packed(new BlockPos(1,0,1),home,1))).unpack(()->{});
        invalid.refresh(consumer->{consumer.accept(BlockPos.ZERO,meeting);consumer.accept(new BlockPos(2,1,3),meeting);});
        section(output,"section.refresh",invalid);
        invalid.refresh(consumer->{});section(output,"section.valid.no_refresh",invalid);

        var sections=new HashMap<Long,PoiSection>();
        for (BlockPos pos:List.of(new BlockPos(-17,-1,0),new BlockPos(-1,0,0),BlockPos.ZERO,new BlockPos(1,0,1),new BlockPos(15,0,0),new BlockPos(16,1,0),new BlockPos(0,15,16),new BlockPos(0,-16,0))) {
            sections.computeIfAbsent(SectionPos.asLong(pos),key->new PoiSection(()->{})).add(pos,home);
        }
        PoiManager manager=mock(PoiManager.class,call->{
            if (call.getMethod().getName().equals("getOrLoad")) return Optional.ofNullable(sections.get((Long)call.getArgument(0)));
            return call.callRealMethod();
        });
        var height=SectionStorage.class.getDeclaredField("levelHeightAccessor");height.setAccessible(true);height.set(manager,LevelHeightAccessor.create(-16,32));
        Predicate<Holder<PoiType>> homes=type->type.is(PoiTypes.HOME);
        var center=BlockPos.ZERO;
        row(output,"query.chunk",positions(manager.getInChunk(homes,new net.minecraft.world.level.ChunkPos(0,0),PoiManager.Occupancy.ANY).map(PoiRecord::getPos)));
        for (int radius:new int[]{0,1,16,32}) {
            row(output,"query.square."+radius,positions(manager.getInSquare(homes,center,radius,PoiManager.Occupancy.ANY).map(PoiRecord::getPos)));
            row(output,"query.range."+radius,positions(manager.getInRange(homes,center,radius,PoiManager.Occupancy.ANY).map(PoiRecord::getPos)));
        }
        row(output,"query.closest",optional(manager.findClosest(homes,center,16,PoiManager.Occupancy.ANY)));
        row(output,"query.closest.filtered",optional(manager.findClosest(homes,pos->pos.getX()>0,center,16,PoiManager.Occupancy.ANY)));
        row(output,"query.find",optional(manager.find(homes,pos->pos.getY()>=0,center,16,PoiManager.Occupancy.ANY)));
        row(output,"query.all",positions(manager.findAll(homes,pos->pos.getX()>=0,center,16,PoiManager.Occupancy.ANY)));
        row(output,"query.closest_first",positions(manager.findAllClosestFirstWithType(homes,pos->true,center,32,PoiManager.Occupancy.ANY).map(pair->pair.getSecond())));
        for (int index=0;index<3;index++) row(output,"tickets.take."+index,optional(manager.take(homes,(type,pos)->true,center,16)));
        row(output,"tickets.occupied",positions(manager.getInRange(homes,center,16,PoiManager.Occupancy.IS_OCCUPIED).map(PoiRecord::getPos)));
        row(output,"tickets.space",positions(manager.getInRange(homes,center,16,PoiManager.Occupancy.HAS_SPACE).map(PoiRecord::getPos)));
        row(output,"tickets.release",manager.release(new BlockPos(-1,0,0)));
        row(output,"tickets.release.full",manager.release(new BlockPos(-1,0,0)));
        row(output,"tickets.count",manager.getCountInRange(homes,center,16,PoiManager.Occupancy.HAS_SPACE));
        var random=RandomSource.create(42);
        row(output,"query.random",optional(manager.getRandom(homes,pos->pos.getX()>=0,PoiManager.Occupancy.ANY,center,32,random)));
        row(output,"query.random.next",random.nextInt());
        row(output,"query.exists",manager.exists(BlockPos.ZERO,homes));
        row(output,"query.type",manager.getType(BlockPos.ZERO).map(Holder::getRegisteredName).orElse("-"));
        Files.writeString(Path.of(args[0]),output.toString());
    }
}
