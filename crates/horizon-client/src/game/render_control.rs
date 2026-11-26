use horizon_client::Result;
use horizon_world::registry::BlockStateId;

/// Parse `name[k=v,...]` into a registry state (defaults for missing properties).
pub(super) fn parse_state(
    registry: &horizon_world::registry::BlockRegistry,
    text: &str,
) -> Result<BlockStateId> {
    let (name, props) = match text.split_once('[') {
        Some((n, rest)) => (n, rest.trim_end_matches(']')),
        None => (text, ""),
    };
    let name = if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    };
    let default = registry
        .default_state(&name)
        .ok_or_else(|| format!("unknown block {name}"))?;
    let mut state = registry
        .state(default)
        .ok_or("missing default state")?
        .clone();
    for pair in props.split(',').filter(|p| !p.is_empty()) {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| format!("bad property {pair}"))?;
        if !state.properties.contains_key(k) {
            return Err(format!("{name} has no property {k}").into());
        }
        state.properties.insert(k.to_owned(), v.to_owned());
    }
    Ok(registry.resolve(&state)?)
}
