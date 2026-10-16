use crate::command::argument_types::argument_type::{ArgumentType, JavaClientArgumentType};
use crate::command::argument_types::entity::ONLY_PLAYERS_ALLOWED_ERROR_TYPE;
use crate::command::argument_types::entity_selector::EntitySelector;
use crate::command::argument_types::entity_selector::parser::{
    EntitySelectorParser, EntitySelectorParserSuggestions,
};
use crate::command::context::command_context::CommandContext;
use crate::command::context::command_source::CommandSource;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::string_reader::StringReader;
use crate::command::suggestion::suggestions::{Suggestions, SuggestionsBuilder};
use crate::net::authentication::lookup_profile_by_name_blocking;
use crate::net::{GameProfile, is_legacy_offline_profile, offline_uuid};
use crate::server::Server;
use arc_swap::ArcSwap;
use pumpkin_data::translation;
use std::sync::Arc;
use uuid::Uuid;

pub const UNKNOWN_PLAYER_ERROR_TYPE: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_PLAYER_UNKNOWN,
    translation::java::ARGUMENT_PLAYER_UNKNOWN,
);

/// A result from the [`GameProfileArgumentType`], which can be resolved into
/// one or more [`GameProfile`]s, successfully or not.
pub enum GameProfileResult {
    Selector(Box<EntitySelector>),
    Name(String),
    Uuid(Uuid),
}

impl GameProfileResult {
    fn unknown_player_syntax_error() -> CommandSyntaxError {
        UNKNOWN_PLAYER_ERROR_TYPE.create_without_context()
    }

    /// Resolves this result with the help of a [`CommandSource`].
    ///
    /// # Warning
    ///
    /// Do not lock write/read access to one of these data locks
    /// before calling this method, as that may cause a *deadlock*:
    /// - `server.data.user_cache`
    /// - `server.data.operator_config`
    /// - `server.data.banned_player_list`
    /// - `server.data.whitelist_config`
    ///
    /// Instead, call this method *before* using `write()`/`read()` on a lock.
    pub fn resolve(&self, source: &CommandSource) -> Result<Vec<GameProfile>, CommandSyntaxError> {
        match self {
            Self::Selector(selector) => {
                let players = selector.find_players(source)?;
                if players.is_empty() {
                    return Err(
                        crate::command::argument_types::entity::NO_PLAYERS_ERROR_TYPE
                            .create_without_context(),
                    );
                }
                Ok(players.iter().map(|p| p.gameprofile.clone()).collect())
            }
            Self::Name(name) => {
                let server = source.server();
                if let Some(player) = server.get_player_by_name(name) {
                    return Ok(vec![player.gameprofile.clone()]);
                }

                let cached_entry = server
                    .data
                    .user_cache
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_by_name(name);
                if let Some(entry) = cached_entry {
                    return Ok(vec![Self::profile_from_uuid_name(entry.uuid, entry.name)]);
                }

                if let Some(profile) = Self::resolve_known_profile_by_name(server, name) {
                    return Ok(vec![profile]);
                }

                if server.advanced_config.networking.java.online_mode {
                    match lookup_profile_by_name_blocking(
                        name,
                        &server.advanced_config.networking.java.authentication,
                    ) {
                        Ok(Some((uuid, resolved_name))) => {
                            server
                                .data
                                .user_cache
                                .write()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .upsert(uuid, resolved_name.clone());
                            Ok(vec![Self::profile_from_uuid_name(uuid, resolved_name)])
                        }
                        _ => Err(Self::unknown_player_syntax_error()),
                    }
                } else if let Ok(uuid) = offline_uuid(name) {
                    let profile = Self::profile_from_uuid_name(uuid, name.clone());
                    server
                        .data
                        .user_cache
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .upsert(profile.id, profile.name.clone());
                    Ok(vec![profile])
                } else {
                    Err(Self::unknown_player_syntax_error())
                }
            }
            Self::Uuid(uuid) => {
                let server = source.server();

                if let Some(player) = server.get_player_by_uuid(*uuid) {
                    return Ok(vec![player.gameprofile.clone()]);
                }

                let cached_entry = server
                    .data
                    .user_cache
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_by_uuid(*uuid);
                if let Some(entry) = cached_entry {
                    return Ok(vec![Self::profile_from_uuid_name(entry.uuid, entry.name)]);
                }

                if let Some(profile) = Self::resolve_known_profile_by_uuid(server, *uuid) {
                    return Ok(vec![profile]);
                }

                Err(Self::unknown_player_syntax_error())
            }
        }
    }

    fn resolve_known_profile_by_name(server: &Server, name: &str) -> Option<GameProfile> {
        let ops = server
            .data
            .operator_config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(op) = ops.ops.iter().find(|op| {
            op.name.eq_ignore_ascii_case(name) && !is_legacy_offline_profile(op.uuid, &op.name)
        }) {
            return Some(Self::profile_from_uuid_name(op.uuid, op.name.clone()));
        }

        let banned_players = server
            .data
            .banned_player_list
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = banned_players.banned_players.iter().find(|entry| {
            entry.name.eq_ignore_ascii_case(name)
                && !is_legacy_offline_profile(entry.uuid, &entry.name)
        }) {
            return Some(Self::profile_from_uuid_name(entry.uuid, entry.name.clone()));
        }

        let whitelist = server
            .data
            .whitelist_config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = whitelist.whitelist.iter().find(|entry| {
            entry.name.eq_ignore_ascii_case(name)
                && !is_legacy_offline_profile(entry.uuid, &entry.name)
        }) {
            return Some(Self::profile_from_uuid_name(entry.uuid, entry.name.clone()));
        }

        None
    }

    fn resolve_known_profile_by_uuid(server: &Server, uuid: Uuid) -> Option<GameProfile> {
        let ops = server
            .data
            .operator_config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(op) = ops.ops.iter().find(|op| op.uuid == uuid) {
            return Some(Self::profile_from_uuid_name(op.uuid, op.name.clone()));
        }

        let banned_players = server
            .data
            .banned_player_list
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = banned_players
            .banned_players
            .iter()
            .find(|entry| entry.uuid == uuid)
        {
            return Some(Self::profile_from_uuid_name(entry.uuid, entry.name.clone()));
        }

        let whitelist = server
            .data
            .whitelist_config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = whitelist.whitelist.iter().find(|entry| entry.uuid == uuid) {
            return Some(Self::profile_from_uuid_name(entry.uuid, entry.name.clone()));
        }

        None
    }

    #[allow(clippy::missing_const_for_fn)]
    fn profile_from_uuid_name(uuid: Uuid, name: String) -> GameProfile {
        GameProfile {
            id: uuid,
            name,
            properties: ArcSwap::new(Arc::new(vec![])),
            profile_actions: None,
        }
    }
}

/// An argument type to parse one or more [`GameProfile`]s.
///
/// Use [`GameProfileArgumentType::get`] to automatically get a `Vec` of
/// [`GameProfile`]s for an argument by providing a [`CommandContext`] and the
/// argument's name.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct GameProfileArgumentType;

impl ArgumentType<CommandSource> for GameProfileArgumentType {
    type Item = GameProfileResult;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        Self::parse_with_allow_selectors(reader, true)
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::GameProfile
    }

    fn list_suggestions(
        &self,
        context: &CommandContext,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        EntitySelectorParserSuggestions::list_suggestions(context, &builder)
    }

    fn examples(&self) -> Vec<String> {
        examples!("Herobrine", "98765", "@a", "@p[limit=2]")
    }
}

impl GameProfileArgumentType {
    fn parse_with_allow_selectors(
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<<Self as ArgumentType<CommandSource>>::Item, CommandSyntaxError> {
        if reader.peek() == Some('@') {
            // We read a selector variable.
            let parser = EntitySelectorParser::new(reader, allow_selectors);
            let selector = parser.parse_and_consume()?;
            if selector.includes_entities {
                Err(ONLY_PLAYERS_ALLOWED_ERROR_TYPE.create(reader))
            } else {
                Ok(GameProfileResult::Selector(Box::new(selector)))
            }
        } else {
            // We read a UUID or player name.
            let i = reader.cursor();
            while reader.can_read_char() && reader.peek() != Some(' ') {
                reader.skip();
            }
            let string = &reader.string()[i..reader.cursor()];
            Ok(Uuid::try_parse(string).map_or_else(
                |_| GameProfileResult::Name(string.to_owned()),
                GameProfileResult::Uuid,
            ))
        }
    }

    /// Tries to get any number of [`GameProfile`]s from a parsed argument of the provided [`CommandContext`].
    ///
    /// # Warning
    ///
    /// Do not lock write/read access to one of these data locks
    /// before calling this method, as that may cause a *deadlock*:
    /// - `server.data.user_cache`
    /// - `server.data.operator_config`
    /// - `server.data.banned_player_list`
    /// - `server.data.whitelist_config`
    ///
    /// Instead, call this function *before* using `write()`/`read()` on a lock.
    pub fn get(
        context: &CommandContext<'_>,
        name: &str,
    ) -> Result<Vec<GameProfile>, CommandSyntaxError> {
        context
            .get_argument::<GameProfileResult>(name)?
            .resolve(context.source.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::commands::data;
    use crate::data::banlist_serializer::BannedPlayerEntry;
    use pumpkin_config::{op::Op, whitelist::WhitelistEntry};

    #[tokio::test]
    async fn legacy_admin_records_can_be_replaced_and_pardoned_by_name() {
        const CHILD: &str = "FERRALOOM_LEGACY_ADMIN_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // Administrative files use cwd. Run the real commands in a separate
            // process so this test never changes the other tests' working dir.
            let directory = tempfile::tempdir().unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "command::argument_types::game_profile::tests::legacy_admin_records_can_be_replaced_and_pardoned_by_name",
                    "--test-threads=1",
                ])
                .current_dir(directory.path())
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let fixture = data::tests::Fixture::new().await;
        let server = fixture.source.server().clone();
        let name = "SpamObserver";
        let legacy = Uuid::parse_str("14209f43-09d3-4876-d114-4c2568f51fc3").unwrap();
        let canonical = offline_uuid(name).unwrap();
        let original = GameProfileResult::profile_from_uuid_name(legacy, name.into());
        server
            .data
            .operator_config
            .write()
            .unwrap()
            .ops
            .push(Op::new(
                legacy,
                name.into(),
                server.basic_config.op_permission_level,
                false,
            ));
        server
            .data
            .whitelist_config
            .write()
            .unwrap()
            .whitelist
            .push(WhitelistEntry::new(legacy, name.into()));
        server
            .data
            .banned_player_list
            .write()
            .unwrap()
            .banned_players
            .push(BannedPlayerEntry::new(
                &original,
                "test".into(),
                None,
                "legacy".into(),
            ));
        assert_eq!(
            server
                .command_dispatcher
                .load()
                .execute_input("whitelist add SpamObserver", &fixture.source)
                .unwrap(),
            1
        );
        let profile = GameProfileResult::profile_from_uuid_name(canonical, name.into());
        assert!(
            server
                .data
                .whitelist_config
                .read()
                .unwrap()
                .is_whitelisted(&profile)
        );
        assert_eq!(
            server
                .command_dispatcher
                .load()
                .execute_input("op SpamObserver", &fixture.source)
                .unwrap(),
            1
        );
        assert!(
            server
                .data
                .operator_config
                .read()
                .unwrap()
                .ops
                .iter()
                .any(|op| op.uuid == canonical)
        );
        let cached = server
            .data
            .user_cache
            .write()
            .unwrap()
            .get_by_name(name)
            .unwrap();
        assert_eq!(cached.uuid, canonical);

        // A canonical cache entry must not hide the legacy alias from /pardon.
        let unrelated = GameProfileResult::profile_from_uuid_name(Uuid::from_u128(7), name.into());
        server
            .data
            .banned_player_list
            .write()
            .unwrap()
            .banned_players
            .push(BannedPlayerEntry::new(
                &unrelated,
                "test".into(),
                None,
                "ordinary UUID ban".into(),
            ));
        assert_eq!(
            server
                .command_dispatcher
                .load()
                .execute_input("pardon SpamObserver", &fixture.source)
                .unwrap(),
            1
        );
        let mut bans = server.data.banned_player_list.write().unwrap();
        assert!(bans.get_entry(&profile).is_none());
        assert_eq!(bans.banned_players.len(), 1);
        assert_eq!(bans.banned_players[0].uuid, unrelated.id);
    }
}
