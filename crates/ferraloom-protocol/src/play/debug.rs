//! Port of DebugSubscriptions and their typed update/event payloads.
use super::*;
packet!(/// Port of DebugBeeInfo.
DebugBeeInfo {hive_pos:Option<BlockPos>,flower_pos:Option<BlockPos>,travel_ticks:VarInt,blacklisted_hives:Vec<BlockPos>});
packet!(/// Port of DebugBrainDump.
DebugBrainDump {name:String,profession:String,xp:i32,health:f32,max_health:f32,inventory:String,wants_golem:bool,anger_level:i32,activities:Vec<String>,behaviors:Vec<String>,memories:Vec<String>,gossips:Vec<String>,pois:Vec<BlockPos>,potential_pois:Vec<BlockPos>});
packet!(/// Port of DebugBreezeInfo.
DebugBreezeInfo {attack_target:Option<VarInt>,jump_target:Option<BlockPos>});
packet!(/// Port of DebugGoalInfo.DebugGoal.
DebugGoal {priority:VarInt,is_running:bool,name:Utf8<255>});
packet!(/// Port of DebugGoalInfo.
DebugGoalInfo {goals:Vec<DebugGoal>});
wire_enum!(DebugEntityBlockIntersection {InBlock=0,InFluid=1,InAir=2},zero);
packet!(/// Port of DebugHiveInfo.
DebugHiveInfo {block_type:BlockId,occupant_count:VarInt,honey_level:VarInt,sedated:bool});
packet!(/// Port of DebugPoiInfo.
DebugPoiInfo {pos:BlockPos,poi_type:VarInt,free_ticket_count:VarInt});
packet!(/// Port of DebugGameEventListenerInfo.
DebugGameEventListenerInfo {listener_radius:VarInt});
packet!(/// Port of DebugGameEventInfo.
DebugGameEventInfo {event:VarInt,pos:Vec3});
packet!(/// Port of BoundingBox.STREAM_CODEC.
BoundingBox {min:BlockPos,max:BlockPos});
packet!(/// Port of DebugStructureInfo.Piece.
DebugStructurePiece {bounding_box:BoundingBox,is_start:bool});
packet!(/// Port of DebugStructureInfo.
DebugStructureInfo {bounding_box:BoundingBox,pieces:Vec<DebugStructurePiece>});
wire_enum!(PathType {Blocked=0,Open=1,Walkable=2,WalkableDoor=3,Trapdoor=4,PowderSnow=5,OnTopOfPowderSnow=6,Fence=7,Lava=8,Water=9,WaterBorder=10,Rail=11,UnpassableRail=12,FireInNeighbor=13,Fire=14,DamagingInNeighbor=15,Damaging=16,DoorOpen=17,DoorWoodClosed=18,DoorIronClosed=19,Breach=20,Leaves=21,StickyHoney=22,Cocoa=23,DamageCautious=24,OnTopOfTrapdoor=25,BigMobsCloseToDanger=26},zero);
packet!(/// Port of Node.DEBUG_STREAM_CODEC (also the Target codec).
PathNode {x:i32,y:i32,z:i32,walked_distance:f32,cost_malus:f32,closed:bool,path_type:PathType,f:f32});
packet!(/// Port of Path.DebugData.
PathDebugData {open_set:Vec<PathNode>,closed_set:Vec<PathNode>,target_nodes:Vec<PathNode>});
packet!(/// Port of Path.DEBUG_STREAM_CODEC.
DebugPath {reached:bool,next_node_index:i32,target:BlockPos,nodes:Vec<PathNode>,debug_data:PathDebugData});
packet!(/// Port of DebugPathInfo.
DebugPathInfo {path:DebugPath,max_node_distance:f32});

/// Port of Orientation's 48-entry packed index.
#[derive(Clone, Debug, PartialEq)]
pub struct RedstoneOrientation(pub u8);
impl Wire for RedstoneOrientation {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.var_int()?;
        if !(0..48).contains(&id) {
            return Err(Error::invalid("orientation", id.to_string()));
        }
        Ok(Self(id as u8))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if self.0 >= 48 {
            return Err(Error::invalid("orientation", self.0.to_string()));
        }
        w.var_int(i32::from(self.0));
        Ok(())
    }
}

wire_union!(DebugSubscriptionEvent:var_int {
    Bees=1 {value:DebugBeeInfo},
    Brains=2 {value:DebugBrainDump},
    Breezes=3 {value:DebugBreezeInfo},
    GoalSelectors=4 {value:DebugGoalInfo},
    EntityPaths=5 {value:DebugPathInfo},
    EntityBlockIntersections=6 {value:DebugEntityBlockIntersection},
    BeeHives=7 {value:DebugHiveInfo},
    Pois=8 {value:DebugPoiInfo},
    RedstoneWireOrientations=9 {value:RedstoneOrientation},
    VillageSections=10 {value:crate::component::Unit},
    Raids=11 {value:Vec<BlockPos>},
    Structures=12 {value:Vec<DebugStructureInfo>},
    GameEventListeners=13 {value:DebugGameEventListenerInfo},
    NeighborUpdates=14 {value:BlockPos},
    GameEvents=15 {value:DebugGameEventInfo},
});
wire_union!(DebugSubscriptionUpdate:var_int {
    Bees=1 {value:Option<DebugBeeInfo>},
    Brains=2 {value:Option<DebugBrainDump>},
    Breezes=3 {value:Option<DebugBreezeInfo>},
    GoalSelectors=4 {value:Option<DebugGoalInfo>},
    EntityPaths=5 {value:Option<DebugPathInfo>},
    EntityBlockIntersections=6 {value:Option<DebugEntityBlockIntersection>},
    BeeHives=7 {value:Option<DebugHiveInfo>},
    Pois=8 {value:Option<DebugPoiInfo>},
    RedstoneWireOrientations=9 {value:Option<RedstoneOrientation>},
    VillageSections=10 {value:Option<crate::component::Unit>},
    Raids=11 {value:Option<Vec<BlockPos>>},
    Structures=12 {value:Option<Vec<DebugStructureInfo>>},
    GameEventListeners=13 {value:Option<DebugGameEventListenerInfo>},
    NeighborUpdates=14 {value:Option<BlockPos>},
    GameEvents=15 {value:Option<DebugGameEventInfo>},
});
