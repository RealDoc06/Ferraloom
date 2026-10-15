#[allow(clippy::wildcard_imports)]
use super::*;

/// Match Java ChatFormatting.stripFormatting, including its ASCII case rules.
fn strip_sign_formatting(line: &str) -> Box<str> {
    let mut result = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{a7}'
            && characters.peek().is_some_and(|code| {
                code.is_ascii_hexdigit() || matches!(code.to_ascii_lowercase(), 'k'..='o' | 'r')
            })
        {
            characters.next();
        } else {
            result.push(character);
        }
    }
    result.into_boxed_str()
}

impl JavaClient {
    pub fn handle_sign_update(&self, player: &Player, sign_data: &SUpdateSign<'_>) {
        let world = player.get_entity().world.load_full();
        let Some(block_entity) = world.get_block_entity(&sign_data.location) else {
            return;
        };
        let Some(sign_entity) =
            crate::block::entities::sign::SignEntityRef::from_block_entity(&*block_entity)
        else {
            return;
        };
        if sign_entity.is_waxed() {
            return;
        }

        let currently_editing = *sign_entity
            .currently_editing_player()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if currently_editing != Some(player.gameprofile.id) {
            return;
        }

        let new_messages = [
            sign_data.line_1,
            sign_data.line_2,
            sign_data.line_3,
            sign_data.line_4,
        ]
        .map(strip_sign_formatting);

        if let Some(player_arc) = world.get_player_by_uuid(player.gameprofile.id) {
            let mut event = crate::plugin::api::events::block::sign_change::SignChangeEvent::new(
                player_arc,
                sign_data.location,
                new_messages.iter().map(ToString::to_string).collect(),
            );
            if let Some(server) = world.server.upgrade() {
                server.plugin_manager.fire_blocking(&server, &mut event);
            }
            if event.cancelled {
                return;
            }
        }

        // Callbacks hold no sign locks. Revalidate editor and wax atomically;
        // Java expires distant editors during the sign tick, not this packet.
        if !sign_entity.update_player_text(
            player.gameprofile.id,
            sign_data.is_front_text,
            player.is_text_filtering_enabled(),
            new_messages,
        ) {
            return;
        }
        world.update_block_entity(&block_entity);
    }
}

#[cfg(test)]
mod tests {
    use super::strip_sign_formatting;

    #[test]
    fn submitted_sign_text_strips_only_recognized_legacy_formatting() {
        for (input, expected) in [
            ("§aGreen §Rreset §khidden§r", "Green reset hidden"),
            ("§G §x §\n§", "§G §x §\n§"),
            (
                "§c{\"text\":\"literal §Acontent\"}",
                "{\"text\":\"literal content\"}",
            ),
        ] {
            assert_eq!(strip_sign_formatting(input).as_ref(), expected);
        }
    }
}
