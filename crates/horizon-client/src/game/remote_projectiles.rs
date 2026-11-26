//! Primed TNT, falling blocks, projectiles, tridents, firework rockets and
//! end crystals on a server's client (`ClientPacketListener.handleAddEntity`
//! and `handleSetEntityData`): the server simulates them, the client draws
//! them where it says and interpolates their moves.
use super::Game;
use horizon_client::particles::{ParticleOptions, ParticleType};
use horizon_client::projectiles::ProjectileKind;
use horizon_core::{geometry::Vec3, random::RandomSource};

/// `AreaEffectCloud.DEFAULT_PARTICLE`: a white entity effect.
fn default_cloud_particle() -> ParticleOptions {
    ParticleOptions::colored(ParticleType::EntityEffect, [1.0; 4])
}

/// `AreaEffectCloud.clientTick`: while waiting, now and then two particles
/// near the middle; then about one per square block of the disc, drifting
/// (an entity effect stays still).
pub(super) fn cloud_client_tick(
    particles: &mut horizon_client::particles::ParticleEngine,
    at: [f64; 3],
    radius: f32,
    waiting: bool,
    particle: &ParticleOptions,
) {
    if waiting && particles.random().next_boolean() {
        return;
    }
    let (count, spread) = if waiting {
        (2, 0.2)
    } else {
        (
            (std::f32::consts::PI * radius * radius).ceil() as i32,
            radius,
        )
    };
    for _ in 0..count {
        let r = particles.random();
        let angle = r.next_float() * std::f32::consts::TAU;
        let distance = r.next_float().sqrt() * spread;
        let at = [
            at[0] + f64::from(angle.cos() * distance),
            at[1],
            at[2] + f64::from(angle.sin() * distance),
        ];
        let (options, velocity) = if particle.ty == ParticleType::EntityEffect {
            let default = waiting && r.next_boolean();
            let options = if default {
                default_cloud_particle()
            } else {
                particle.clone()
            };
            (options, [0.0; 3])
        } else if waiting {
            (particle.clone(), [0.0; 3])
        } else {
            let v = [
                (0.5 - r.next_double()) * 0.15,
                f64::from(0.01f32),
                (0.5 - r.next_double()) * 0.15,
            ];
            (particle.clone(), v)
        };
        // `addAlwaysVisibleParticle`.
        particles.add_with(&options, at, velocity, false, true);
    }
}

/// `AbstractHurtingProjectile.getTrailParticle`: smoke for every one of them
/// but the wind charges (no trail) and the dragon's fireball (its own).
pub(super) fn has_smoke_trail(kind: &ProjectileKind) -> bool {
    matches!(
        kind,
        ProjectileKind::SmallFireball
            | ProjectileKind::LargeFireball { .. }
            | ProjectileKind::WitherSkull { .. }
    )
}

impl Game<'_> {
    /// `AbstractHurtingProjectile.createParticleTrail`: one smoke puff half a
    /// block above each position.
    pub(super) fn smoke_trails(&mut self, at: impl IntoIterator<Item = Vec3>) {
        let smoke = ParticleOptions::simple(ParticleType::Smoke);
        for p in at {
            self.particles.add(&smoke, [p.x, p.y + 0.5, p.z], [0.0; 3]);
        }
    }
}
