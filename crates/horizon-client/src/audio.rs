//! The sounds the game's logic plays. A server plays none itself: a
//! positional sound played while the world is served is kept (`capture`)
//! for its players, whose clients play it from the sound packet
//! (`ServerLevel.playSeededSound`).
use crate::sound_events::SoundRegistry;
use std::path::Path;

pub struct Audio {
    /// The sound events (`sounds.json`), whose names commands suggest.
    registry: SoundRegistry,
    /// Positional sounds played since the last drain, kept while the world
    /// is open to the network so guests hear them too.
    pub capture: Option<Vec<(String, [f64; 3], f32, f32)>>,
}

impl Audio {
    /// The sound events of the resource root, and nothing captured yet.
    pub fn silent(root: &Path) -> Self {
        Self {
            registry: SoundRegistry::load(root),
            capture: None,
        }
    }
    /// A sound event at a world position, kept for the players while the
    /// world is served; without a position it is the window's own and
    /// nobody else hears it.
    pub fn play(&mut self, event: &str, at: Option<[f64; 3]>, volume: f32, pitch: f32) {
        if let (Some(capture), Some(at)) = (self.capture.as_mut(), at) {
            capture.push((event.to_owned(), at, volume, pitch));
        }
    }
    /// `play` for the window's own client only (`playLocalSound`): the guests
    /// play it from the packet that caused it, so nothing is kept.
    pub fn play_unshared(
        &mut self,
        _event: &str,
        _at: Option<[f64; 3]>,
        _volume: f32,
        _pitch: f32,
    ) {
    }
    /// `play` on a given `SoundSource` index (`/playsound`); the players'
    /// packets take the source from the event.
    pub fn play_on(
        &mut self,
        _source: usize,
        event: &str,
        at: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
    ) {
        self.play(event, at, volume, pitch);
    }
    /// Every sound event's name, sorted.
    pub fn event_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.registry.names().cloned().collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only positional sounds played while capturing are kept.
    #[test]
    fn capture_keeps_positional_sounds_while_serving() {
        let mut audio = Audio::silent(Path::new("/nonexistent/horizon-audio-test"));
        audio.play(
            "minecraft:block.stone.break",
            Some([1.0, 2.0, 3.0]),
            1.0,
            1.0,
        );
        assert!(audio.capture.is_none());
        audio.capture = Some(Vec::new());
        audio.play("minecraft:ui.button.click", None, 1.0, 1.0);
        audio.play_unshared("minecraft:item.totem.use", Some([0.0; 3]), 1.0, 1.0);
        audio.play_on(
            3,
            "minecraft:entity.cow.ambient",
            Some([4.0, 5.0, 6.0]),
            0.5,
            2.0,
        );
        assert_eq!(
            audio.capture.unwrap(),
            vec![(
                "minecraft:entity.cow.ambient".to_owned(),
                [4.0, 5.0, 6.0],
                0.5,
                2.0
            )]
        );
    }
}
