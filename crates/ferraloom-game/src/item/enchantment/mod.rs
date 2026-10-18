//! Native enchantment definitions, conditions and effect execution.
mod value_effect;
pub use value_effect::EnchantmentValueEffect;
mod conditional;
pub use conditional::{ConditionalEffect, EnchantmentTarget, TargetedConditionalEffect};
mod definition;
pub use definition::{EnchantmentCost, EnchantmentDefinition};
