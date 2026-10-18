use super::{
    CraftingContext, CraftingInputAccess, Recipe, Result, StackedItemContents,
    recipe::{RecipeKind, ShapedRecipePattern},
    stack,
    stacked_contents::{crafting_remainder, default_crafting_remainders},
};
use ferraloom_protocol::{
    codec::VarInt,
    component::{
        BoundedList, ComponentTypeId as Id, DataComponent as C, DyeColor, FireworkExplosion,
        FireworkShape, Fireworks, ItemEnchantments, ItemStack, MapPostProcessing, PotDecorations,
    },
};

impl Recipe {
    pub(super) fn matches_crafting(
        &self,
        input: &dyn CraftingInputAccess,
        context: &impl CraftingContext,
    ) -> Result<bool> {
        use RecipeKind::*;
        let occupied: Vec<_> = input.iter().filter(|stack| !stack.is_empty()).collect();
        let count = occupied.len();
        Ok(match &self.kind {
            Shaped { pattern, .. } => pattern.matches(input),
            Shapeless { ingredients, .. } => {
                count == ingredients.len()
                    && StackedItemContents::from_grid(input)
                        .pick(ingredients, 1)
                        .is_some()
            }
            Imbue {
                source, material, ..
            } => {
                input.width() == 3
                    && input.height() == 3
                    && count == 9
                    && (0..9).all(|i| {
                        if i == 4 { source } else { material }.test(input.get_item_index(i))
                    })
            }
            Dye { target, dye, .. } => {
                if count < 2 {
                    return Ok(false);
                }
                let mut targets = 0;
                let mut dyes = 0;
                for stack in occupied {
                    if target.test(stack) {
                        targets += 1;
                    } else if dye.test(stack) && stack::component(stack, Id::DYE)?.is_some() {
                        dyes += 1;
                    } else {
                        return Ok(false);
                    }
                }
                targets == 1 && dyes > 0
            }
            Transmute {
                input: source,
                material,
                material_count,
                result,
                add_material_count,
            } => {
                if count < material_count.min.unwrap_or(1) as usize + 1
                    || count > material_count.max.unwrap_or(8) as usize + 1
                {
                    return Ok(false);
                }
                let mut found = None;
                let mut materials = 0;
                for stack in occupied {
                    if source.test(stack) {
                        if found.is_some() {
                            return Ok(false);
                        }
                        found = Some(stack);
                    } else if material.test(stack) {
                        materials += 1;
                    } else {
                        return Ok(false);
                    }
                }
                let Some(source) = found else {
                    return Ok(false);
                };
                if !material_count.matches(materials) {
                    return Ok(false);
                }
                let result_count = result.count + if *add_material_count { materials } else { 0 };
                if result_count != 1 {
                    true
                } else {
                    let assembled = stack::transmute(
                        &result.resolve(stack::item(source), result.count),
                        source,
                        0,
                    );
                    !assembled.is_empty() && !stack::same_item_components(source, &assembled)?
                }
            }
            DecoratedPot { walls, .. } => {
                input.width() == 3
                    && input.height() == 3
                    && count == 4
                    && [1, 3, 5, 7]
                        .into_iter()
                        .zip(walls)
                        .all(|(i, wall)| wall.test(input.get_item_index(i)))
            }
            BookCloning {
                source,
                material,
                allowed_generations,
                ..
            } => {
                if count < 2 {
                    return Ok(false);
                }
                let mut sources = 0;
                let mut materials = 0;
                for stack in occupied {
                    if source.test(stack) {
                        let Some(C::WrittenBookContent(content)) =
                            stack::component(stack, Id::WRITTEN_BOOK_CONTENT)?
                        else {
                            return Ok(false);
                        };
                        if !allowed_generations.matches(content.generation.0) {
                            return Ok(false);
                        }
                        sources += 1;
                    } else if material.test(stack) {
                        materials += 1;
                    } else {
                        return Ok(false);
                    }
                }
                sources == 1 && materials > 0
            }
            MapExtending { map, material, .. } => {
                let ingredients = (0..9)
                    .map(|i| {
                        Some(if i == 4 {
                            map.clone()
                        } else {
                            material.clone()
                        })
                    })
                    .collect();
                if !ShapedRecipePattern::new(3, 3, ingredients).matches(input) {
                    return Ok(false);
                }
                for stack in occupied {
                    if let Some(C::MapId(id)) = stack::component(stack, Id::MAP_ID)? {
                        return Ok(context.map_scale(id.0)?.is_some_and(|scale| scale < 4));
                    }
                }
                false
            }
            BannerDuplicate { banner, .. } => {
                if count != 2 {
                    return Ok(false);
                }
                let mut color = None;
                let mut sources = 0;
                let mut targets = 0;
                for stack in occupied {
                    let Some(c) = stack::item(stack).data().banner_color else {
                        return Ok(false);
                    };
                    if !banner.test(stack) || color.is_some_and(|color| color != c) {
                        return Ok(false);
                    }
                    color = Some(c);
                    let n = banner_patterns(stack)?.len();
                    if n > 6 {
                        return Ok(false);
                    }
                    if n == 0 {
                        targets += 1;
                    } else {
                        sources += 1;
                    }
                }
                sources == 1 && targets == 1
            }
            ShieldDecoration { banner, target, .. } => {
                if count != 2 {
                    return Ok(false);
                }
                let mut banners = 0;
                let mut targets = 0;
                for stack in occupied {
                    if banner.test(stack) && stack::item(stack).data().banner_color.is_some() {
                        banners += 1;
                    } else if target.test(stack) && banner_patterns(stack)?.is_empty() {
                        targets += 1;
                    } else {
                        return Ok(false);
                    }
                }
                banners == 1 && targets == 1
            }
            RepairItem => repair_pair(input)?.is_some(),
            FireworkRocket {
                shell, fuel, star, ..
            } => {
                if count < 2 {
                    return Ok(false);
                }
                let mut shells = 0;
                let mut fuels = 0;
                for stack in occupied {
                    if shell.test(stack) {
                        shells += 1;
                    } else if fuel.test(stack) {
                        fuels += 1;
                    } else if !star.test(stack) {
                        return Ok(false);
                    }
                }
                shells == 1 && (1..=3).contains(&fuels)
            }
            FireworkStar {
                shapes,
                trail,
                twinkle,
                fuel,
                dye,
                ..
            } => {
                if count < 2 {
                    return Ok(false);
                }
                let (mut fuels, mut dyes, mut trails, mut twinkles, mut shape_count) =
                    (0, 0, 0, 0, 0);
                for stack in occupied {
                    if twinkle.test(stack) {
                        twinkles += 1;
                    } else if trail.test(stack) {
                        trails += 1;
                    } else if fuel.test(stack) {
                        fuels += 1;
                    } else if dye.test(stack) && stack::component(stack, Id::DYE)?.is_some() {
                        dyes += 1;
                    } else if shapes.iter().any(|(_, ingredient)| ingredient.test(stack)) {
                        shape_count += 1;
                    } else {
                        return Ok(false);
                    }
                }
                fuels == 1 && dyes > 0 && trails <= 1 && twinkles <= 1 && shape_count <= 1
            }
            FireworkStarFade { target, dye, .. } => {
                if count < 2 {
                    return Ok(false);
                }
                let mut targets = 0;
                let mut dyes = 0;
                for stack in occupied {
                    if dye.test(stack) && stack::component(stack, Id::DYE)?.is_some() {
                        dyes += 1;
                    } else if target.test(stack) {
                        targets += 1;
                    } else {
                        return Ok(false);
                    }
                }
                targets == 1 && dyes > 0
            }
            _ => false,
        })
    }
    pub(super) fn assemble_crafting(
        &self,
        input: &dyn CraftingInputAccess,
        context: &impl CraftingContext,
    ) -> Result<ItemStack> {
        use RecipeKind::*;
        match &self.kind {
            Shaped { result, .. } | Shapeless { result, .. } => Ok(stack::create(result)),
            Imbue { result, .. } => {
                let mut result = stack::create(result);
                match stack::component(input.get_item(1, 1), Id::POTION_CONTENTS)? {
                    Some(component) => stack::set(&mut result, component),
                    None => stack::remove(&mut result, Id::POTION_CONTENTS),
                }
                Ok(result)
            }
            Transmute {
                input: source,
                material,
                result,
                add_material_count,
                ..
            } => {
                let mut sources = input
                    .iter()
                    .filter(|stack| !stack.is_empty() && source.test(stack));
                let source = if *add_material_count {
                    sources.next_back()
                } else {
                    sources.next()
                };
                let Some(source) = source else {
                    return Ok(ItemStack::default());
                };
                let materials = if *add_material_count {
                    input
                        .iter()
                        .filter(|stack| {
                            !stack.is_empty()
                                && !self_source(&self.kind, stack)
                                && material.test(stack)
                        })
                        .count() as i32
                } else {
                    0
                };
                Ok(stack::transmute(
                    &result.resolve(stack::item(source), result.count),
                    source,
                    materials,
                ))
            }
            Dye {
                target,
                dye,
                result,
            } => {
                let mut source = None;
                let mut dyes = Vec::new();
                for item in input.iter().filter(|s| !s.is_empty()) {
                    if target.test(item) {
                        if source.is_some() {
                            return Ok(ItemStack::default());
                        }
                        source = Some(item);
                    } else if dye.test(item) {
                        dyes.push(dye_color(item)?);
                    } else {
                        return Ok(ItemStack::default());
                    }
                }
                let Some(source) = source else {
                    return Ok(ItemStack::default());
                };
                if dyes.is_empty() {
                    return Ok(ItemStack::default());
                }
                let current = match stack::component(source, Id::DYED_COLOR)? {
                    Some(C::DyedColor(rgb)) => Some(rgb),
                    _ => None,
                };
                let mut output = stack::transmute(result, source, 0);
                stack::set(&mut output, C::DyedColor(mix_dyes(current, &dyes)));
                Ok(output)
            }
            DecoratedPot { result, .. } => {
                let template = |index: usize| {
                    let stack = input.get_item_index(index);
                    stack
                        .0
                        .as_ref()
                        .map(|stack| ferraloom_protocol::component::ItemStackTemplate {
                            item: stack.item,
                            count: 1,
                            components: stack.components.clone(),
                        })
                };
                let decorations = PotDecorations {
                    back: template(1),
                    left: template(3),
                    right: template(5),
                    front: template(7),
                };
                // ItemStackTemplate#apply overlays the template after the generated patch.
                let mut patch = ferraloom_protocol::component::DataComponentPatch::default();
                patch.set(C::PotDecorations(decorations));
                stack::apply_patch(&mut patch, &result.components);
                Ok(stack::create(
                    &ferraloom_protocol::component::ItemStackTemplate {
                        item: result.item,
                        count: result.count,
                        components: patch,
                    },
                ))
            }
            BookCloning {
                source,
                material,
                result,
                ..
            } => {
                let mut found = None;
                let mut count = 0;
                for stack in input.iter().filter(|s| !s.is_empty()) {
                    if source.test(stack)
                        && stack::component(stack, Id::WRITTEN_BOOK_CONTENT)?.is_some()
                    {
                        if found.is_some() {
                            return Ok(ItemStack::default());
                        }
                        found = Some(stack);
                    } else if material.test(stack) {
                        count += 1;
                    } else {
                        return Ok(ItemStack::default());
                    }
                }
                let Some(source) = found else {
                    return Ok(ItemStack::default());
                };
                let Some(C::WrittenBookContent(mut content)) =
                    stack::component(source, Id::WRITTEN_BOOK_CONTENT)?
                else {
                    return Ok(ItemStack::default());
                };
                content.generation.0 += 1;
                let mut result = stack::transmute(result, source, count - 1);
                stack::set(&mut result, C::WrittenBookContent(content));
                Ok(result)
            }
            MapExtending { result, .. } => {
                for source in input.iter() {
                    if stack::component(source, Id::MAP_ID)?.is_some() {
                        let mut result = stack::transmute(
                            &result.resolve(stack::item(source), result.count),
                            source,
                            0,
                        );
                        stack::set(&mut result, C::MapPostProcessing(MapPostProcessing::Scale));
                        return Ok(result);
                    }
                }
                Ok(ItemStack::default())
            }
            BannerDuplicate { result, .. } => {
                for source in input.iter() {
                    let count = banner_patterns(source)?.len();
                    if !source.is_empty() && (1..=6).contains(&count) {
                        return Ok(stack::transmute(result, source, 0));
                    }
                }
                Ok(ItemStack::default())
            }
            ShieldDecoration {
                banner,
                target,
                result,
            } => {
                let mut found = None;
                let mut patterns = None;
                let mut color = DyeColor::White;
                for source in input.iter().filter(|s| !s.is_empty()) {
                    if banner.test(source)
                        && let Some(name) = stack::item(source).data().banner_color
                    {
                        patterns = stack::component(source, Id::BANNER_PATTERNS)?;
                        color = <DyeColor as ferraloom_protocol::component::Persistent>::from_json(
                            &serde_json::Value::String(name.into()),
                        )?;
                    } else if target.test(source) {
                        found = Some(source);
                    }
                }
                let empty = ItemStack::default();
                let source = found.unwrap_or(&empty);
                let mut result = stack::transmute(result, source, 0);
                match patterns {
                    Some(patterns) => stack::set(&mut result, patterns),
                    None => stack::remove(&mut result, Id::BANNER_PATTERNS),
                }
                stack::set(&mut result, C::BaseColor(color));
                Ok(result)
            }
            RepairItem => assemble_repair(input, context),
            FireworkRocket {
                fuel, star, result, ..
            } => {
                let mut count = 0;
                let mut explosions = Vec::new();
                for source in input.iter().filter(|s| !s.is_empty()) {
                    if fuel.test(source) {
                        count += 1;
                    } else if star.test(source)
                        && let Some(C::FireworkExplosion(explosion)) =
                            stack::component(source, Id::FIREWORK_EXPLOSION)?
                    {
                        explosions.push(explosion);
                    }
                }
                let mut patch = ferraloom_protocol::component::DataComponentPatch::default();
                patch.set(C::Fireworks(Fireworks {
                    flight_duration: VarInt(count),
                    explosions: BoundedList(explosions),
                }));
                stack::apply_patch(&mut patch, &result.components);
                Ok(stack::create(
                    &ferraloom_protocol::component::ItemStackTemplate {
                        item: result.item,
                        count: result.count,
                        components: patch,
                    },
                ))
            }
            FireworkStar {
                shapes,
                trail,
                twinkle,
                dye,
                result,
                ..
            } => {
                let mut shape = FireworkShape::SmallBall;
                let mut has_trail = false;
                let mut has_twinkle = false;
                let mut colors = Vec::new();
                for source in input.iter().filter(|s| !s.is_empty()) {
                    if let Some((found, _)) = shapes
                        .iter()
                        .find(|(_, ingredient)| ingredient.test(source))
                    {
                        shape = *found;
                    } else if twinkle.test(source) {
                        has_twinkle = true;
                    } else if trail.test(source) {
                        has_trail = true;
                    } else if dye.test(source) {
                        colors.push(firework_color(dye_color(source)?));
                    }
                }
                let mut result = stack::create(result);
                stack::set(
                    &mut result,
                    C::FireworkExplosion(FireworkExplosion {
                        shape,
                        colors,
                        fade_colors: Vec::new(),
                        has_trail,
                        has_twinkle,
                    }),
                );
                Ok(result)
            }
            FireworkStarFade {
                target,
                dye,
                result,
            } => {
                let mut found = None;
                let mut colors = Vec::new();
                for source in input.iter() {
                    if dye.test(source) {
                        colors.push(firework_color(dye_color(source)?));
                    } else if target.test(source) {
                        found = Some(source);
                    }
                }
                let Some(source) = found else {
                    return Ok(ItemStack::default());
                };
                if colors.is_empty() {
                    return Ok(ItemStack::default());
                }
                let mut result = stack::transmute(result, source, 0);
                let mut explosion = match stack::component(&result, Id::FIREWORK_EXPLOSION)? {
                    Some(C::FireworkExplosion(explosion)) => explosion,
                    _ => FireworkExplosion {
                        shape: FireworkShape::SmallBall,
                        colors: Vec::new(),
                        fade_colors: Vec::new(),
                        has_trail: false,
                        has_twinkle: false,
                    },
                };
                explosion.fade_colors = colors;
                stack::set(&mut result, C::FireworkExplosion(explosion));
                Ok(result)
            }
            _ => Ok(ItemStack::default()),
        }
    }
    /// Port of CraftingRecipe#getRemainingItems and the book/banner overrides.
    pub fn remaining_items(&self, input: &dyn CraftingInputAccess) -> Result<Vec<ItemStack>> {
        let mut result = vec![ItemStack::default(); input.size()];
        match self.kind {
            RecipeKind::BookCloning { .. } | RecipeKind::BannerDuplicate { .. } => {
                for (index, source) in input.iter().enumerate() {
                    let remainder = crafting_remainder(stack::item(source))?;
                    if !remainder.is_empty() {
                        result[index] = remainder;
                    } else if matches!(self.kind, RecipeKind::BookCloning { .. })
                        && stack::component(source, Id::WRITTEN_BOOK_CONTENT)?.is_some()
                    {
                        result[index] = stack::copy_with_count(source, 1);
                        break;
                    } else if matches!(self.kind, RecipeKind::BannerDuplicate { .. })
                        && !banner_patterns(source)?.is_empty()
                    {
                        result[index] = stack::copy_with_count(source, 1);
                    }
                }
                Ok(result)
            }
            _ => default_crafting_remainders(input),
        }
    }
}
fn self_source(kind: &RecipeKind, stack: &ItemStack) -> bool {
    matches!(kind,RecipeKind::Transmute{input,..} if input.test(stack))
}
fn banner_patterns(stack: &ItemStack) -> Result<Vec<ferraloom_protocol::component::BannerLayer>> {
    Ok(match stack::component(stack, Id::BANNER_PATTERNS)? {
        Some(C::BannerPatterns(patterns)) => patterns,
        _ => Vec::new(),
    })
}
fn dye_color(stack: &ItemStack) -> Result<DyeColor> {
    Ok(match stack::component(stack, Id::DYE)? {
        Some(C::Dye(color)) => color,
        _ => DyeColor::White,
    })
}
/// Port of DyeColor#getFireworkColor, in the enum's declared order.
fn firework_color(color: DyeColor) -> i32 {
    [
        15790320, 15435844, 12801229, 6719955, 14602026, 4312372, 14188952, 4408131, 11250603,
        2651799, 8073150, 2437522, 5320730, 3887386, 11743532, 1973019,
    ][color as usize]
}
/// Port of DyedItemColor#applyDyes. Java float evaluation/truncation is intentional.
fn mix_dyes(current: Option<i32>, dyes: &[DyeColor]) -> i32 {
    let mut total = [0; 3];
    let mut intensity = 0;
    let mut count = 0;
    for rgb in current.into_iter().chain(dyes.iter().map(|dye| {
        [
            16383998, 16351261, 13061821, 3847130, 16701501, 8439583, 15961002, 4673362, 10329495,
            1481884, 8991416, 3949738, 8606770, 6192150, 11546150, 1908001,
        ][*dye as usize]
    })) {
        let channels = [(rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255];
        intensity += channels.into_iter().max().unwrap();
        for i in 0..3 {
            total[i] += channels[i];
        }
        count += 1;
    }
    let average = total.map(|channel| channel / count);
    let average_intensity = intensity as f32 / count as f32;
    let peak = *average.iter().max().unwrap() as f32;
    let [r, g, b] = average.map(|channel| (channel as f32 * average_intensity / peak) as i32);
    (r << 16) | (g << 8) | b
}
fn repair_pair(input: &dyn CraftingInputAccess) -> Result<Option<(&ItemStack, &ItemStack)>> {
    if input.ingredient_count() != 2 {
        return Ok(None);
    }
    let mut items = input.iter().filter(|s| !s.is_empty());
    let first = items.next().unwrap();
    let second = items.next().unwrap();
    if stack::item(first) != stack::item(second)
        || first.0.as_ref().unwrap().count != 1
        || second.0.as_ref().unwrap().count != 1
    {
        return Ok(None);
    }
    for source in [first, second] {
        if stack::component(source, Id::MAX_DAMAGE)?.is_none()
            || stack::component(source, Id::DAMAGE)?.is_none()
        {
            return Ok(None);
        }
    }
    Ok(Some((first, second)))
}
fn integer_component(stack: &ItemStack, id: Id) -> Result<i32> {
    Ok(match stack::component(stack, id)? {
        Some(C::MaxDamage(v) | C::Damage(v)) => v.0,
        _ => 0,
    })
}
fn assemble_repair(
    input: &dyn CraftingInputAccess,
    context: &impl CraftingContext,
) -> Result<ItemStack> {
    let Some((first, second)) = repair_pair(input)? else {
        return Ok(ItemStack::default());
    };
    let max1 = integer_component(first, Id::MAX_DAMAGE)?;
    let max2 = integer_component(second, Id::MAX_DAMAGE)?;
    let durability = max1.max(max2);
    let damage1 = integer_component(first, Id::DAMAGE)?.clamp(0, max1.max(0));
    let damage2 = integer_component(second, Id::DAMAGE)?.clamp(0, max2.max(0));
    let remaining = max1
        .wrapping_sub(damage1)
        .wrapping_add(max2.wrapping_sub(damage2))
        .wrapping_add(durability.wrapping_mul(5) / 100);
    let mut result = ItemStack::new(stack::item(first), 1, Default::default());
    stack::set(&mut result, C::MaxDamage(VarInt(durability)));
    stack::set(
        &mut result,
        C::Damage(VarInt(
            durability.wrapping_sub(remaining).max(0).min(durability),
        )),
    );
    let mut curses: Vec<(ferraloom_protocol::component::Enchantment, VarInt)> = Vec::new();
    for source in [first, second] {
        let id = if stack::item(source) == ferraloom_data::Item::ENCHANTED_BOOK {
            Id::STORED_ENCHANTMENTS
        } else {
            Id::ENCHANTMENTS
        };
        if let Some(C::Enchantments(values) | C::StoredEnchantments(values)) =
            stack::component(source, id)?
        {
            for (enchantment, level) in values.0 {
                if context.is_curse(&enchantment.0)? {
                    if let Some((_, previous)) =
                        curses.iter_mut().find(|(key, _)| key == &enchantment)
                    {
                        *previous = VarInt(previous.0.max(level.0));
                    } else {
                        curses.push((enchantment, level));
                    }
                }
            }
        }
    }
    let id = if stack::item(&result) == ferraloom_data::Item::ENCHANTED_BOOK {
        Id::STORED_ENCHANTMENTS
    } else {
        Id::ENCHANTMENTS
    };
    if stack::component(&result, id)?.is_some() {
        let curses = ItemEnchantments(curses);
        stack::set(
            &mut result,
            if id == Id::STORED_ENCHANTMENTS {
                C::StoredEnchantments(curses)
            } else {
                C::Enchantments(curses)
            },
        );
    }
    Ok(result)
}
