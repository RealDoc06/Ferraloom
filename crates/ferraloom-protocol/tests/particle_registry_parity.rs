//! Fresh vanilla bootstrap/codec vectors; production particle code stays unchanged.
use ferraloom_data::registries::particle_type;
use ferraloom_protocol::{Wire, entity_data::Particle};

fn codec_kind(particle: &Particle) -> &'static str {
    match particle {
        Particle::Block(_)
        | Particle::BlockMarker(_)
        | Particle::FallingDust(_)
        | Particle::DustPillar(_)
        | Particle::BlockCrumble(_) => "BlockParticleOption",
        Particle::Geyser(_) | Particle::GeyserPlume(_) => "GeyserParticleOptions",
        Particle::GeyserBase(_) | Particle::GeyserPoof(_) => "GeyserBaseParticleOptions",
        Particle::DragonBreath(_) => "PowerParticleOption",
        Particle::Dust(_) => "DustParticleOptions",
        Particle::DustColorTransition(_) => "DustColorTransitionOptions",
        Particle::Effect(_) | Particle::InstantEffect(_) => "SpellParticleOption",
        Particle::EntityEffect(_) | Particle::TintedLeaves(_) | Particle::Flash(_) => {
            "ColorParticleOption"
        }
        Particle::SculkCharge(_) => "SculkChargeParticleOptions",
        Particle::Item(_) => "ItemParticleOption",
        Particle::Vibration(_) => "VibrationParticleOption",
        Particle::Trail(_) => "TrailParticleOption",
        Particle::Shriek(_) => "ShriekParticleOption",
        _ => "SimpleParticleType",
    }
}
#[test]
fn particle_registry_names_ids_and_options_codecs_match_vanilla_bootstrap() {
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../ferraloom-data/tests/fixtures/registry_parity/particles.json"
    ))
    .unwrap();
    let entries = native.as_array().unwrap();
    assert_eq!(particle_type::ALL.len(), entries.len());
    for (ordinal, (id, entry)) in particle_type::ALL.iter().zip(entries).enumerate() {
        assert_eq!(usize::from(id.protocol_id()), ordinal);
        assert_eq!(u64::from(id.protocol_id()), entry["id"].as_u64().unwrap());
        assert_eq!(id.name(), entry["name"].as_str().unwrap());
        let wire = entry["wire"].as_str().unwrap();
        assert_eq!(wire.len() % 2, 0);
        let wire = wire
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        // Native ParticleType#codec creates the options, then the actual native
        // registry-dispatched StreamCodec writes them. Decode checks the modeled
        // options kind; exact re-encoding checks its entire typed payload.
        let particle =
            Particle::from_bytes(&wire).unwrap_or_else(|error| panic!("{}: {error}", id.name()));
        assert_eq!(
            codec_kind(&particle),
            entry["codec_kind"].as_str().unwrap(),
            "{} codec",
            id.name()
        );
        assert_eq!(particle.to_bytes().unwrap(), wire, "{} payload", id.name());
    }
}
