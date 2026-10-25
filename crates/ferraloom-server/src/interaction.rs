//! ServerGamePacketListenerImpl's canonical block interaction packet paths.
use super::*;
use ferraloom_data::registries::data_component_type::Id as ComponentType;
use ferraloom_game::{
    advancements::criteria,
    block::{InteractionResult, UseContext, interaction::SwingSource},
    entity::player::{inventory, swing_and_reset_attack_strength},
};
use ferraloom_util::Direction;
use protocol::{
    chat::{Argument, Numeric, TextColor},
    component::{DataComponent, SwingAnimation, SwingAnimationType},
};

impl MinecraftServer {
    /// Port of ServerGamePacketListenerImpl#handleUseItemOn.
    pub(super) fn handle_use_item_on(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::UseItemOn,
    ) -> Result<()> {
        if !self
            .level_for_entity(entity)
            .map_err(game_error)?
            .entity::<ServerPlayer>(entity)
            .map_err(game_error)?
            .has_client_loaded()
        {
            return Ok(());
        }
        self.connections
            .get_mut(&connection)
            .unwrap()
            .ack_block_changes_up_to(packet.sequence.0)?;
        let player = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .entity::<ServerPlayer>(entity)
            .map_err(game_error)?;
        let stack = player.get_item_in_hand(packet.hand).clone();
        let item = stack.item();
        if !item.is_enabled_in_vanilla()
            || !player.is_within_block_interaction_range(packet.hit_result.block_pos, 1.0)
        {
            return Ok(());
        }
        if !valid_hit_location(&packet.hit_result) {
            eprintln!(
                "Rejecting UseItemOnPacket from {}: location too far away from hit block {:?}",
                self.connections[&connection].profile.name, packet.hit_result.block_pos
            );
            return Ok(());
        }
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        let pos = packet.hit_result.block_pos;
        let max_y = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .max_build_height();
        let min_y = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .min_build_height();
        if pos.y > max_y {
            self.send_build_limit(connection, true, max_y);
        } else if pos.y < min_y {
            self.send_build_limit(connection, false, min_y);
        } else {
            let animation = match inventory::component(&stack, ComponentType::INTERACT_ANIMATION) {
                Some(DataComponent::InteractAnimation(animation)) => animation.clone(),
                _ => SwingAnimation {
                    animation_type: SwingAnimationType::Whack,
                    duration: VarInt(6),
                },
            };
            if self
                .level_for_entity(entity)
                .map_err(game_error)?
                .is_under_spawn_protection(entity, pos)
                .map_err(game_error)?
            {
                self.send_interaction_overlay(
                    connection,
                    "build.spawn_protection",
                    Argument::String(format!("{}, {}, {}", pos.x, pos.y, pos.z)),
                );
            } else if self
                .level_for_entity(entity)
                .map_err(game_error)?
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?
                .teleport
                .awaiting_position
                .is_none()
                && self
                    .level_for_entity(entity)
                    .map_err(game_error)?
                    .may_interact(entity, pos)
                    .map_err(game_error)?
            {
                let context = UseContext {
                    player: entity,
                    hand: packet.hand,
                    hit: packet.hit_result.clone(),
                };
                if let Some(result) = ferraloom_game::block::interaction::use_item_on(
                    &context,
                    self.level_for_entity_mut(entity).map_err(game_error)?,
                )
                .map_err(game_error)?
                {
                    if result.consumes_action() {
                        criteria::any_block_use(
                            self.level_for_entity_mut(entity).map_err(game_error)?,
                            entity,
                            pos,
                            &stack,
                        )
                        .map_err(game_error)?;
                    }
                    let direction = packet.hit_result.direction;
                    if direction == Direction::Up
                        && !result.consumes_action()
                        && pos.y >= max_y
                        && self.was_block_placement_attempt(entity, &stack)?
                    {
                        self.send_build_limit(connection, true, max_y);
                    } else {
                        swing_for_result(
                            entity,
                            packet.hand,
                            &animation,
                            &result,
                            self.level_for_entity_mut(entity).map_err(game_error)?,
                        )?;
                    }
                    // 26.3 performs both checks and both swing calls. The entity
                    // swing receiver owns the native suppression of a restart.
                    if !result.consumes_action()
                        && self.was_block_placement_attempt(entity, &stack)?
                    {
                        if direction == Direction::Up && pos.y >= max_y {
                            self.send_build_limit(connection, true, max_y);
                        } else if direction == Direction::Down && pos.y <= min_y {
                            self.send_build_limit(connection, false, min_y);
                        }
                    } else {
                        swing_for_result(
                            entity,
                            packet.hand,
                            &animation,
                            &result,
                            self.level_for_entity_mut(entity).map_err(game_error)?,
                        )?;
                    }
                }
            } else {
                self.send_build_limit(connection, true, max_y);
            }
            for pos in [pos, pos.relative(packet.hit_result.direction, 1)] {
                let state = self
                    .level_for_entity(entity)
                    .map_err(game_error)?
                    .get_block_state(pos)
                    .map_err(game_error)?;
                self.connections
                    .get_mut(&connection)
                    .unwrap()
                    .push(cb::Packet::BlockUpdate(cb::BlockUpdate {
                        pos,
                        block_state: BlockStateId(i32::from(state.id())),
                    }));
            }
        }
        Ok(())
    }

    fn was_block_placement_attempt(
        &self,
        entity: EntityRef,
        stack: &protocol::component::ItemStack,
    ) -> Result<bool> {
        Ok(!stack.is_empty()
            && stack.item().is_block_placement_attempt()
            && !self
                .level_for_entity(entity)
                .map_err(game_error)?
                .entity::<ServerPlayer>(entity)
                .map_err(game_error)?
                .cooldowns
                .is_on_cooldown(stack))
    }

    fn send_build_limit(&mut self, connection: ConnectionId, high: bool, limit: i32) {
        self.send_interaction_overlay(
            connection,
            if high {
                "build.tooHigh"
            } else {
                "build.tooLow"
            },
            Argument::Number(Numeric::Int(limit)),
        );
    }
    fn send_interaction_overlay(
        &mut self,
        connection: ConnectionId,
        key: &str,
        argument: Argument,
    ) {
        let mut content = Component::translatable(key, vec![argument]);
        content.style.color = Some(TextColor::parse("red").expect("native color"));
        self.connections
            .get_mut(&connection)
            .unwrap()
            .push(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                content,
                overlay: true,
            })));
    }

    /// Port of handleSignUpdate/filterTextPacket/updateSignText with the native
    /// pass-through TextFilter. Sign authorization lives in the block entity.
    pub(super) fn handle_sign_update(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::SignUpdate,
    ) -> Result<()> {
        ensure!(
            self.properties.get("text-filtering-config").is_empty(),
            "Unsupported configured TextFilter#processMessageBundle"
        );
        let lines = packet.lines.map(|line| {
            let raw = strip_formatting(&line.0);
            (raw.clone(), raw)
        });
        self.connections.get_mut(&connection).unwrap().last_action = Instant::now();
        if self
            .level_for_entity(entity)
            .map_err(game_error)?
            .chunk(ChunkPos::containing(packet.pos))
            .is_none()
        {
            return Ok(());
        }
        let Some(handle) = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .block_entity_ref(packet.pos)
            .map_err(game_error)?
        else {
            return Ok(());
        };
        if !matches!(
            self.level_for_entity(entity)
                .map_err(game_error)?
                .block_entity_state(handle)
                .map_err(game_error)?,
            ferraloom_game::block::entity::BlockEntityState::Sign(_)
        ) {
            return Ok(());
        }
        let player = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .entity::<ServerPlayer>(entity)
            .map_err(game_error)?;
        let uuid = player.uuid;
        let filter = player.is_text_filtering_enabled();
        let accepted = ferraloom_game::block::containers::sign::update_sign_text(
            handle,
            uuid,
            packet.slot == SignTextSlot::Front,
            &lines,
            filter,
            self.level_for_entity_mut(entity).map_err(game_error)?,
        )
        .map_err(game_error)?;
        if !accepted {
            eprintln!(
                "Player {} tried to change a non-editable sign",
                self.connections[&connection].profile.name
            );
        }
        Ok(())
    }
}

fn swing_for_result(
    entity: EntityRef,
    hand: InteractionHand,
    animation: &SwingAnimation,
    result: &InteractionResult,
    level: &mut ferraloom_game::level::Level,
) -> Result<()> {
    if let InteractionResult::Success { swing_source, .. } = result
        && *swing_source != SwingSource::None
    {
        swing_and_reset_attack_strength(
            entity,
            hand,
            animation.clone(),
            *swing_source != SwingSource::Predicted,
            level,
        )
        .map_err(game_error)?;
    }
    Ok(())
}

fn valid_hit_location(hit: &BlockHitResult) -> bool {
    [
        (hit.block_pos.x, hit.offset_x),
        (hit.block_pos.y, hit.offset_y),
        (hit.block_pos.z, hit.offset_z),
    ]
    .into_iter()
    .all(|(pos, offset)| {
        ((f64::from(pos) + f64::from(offset)) - (f64::from(pos) + 0.5)).abs() < 1.0000001
    })
}

/// Port of ChatFormatting#stripFormatting: only section-sign formatting codes
/// in [0-9A-FK-OR], case insensitive, are removed.
fn strip_formatting(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut result = String::with_capacity(text.len());
    while let Some(character) = chars.next() {
        if character == '§'
            && chars.peek().is_some_and(|next| {
                next.is_ascii_hexdigit() || matches!(next.to_ascii_lowercase(), 'k'..='o' | 'r')
            })
        {
            chars.next();
        } else {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_location_rejects_non_finite_and_outside_values() {
        let mut hit = BlockHitResult {
            block_pos: BlockPos::new(30_000_000, -64, -30_000_000),
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 0.5,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        };
        assert!(valid_hit_location(&hit));
        for offset in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1.500001,
            -0.500001,
        ] {
            hit.offset_x = offset;
            assert!(!valid_hit_location(&hit));
        }
        for offset in [1.5, -0.5] {
            hit.offset_x = offset;
            assert!(valid_hit_location(&hit));
        }
    }
    #[test]
    fn sign_packet_strips_only_native_formatting_codes() {
        assert_eq!(
            strip_formatting("§aGreen §Lbold§r §zkept §"),
            "Green bold §zkept §"
        );
        assert_eq!(strip_formatting("§§A§0héllo§f"), "§héllo");
    }
}
