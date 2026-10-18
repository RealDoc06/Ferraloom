use ferraloom_protocol::component::{
    CollectionPredicate, IntBounds, MobEffectInstance, MobEffectsPredicate,
};
/// Port of CollectionPredicate and MobEffectsPredicate#test, for brewing inputs.
pub(super) fn test_effects(
    predicate: &CollectionPredicate<MobEffectsPredicate>,
    effects: &[MobEffectInstance],
) -> bool {
    if predicate
        .size
        .as_ref()
        .is_some_and(|bounds| !bounds_match(bounds, effects.len() as i32))
    {
        return false;
    }
    if let Some(contains) = &predicate.contains {
        for predicate in contains {
            if !effects.iter().any(|effect| test(predicate, effect)) {
                return false;
            }
        }
    }
    if let Some(counts) = &predicate.count {
        for count in counts {
            if !bounds_match(
                &count.count,
                effects
                    .iter()
                    .filter(|effect| test(&count.test, effect))
                    .count() as i32,
            ) {
                return false;
            }
        }
    }
    true
}
fn bounds_match(bounds: &IntBounds, value: i32) -> bool {
    bounds.min.is_none_or(|min| value >= min) && bounds.max.is_none_or(|max| value <= max)
}
fn test(predicate: &MobEffectsPredicate, effect: &MobEffectInstance) -> bool {
    let Some((_, predicate)) = predicate.0.iter().find(|(key, _)| key == &effect.effect) else {
        return false;
    };
    bounds_match(&predicate.amplifier, effect.details.amplifier.0)
        && bounds_match(&predicate.duration, effect.details.duration.0)
        && predicate
            .ambient
            .is_none_or(|ambient| ambient == effect.details.ambient)
        && predicate
            .visible
            .is_none_or(|visible| visible == effect.details.show_particles)
}
