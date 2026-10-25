//! CraftingTableBlock and the six other workstation menu providers/openers.
use crate::{
    block::{BlockBehavior, BlockMethod, InteractionResult, UseContext},
    entity::EntityRef,
    inventory::{
        self, AbstractContainerMenu, ContainerLevelAccess, MenuError, MenuKind, MenuProvider,
    },
    level::{Level, LevelError},
    stats::{Stat, custom},
};
use ferraloom_data::{Block, BlockClass, BlockState};
use ferraloom_protocol::chat::Component;
use ferraloom_util::BlockPos;

struct Specification {
    kind: MenuKind,
    title: &'static str,
    stat: custom::Id,
}
fn specification(block: Block) -> Option<Specification> {
    let (kind, title, stat) = match block.class() {
        BlockClass::CraftingTableBlock => (
            MenuKind::Crafting,
            "container.crafting",
            custom::Id::INTERACT_WITH_CRAFTING_TABLE,
        ),
        BlockClass::AnvilBlock => (
            MenuKind::Anvil,
            "container.repair",
            custom::Id::INTERACT_WITH_ANVIL,
        ),
        BlockClass::LoomBlock => (
            MenuKind::Loom,
            "container.loom",
            custom::Id::INTERACT_WITH_LOOM,
        ),
        BlockClass::StonecutterBlock => (
            MenuKind::Stonecutter,
            "container.stonecutter",
            custom::Id::INTERACT_WITH_STONECUTTER,
        ),
        BlockClass::GrindstoneBlock => (
            MenuKind::Grindstone,
            "container.grindstone_title",
            custom::Id::INTERACT_WITH_GRINDSTONE,
        ),
        BlockClass::SmithingTableBlock => (
            MenuKind::Smithing,
            "container.upgrade",
            custom::Id::INTERACT_WITH_SMITHING_TABLE,
        ),
        BlockClass::CartographyTableBlock => (
            MenuKind::Cartography,
            "container.cartography_table",
            custom::Id::INTERACT_WITH_CARTOGRAPHY_TABLE,
        ),
        _ => return None,
    };
    Some(Specification { kind, title, stat })
}
fn level_error(error: MenuError) -> LevelError {
    match error {
        MenuError::Level(error) => error,
        MenuError::Unsupported(operation) => LevelError::Inventory(operation),
        _ => LevelError::Inventory("workstation menu"),
    }
}

/// Port of each workstation's SimpleMenuProvider. Capture the original level
/// identity and position before openMenu can close the player's previous menu.
struct WorkstationProvider {
    kind: MenuKind,
    title: &'static str,
    access: ContainerLevelAccess,
}
impl MenuProvider for WorkstationProvider {
    fn display_name(&self) -> Component {
        Component::translatable(self.title, Vec::new())
    }
    fn create_menu(
        &self,
        id: i32,
        _player: EntityRef,
        level: &mut Level,
    ) -> inventory::Result<Option<AbstractContainerMenu>> {
        let mut menu =
            AbstractContainerMenu::new_bound_workstation(self.kind, id, self.access.pos, level)?;
        // The constructor has no callbacks. Retain the provider's original
        // access if closing the previous menu changed the current dimension.
        menu.level_access = Some(self.access.clone());
        Ok(Some(menu))
    }
}

struct WorkstationBlock;
static WORKSTATION: WorkstationBlock = WorkstationBlock;
pub(super) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    (matches!(
        method,
        BlockMethod::GetMenuProvider | BlockMethod::UseWithoutItem
    ) && specification(block).is_some())
    .then_some(&WORKSTATION)
}
impl BlockBehavior for WorkstationBlock {
    /// Port of CraftingTable/Anvil/Loom/Stonecutter/Grindstone/Smithing/Cartography
    /// getMenuProvider, with the same native kind, title and captured access.
    fn get_menu_provider(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &Level,
    ) -> Result<Option<Box<dyn MenuProvider>>, LevelError> {
        let spec = specification(state.block()).expect("registered workstation");
        let access = ContainerLevelAccess::create(level, pos).map_err(level_error)?;
        Ok(Some(Box::new(WorkstationProvider {
            kind: spec.kind,
            title: spec.title,
            access,
        })))
    }
    /// Port of the seven useWithoutItem implementations: virtual provider read,
    /// synchronous openMenu, interaction stat, then SUCCESS.
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let spec = specification(state.block()).expect("registered workstation");
        let provider = level
            .required_block_behavior(state.block(), BlockMethod::GetMenuProvider)?
            .get_menu_provider(state, pos, level)?;
        if let Some(provider) = provider {
            inventory::open_menu_live(context.player, provider.as_ref(), level)
                .map_err(level_error)?;
        }
        crate::entity::player::award_stat(context.player, Stat::Custom(spec.stat), 1, level)?;
        Ok(InteractionResult::SUCCESS)
    }
}
