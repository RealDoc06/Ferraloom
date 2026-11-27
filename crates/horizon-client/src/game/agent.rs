//! A block state's name as mods read it (`name[k=v,...]`, `server_mods`).
use super::Game;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    pub(super) fn state_string(&self, state: BlockStateId) -> String {
        let Some(s) = self.pack.assets.registry.state(state) else {
            return "?".into();
        };
        if s.properties.is_empty() {
            return s.name.clone();
        }
        let props: Vec<String> = s
            .properties
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("{}[{}]", s.name, props.join(","))
    }
}
