//! SignBlock's editing interaction; sign applicator effects remain explicit.
use super::sign::SignBlockEntity;
use crate::block::entity::BlockEntityRef;
use crate::entity::EntityRef;
use crate::{
    block::{BlockBehavior, BlockMethod, InteractionResult, UseContext},
    entity::player::ServerPlayer,
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_data::{BlockClass, BlockState, registries::sound_event::Id as SoundEvent};
use ferraloom_protocol::component::SignText;
use ferraloom_protocol::{
    chat::{ClickEvent, Component, Contents, TextColor},
    play::{BlockStateId, SignTextSlot, SoundSource, clientbound as cb},
};
use ferraloom_util::{BlockPos, Direction, Vec3};

pub(super) struct SignBlock;
pub(super) static SIGN_BLOCK: SignBlock = SignBlock;
fn command(event: &Option<ClickEvent>) -> bool {
    matches!(
        event,
        Some(ClickEvent::RunCommand(_) | ClickEvent::ShowDialog(_) | ClickEvent::Custom { .. })
    )
}
/// Port of SignText#hasEditableText. Native empty equality and literal contents
/// remain distinct from display text or command-action eligibility.
pub(super) fn has_editable_text(text: &SignText, filtered: bool) -> bool {
    let lines = if filtered {
        text.filtered_messages.as_ref().unwrap_or(&text.messages)
    } else {
        &text.messages
    };
    lines
        .iter()
        .all(|line| matches!(line.contents, Contents::Text(_)) || *line == Component::default())
}
/// Port of SignBlock#openTextEdit followed by ServerPlayer#openTextEdit: editor
/// identity, the live BlockUpdate, then OpenSignEditor in the same output stream.
pub(super) fn open_text_edit(
    handle: BlockEntityRef,
    player: EntityRef,
    slot: SignTextSlot,
    level: &mut Level,
) -> Result<(), LevelError> {
    let editor = level.entity::<ServerPlayer>(player)?.uuid;
    level
        .block_entity_mut::<SignBlockEntity>(handle)?
        .allowed_player_editor = Some(editor);
    let state = level.get_block_state(handle.pos)?;
    for packet in [
        cb::Packet::BlockUpdate(cb::BlockUpdate {
            pos: handle.pos,
            block_state: BlockStateId(i32::from(state.id())),
        }),
        cb::Packet::OpenSignEditor(cb::OpenSignEditor {
            pos: handle.pos,
            slot,
        }),
    ] {
        level.emit(LevelEffect::PlayerPacket {
            recipients: vec![player],
            packet: Box::new(packet),
        });
    }
    Ok(())
}
impl BlockBehavior for SignBlock {
    fn use_item_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(InteractionResult::Pass);
        };
        let sign = level.block_entity::<SignBlockEntity>(handle)?;
        let player = level.entity::<ServerPlayer>(context.player)?;
        let stack = player.get_item_in_hand(context.hand);
        let item = stack.item();
        let front = sign.player_is_facing_front(player.position);
        let text = if front {
            &sign.front_text
        } else {
            &sign.back_text
        };
        let lines = if player.is_text_filtering_enabled() {
            text.filtered_messages.as_ref().unwrap_or(&text.messages)
        } else {
            &text.messages
        };
        let can_execute =
            sign.is_waxed && lines.iter().any(|line| command(&line.style.click_event));
        if !can_execute && item.is_hanging_sign_item() {
            let chain = match state.block().class() {
                BlockClass::CeilingHangingSignBlock => context.hit.direction == Direction::Down,
                BlockClass::WallHangingSignBlock => {
                    context.hit.direction.axis() != super::menu::facing(state).axis()
                }
                _ => false,
            };
            if chain {
                return Ok(InteractionResult::Pass);
            }
        }
        if item.is_sign_applicator()
            && player.may_build()
            && !sign.is_waxed
            && sign
                .allowed_player_editor
                .is_none_or(|editor| editor == player.uuid)
        {
            if item == ferraloom_data::Item::HONEYCOMB {
                let slot =
                    crate::block::interaction::hand_slot(level, context.player, context.hand)?;
                let mut operation = crate::inventory::StackOperation::new(level);
                let receiver = operation
                    .pin(slot)
                    .map_err(crate::item::stack::receiver_error)?;
                return operation.level.with_block_entity_callback(handle, |level| {
                    // The outer operation keeps this receiver pinned through
                    // sign notifications, game-event listeners and consumption.
                    if !crate::item::honeycomb::try_apply_to_sign(handle, level)? {
                        return Ok(InteractionResult::TryWithEmptyHand);
                    }
                    let sign = level.block_entity::<SignBlockEntity>(handle)?;
                    let text = if front {
                        &sign.front_text
                    } else {
                        &sign.back_text
                    };
                    let filtered = level
                        .entity::<ServerPlayer>(context.player)?
                        .is_text_filtering_enabled();
                    let lines = if filtered {
                        text.filtered_messages.as_ref().unwrap_or(&text.messages)
                    } else {
                        &text.messages
                    };
                    let commands = lines.iter().any(|line| command(&line.style.click_event));
                    if commands && sign.allow_op_features {
                        return Err(LevelError::UnsupportedBlockMethod(
                            state.block(),
                            BlockMethod::UseItemOn,
                        ));
                    }
                    if commands {
                        let mut content =
                            Component::translatable("sign.click_actions_disabled", Vec::new());
                        content.style.color = Some(TextColor::parse("red").expect("native color"));
                        level.emit(LevelEffect::PlayerPacket {
                            recipients: vec![context.player],
                            packet: Box::new(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                                content,
                                overlay: true,
                            }))),
                        });
                    }
                    let item = level
                        .stack(receiver)
                        .map_err(crate::item::stack::receiver_error)?
                        .item();
                    crate::stats::award_stat(
                        level,
                        context.player,
                        crate::stats::Stat::Used(item),
                        1,
                    )?;
                    let sign_state = level
                        .block_entity::<SignBlockEntity>(handle)?
                        .parent
                        .block_state;
                    level.game_event(
                        ferraloom_data::registries::game_event::Id::BLOCK_CHANGE,
                        Vec3::new(
                            handle.pos.x as f64 + 0.5,
                            handle.pos.y as f64 + 0.5,
                            handle.pos.z as f64 + 0.5,
                        ),
                        Some(context.player),
                        Some(sign_state),
                    )?;
                    if !level.entity::<ServerPlayer>(context.player)?.instabuild() {
                        let stack = level
                            .stack_mut(receiver)
                            .map_err(crate::item::stack::receiver_error)?;
                        let count = crate::entity::player::inventory::count(stack);
                        crate::entity::player::inventory::set_count(stack, count.wrapping_sub(1));
                    }
                    Ok(InteractionResult::SUCCESS)
                });
            }
            // Applicator canApply/tryApply must produce a real query result; a
            // skipped effect cannot manufacture TRY_WITH_EMPTY_HAND or success.
            return Err(LevelError::UnsupportedBlockMethod(
                state.block(),
                BlockMethod::UseItemOn,
            ));
        }
        Ok(InteractionResult::TryWithEmptyHand)
    }

    /// Port of SignBlock#useWithoutItem / openTextEdit.
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(InteractionResult::Pass);
        };
        level.with_block_entity_callback(handle, |level| {
            let sign = level.block_entity::<SignBlockEntity>(handle)?;
            let player = level.entity::<ServerPlayer>(context.player)?;
            let front = sign.player_is_facing_front(player.position);
            let text = if front {
                &sign.front_text
            } else {
                &sign.back_text
            };
            let lines = if player.is_text_filtering_enabled() {
                text.filtered_messages.as_ref().unwrap_or(&text.messages)
            } else {
                &text.messages
            };
            let has_commands = lines.iter().any(|line| command(&line.style.click_event));
            if has_commands && sign.allow_op_features {
                return Err(LevelError::UnsupportedBlockMethod(
                    state.block(),
                    BlockMethod::UseWithoutItem,
                ));
            }
            let waxed = sign.is_waxed;
            let can_edit = sign
                .allowed_player_editor
                .is_none_or(|editor| editor == player.uuid)
                && player.may_build()
                && has_editable_text(text, player.is_text_filtering_enabled());
            if has_commands {
                let mut content =
                    Component::translatable("sign.click_actions_disabled", Vec::new());
                content.style.color = Some(TextColor::parse("red").expect("native color"));
                level.emit(LevelEffect::PlayerPacket {
                    recipients: vec![context.player],
                    packet: Box::new(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                        content,
                        overlay: true,
                    }))),
                });
            }
            if waxed {
                level.play_sound(
                    None,
                    Vec3::new(
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ),
                    if state.block().block_entity_type()
                        == Some(crate::block::entity::BlockEntityType::HANGING_SIGN)
                    {
                        SoundEvent::BLOCK_HANGING_SIGN_WAXED_INTERACT_FAIL
                    } else {
                        SoundEvent::BLOCK_SIGN_WAXED_INTERACT_FAIL
                    },
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                )?;
                Ok(InteractionResult::SUCCESS_SERVER)
            } else if has_commands {
                Ok(InteractionResult::SUCCESS_SERVER)
            } else if can_edit {
                open_text_edit(
                    handle,
                    context.player,
                    if front {
                        SignTextSlot::Front
                    } else {
                        SignTextSlot::Back
                    },
                    level,
                )?;
                Ok(InteractionResult::SUCCESS_SERVER)
            } else {
                Ok(InteractionResult::Pass)
            }
        })
    }
}
