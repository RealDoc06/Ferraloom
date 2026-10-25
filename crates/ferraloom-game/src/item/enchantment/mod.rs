//! Native enchantment definitions, conditions and effect execution.
mod value_effect;
pub use value_effect::EnchantmentValueEffect;
mod conditional;
pub use conditional::{ConditionalEffect, EnchantmentTarget, TargetedConditionalEffect};
mod definition;
pub use definition::{EnchantmentCost, EnchantmentDefinition};
mod attribute;
pub mod helper;
mod registry;
pub use attribute::EnchantmentAttributeEffect;
pub use registry::{Enchantment, EnchantmentEffect, EnchantmentRegistry};
mod value_component;
pub use value_component::EnchantmentValueEffectComponent;
