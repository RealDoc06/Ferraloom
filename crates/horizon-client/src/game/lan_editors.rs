//! Ferraloom: the server side of what a vanilla client's editors and debug
//! keys ask about blocks and entities. So far F3+I's tag queries
//! (`handleEntityTagQuery`, `handleBlockEntityTagQuery`), answered with the
//! NBT `/data get` reads. Structure, jigsaw and test blocks and command
//! minecarts have no data here yet to edit (no block entity, no command).
use super::{Game, commands::Target, lan::LanHost};
use horizon_protocol::packets::play::{self, Clientbound as C};
use horizon_world::nbt::Tag;

/// `Permissions.COMMANDS_GAMEMASTER`: what F3+I needs.
const GAMEMASTER: u8 = 2;

impl Game<'_> {
    /// The entity `id` in the level in place (`Level.getEntity`), but a player.
    fn entity_with_id(&self, id: i32) -> Option<Target> {
        let id = u32::try_from(id).ok()?;
        let target = if self.mobs.iter().any(|m| m.id == id) {
            Target::Mob(id)
        } else if self.dropped.iter().any(|i| i.id == id) {
            Target::Item(id)
        } else if self.orbs.iter().any(|o| o.id == id) {
            Target::Orb(id)
        } else if self.creatures.projectiles.iter().any(|p| p.id == id) {
            Target::Projectile(id)
        } else if self.armor_stands.iter().any(|a| a.id == id) {
            Target::Stand(id)
        } else if self.item_frames.iter().any(|f| f.id == id) {
            Target::Frame(id)
        } else if self.paintings.iter().any(|p| p.id == id) {
            Target::Painting(id)
        } else if self.boats.iter().any(|b| b.id == id) {
            Target::Boat(id)
        } else if self.carts.iter().any(|c| c.id == id) {
            Target::Cart(id)
        } else if self.others.iter().any(|o| o.id == id) {
            Target::Other(id)
        } else if self.crystals.iter().any(|c| c.id == id) {
            Target::Crystal(id)
        } else if self.dragon.as_ref().is_some_and(|d| d.id == id) {
            Target::Dragon(id)
        } else {
            return None;
        };
        Some(target)
    }

    /// Port of ServerGamePacketListenerImpl#handleEntityTagQuery: a game
    /// master is sent the entity as saved, without its id
    /// (`Entity.saveWithoutId`); nothing when there is no such entity.
    pub(super) fn guest_entity_tag_query(
        &mut self,
        host: &mut LanHost,
        gi: usize,
        transaction: i32,
        id: i32,
    ) {
        if host.hub.guests[gi].permission < GAMEMASTER {
            return;
        }
        let conn = host.hub.guests[gi].conn;
        let player = host
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id == id && g.placed && host.hub.here(g))
            .map(|g| g.conn);
        let tag = match player {
            // A player is read in their own turn, as `/data get entity` reads one.
            Some(other) => self
                .as_guest(host, other, |g| g.entity_nbt(Target::Player))
                .flatten(),
            None => self.entity_with_id(id).and_then(|t| self.entity_nbt(t)),
        };
        let Some(Tag::Compound(mut c)) = tag else {
            return;
        };
        c.remove(&"id".into());
        host.hub.send_to(
            conn,
            &C::TagQuery(play::TagQuery {
                transaction,
                tag: Tag::Compound(c),
            }),
        );
    }

    /// Port of ServerGamePacketListenerImpl#handleBlockEntityTagQuery: a game
    /// master is sent the block entity at `pos` without its id and position
    /// (`BlockEntity.saveWithoutMetadata`), or no tag when there is none.
    pub(super) fn guest_block_tag_query(
        &mut self,
        host: &LanHost,
        gi: usize,
        transaction: i32,
        pos: [i32; 3],
    ) {
        let g = &host.hub.guests[gi];
        if g.permission < GAMEMASTER {
            return;
        }
        // As `/data get block` saves it, less the metadata.
        let key = (pos[0], pos[1], pos[2]);
        let tag = match self.world.block_entities.get(&key) {
            Some(e) => {
                let name = self.data_block_name(pos);
                let mut tag = horizon_client::save::encode_block_entity(key, e, &name);
                self.restore_loot_table(key, &mut tag);
                if let Tag::Compound(c) = &mut tag {
                    for k in ["id", "x", "y", "z", "keepPacked"] {
                        c.remove(&k.into());
                    }
                }
                tag
            }
            None => Tag::End,
        };
        g.send(&C::TagQuery(play::TagQuery { transaction, tag }));
    }
}
