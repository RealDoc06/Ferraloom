use super::{DamageSource, DamageTypeHolder, DamageTypeKey};
use crate::entity::EntityRef;
use ferraloom_util::Vec3;

/// Port of DamageSources. Environmental sources are built once in native
/// constructor order; factories retain the same immutable registry holders.
#[derive(Debug)]
pub struct DamageSources {
    builtin: Box<[DamageTypeHolder]>,
    environmental: [DamageSource; 25],
}
impl DamageSources {
    pub(super) fn new(builtin: Box<[DamageTypeHolder]>) -> Self {
        let environmental = [
            DamageSource::new(builtin[DamageTypeKey::InFire as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Campfire as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::LightningBolt as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::OnFire as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Lava as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::HotFloor as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::InWall as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Cramming as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Drown as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Starve as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Cactus as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Fall as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::EnderPearl as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::FlyIntoWall as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::FellOutOfWorld as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Generic as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Magic as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Wither as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::DragonBreath as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::DryOut as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::SweetBerryBush as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Freeze as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::Stalagmite as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::OutsideBorder as usize].clone()),
            DamageSource::new(builtin[DamageTypeKey::GenericKill as usize].clone()),
        ];
        Self {
            builtin,
            environmental,
        }
    }
    pub fn holder(&self, key: DamageTypeKey) -> &DamageTypeHolder {
        &self.builtin[key as usize]
    }
    pub fn source(&self, key: DamageTypeKey) -> DamageSource {
        DamageSource::new(self.holder(key).clone())
    }
    fn caused(&self, key: DamageTypeKey, cause: EntityRef) -> DamageSource {
        DamageSource::with_cause(self.holder(key).clone(), Some(cause))
    }
    fn indirect(
        &self,
        key: DamageTypeKey,
        direct: EntityRef,
        cause: Option<EntityRef>,
    ) -> DamageSource {
        DamageSource::with_entities(self.holder(key).clone(), Some(direct), cause)
    }
    pub fn in_fire(&self) -> DamageSource {
        self.environmental[0].clone()
    }
    pub fn campfire(&self) -> DamageSource {
        self.environmental[1].clone()
    }
    pub fn lightning_bolt(&self) -> DamageSource {
        self.environmental[2].clone()
    }
    pub fn on_fire(&self) -> DamageSource {
        self.environmental[3].clone()
    }
    pub fn lava(&self) -> DamageSource {
        self.environmental[4].clone()
    }
    pub fn hot_floor(&self) -> DamageSource {
        self.environmental[5].clone()
    }
    pub fn in_wall(&self) -> DamageSource {
        self.environmental[6].clone()
    }
    pub fn cramming(&self) -> DamageSource {
        self.environmental[7].clone()
    }
    pub fn drown(&self) -> DamageSource {
        self.environmental[8].clone()
    }
    pub fn starve(&self) -> DamageSource {
        self.environmental[9].clone()
    }
    pub fn cactus(&self) -> DamageSource {
        self.environmental[10].clone()
    }
    pub fn fall(&self) -> DamageSource {
        self.environmental[11].clone()
    }
    pub fn ender_pearl(&self) -> DamageSource {
        self.environmental[12].clone()
    }
    pub fn fly_into_wall(&self) -> DamageSource {
        self.environmental[13].clone()
    }
    pub fn fell_out_of_world(&self) -> DamageSource {
        self.environmental[14].clone()
    }
    pub fn generic(&self) -> DamageSource {
        self.environmental[15].clone()
    }
    pub fn magic(&self) -> DamageSource {
        self.environmental[16].clone()
    }
    pub fn wither(&self) -> DamageSource {
        self.environmental[17].clone()
    }
    pub fn dragon_breath(&self) -> DamageSource {
        self.environmental[18].clone()
    }
    pub fn dry_out(&self) -> DamageSource {
        self.environmental[19].clone()
    }
    pub fn sweet_berry_bush(&self) -> DamageSource {
        self.environmental[20].clone()
    }
    pub fn freeze(&self) -> DamageSource {
        self.environmental[21].clone()
    }
    pub fn stalagmite(&self) -> DamageSource {
        self.environmental[22].clone()
    }
    pub fn out_of_border(&self) -> DamageSource {
        self.environmental[23].clone()
    }
    pub fn generic_kill(&self) -> DamageSource {
        self.environmental[24].clone()
    }
    pub fn sting(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::Sting, cause)
    }
    pub fn mob_attack(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::MobAttack, cause)
    }
    pub fn no_aggro_mob_attack(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::MobAttackNoAggro, cause)
    }
    pub fn player_attack(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::PlayerAttack, cause)
    }
    pub fn thorns(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::Thorns, cause)
    }
    pub fn sonic_boom(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::SonicBoom, cause)
    }
    pub fn mace(&self, cause: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::MaceSmash, cause)
    }
    pub fn falling_block(&self, direct: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::FallingBlock, direct)
    }
    pub fn anvil(&self, direct: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::FallingAnvil, direct)
    }
    pub fn falling_stalactite(&self, direct: EntityRef) -> DamageSource {
        self.caused(DamageTypeKey::FallingStalactite, direct)
    }
    pub fn arrow(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::Arrow, direct, cause)
    }
    pub fn trident(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::Trident, direct, cause)
    }
    pub fn mob_projectile(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::MobProjectile, direct, cause)
    }
    pub fn spit(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::Spit, direct, cause)
    }
    pub fn wind_charge(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::WindCharge, direct, cause)
    }
    pub fn fireworks(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::Fireworks, direct, cause)
    }
    pub fn wither_skull(&self, direct: EntityRef, cause: EntityRef) -> DamageSource {
        self.indirect(DamageTypeKey::WitherSkull, direct, Some(cause))
    }
    pub fn thrown(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::Thrown, direct, cause)
    }
    pub fn indirect_magic(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        self.indirect(DamageTypeKey::IndirectMagic, direct, cause)
    }
    pub fn fireball(&self, direct: EntityRef, cause: Option<EntityRef>) -> DamageSource {
        match cause {
            None => self.caused(DamageTypeKey::UnattributedFireball, direct),
            Some(cause) => self.indirect(DamageTypeKey::Fireball, direct, Some(cause)),
        }
    }
    pub fn explosion(&self, direct: Option<EntityRef>, cause: Option<EntityRef>) -> DamageSource {
        let key = if direct.is_some() && cause.is_some() {
            DamageTypeKey::PlayerExplosion
        } else {
            DamageTypeKey::Explosion
        };
        DamageSource::with_entities(self.holder(key).clone(), direct, cause)
    }
    pub fn bad_respawn_point_explosion(&self, position: Vec3) -> DamageSource {
        DamageSource::at_position(
            self.holder(DamageTypeKey::BadRespawnPoint).clone(),
            position,
        )
    }
}
