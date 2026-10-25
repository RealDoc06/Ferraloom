//! Synchronous ServerPlayer notifications shared with connection-side guards.
use crate::{
    entity::EntityRef,
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_protocol::{
    chat::{Argument, Component, Numeric, TextColor},
    play::clientbound as cb,
};

/// Port of ServerPlayer#sendBuildLimitMessage's native overlay packet.
pub fn build_limit_packet(too_high: bool, limit: i32) -> cb::Packet {
    let mut content = Component::translatable(
        if too_high {
            "build.tooHigh"
        } else {
            "build.tooLow"
        },
        vec![Argument::Number(Numeric::Int(limit))],
    );
    content.style.color = Some(TextColor::parse("red").expect("native color"));
    cb::Packet::SystemChat(Box::new(cb::SystemChat {
        content,
        overlay: true,
    }))
}

/// Port of ServerPlayer#sendBuildLimitMessage at the originating gameplay call.
pub fn send_build_limit_message(
    player: EntityRef,
    too_high: bool,
    limit: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity::<super::ServerPlayer>(player)?;
    level.emit(LevelEffect::PlayerPacket {
        recipients: vec![player],
        packet: Box::new(build_limit_packet(too_high, limit)),
    });
    Ok(())
}
