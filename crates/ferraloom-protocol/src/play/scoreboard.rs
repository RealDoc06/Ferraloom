//! Port of objective, team, and score update operation formats.
use super::*;
wire_union!(NumberFormat:var_int {Blank=0 {},Styled=1 {style:crate::chat::Style},Fixed=2 {value:Component}});
wire_enum!(ObjectiveRenderType {Integer=0,Hearts=1});
wire_union!(ObjectiveOperation:u8 {
    Add=0 {display_name:Component,render_type:ObjectiveRenderType,number_format:Option<NumberFormat>},
    Remove=1 {},
    Change=2 {display_name:Component,render_type:ObjectiveRenderType,number_format:Option<NumberFormat>}
});
wire_enum!(TeamVisibility {Always=0,Never=1,HideForOtherTeams=2,HideForOwnTeam=3},zero);
wire_enum!(TeamCollisionRule {Always=0,Never=1,PushOtherTeams=2,PushOwnTeam=3},zero);
wire_enum!(TeamColor {Black=0,DarkBlue=1,DarkGreen=2,DarkAqua=3,DarkRed=4,DarkPurple=5,Gold=6,Gray=7,DarkGray=8,Blue=9,Green=10,Aqua=11,Red=12,LightPurple=13,Yellow=14,White=15},zero);
packet!(/// Port of ClientboundSetPlayerTeamPacket.Parameters.
TeamParameters {display_name:Component,player_prefix:Component,player_suffix:Component,name_tag_visibility:TeamVisibility,collision_rule:TeamCollisionRule,color:Option<TeamColor>,options:u8});
wire_union!(TeamOperation:u8 {
    Add=0 {parameters:TeamParameters,players:Vec<String>},
    Remove=1 {},
    Change=2 {parameters:TeamParameters},
    Join=3 {players:Vec<String>},
    Leave=4 {players:Vec<String>}
});
