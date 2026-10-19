//! Port of net.minecraft.server.permissions.
mod operators;
pub use operators::{ServerOpList, ServerOpListEntry};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PermissionLevel {
    All,
    Moderators,
    Gamemasters,
    Admins,
    Owners,
}
impl PermissionLevel {
    /// Port of PermissionLevel#byId (clamps out-of-range IDs).
    pub fn by_id(id: i32) -> Self {
        match id {
            ..=0 => Self::All,
            1 => Self::Moderators,
            2 => Self::Gamemasters,
            3 => Self::Admins,
            _ => Self::Owners,
        }
    }
    pub fn id(self) -> i32 {
        self as i32
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Moderators => "moderators",
            Self::Gamemasters => "gamemasters",
            Self::Admins => "admins",
            Self::Owners => "owners",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    HasCommandLevel(PermissionLevel),
    Atom(String),
}
impl Permission {
    pub const COMMANDS_GAMEMASTER: Self = Self::HasCommandLevel(PermissionLevel::Gamemasters);
    pub const ENTITY_SELECTORS: &'static str = "minecraft:commands/entity_selectors";
}
/// Port of PermissionSet, LevelBasedPermissionSet and PermissionSetUnion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionSet {
    None,
    All,
    Level(PermissionLevel),
    Atoms(Arc<[String]>),
    Union(Arc<[PermissionSet]>),
}
impl PermissionSet {
    pub fn for_level(id: i32) -> Self {
        Self::Level(PermissionLevel::by_id(id))
    }
    pub fn has(&self, permission: &Permission) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Level(level) => match permission {
                Permission::HasCommandLevel(required) => level >= required,
                Permission::Atom(atom) => {
                    atom == Permission::ENTITY_SELECTORS && *level >= PermissionLevel::Gamemasters
                }
            },
            Self::Atoms(atoms) => match permission {
                Permission::Atom(atom) => atoms.contains(atom),
                _ => false,
            },
            Self::Union(sets) => sets.iter().any(|set| set.has(permission)),
        }
    }
    pub fn has_level(&self, level: i32) -> bool {
        self.has(&Permission::HasCommandLevel(PermissionLevel::by_id(level)))
    }
    /// 26.3's level-specialized union returns the LOWER level. Checked against Java.
    pub fn union(self, other: Self) -> Self {
        if let (Self::Level(a), Self::Level(b)) = (&self, &other) {
            return Self::Level((*a).min(*b));
        }
        Self::Union(Arc::from([self, other]))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn level_union_matches_java_even_when_it_reduces_permissions() {
        let expected: [i32; 2] =
            serde_json::from_str(include_str!("vanilla/level-unions.json")).unwrap();
        for (a, b, expected) in [(4, 0, expected[0]), (0, 4, expected[1])] {
            assert_eq!(
                PermissionSet::for_level(a).union(PermissionSet::for_level(b)),
                PermissionSet::for_level(expected)
            );
        }
        assert!(
            !PermissionSet::for_level(0)
                .has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()))
        );
        assert!(
            PermissionSet::for_level(2).has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()))
        );
        assert!(
            !PermissionSet::for_level(4)
                .has(&Permission::Atom("minecraft:chat/send_messages".into()))
        );
    }
}
