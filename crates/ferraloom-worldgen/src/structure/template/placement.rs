use super::*;
use crate::structure::{processor::StructureProcessor, rule::BlockTags};
use ferraloom_nbt::ListTag;
use ferraloom_util::Direction;
use std::collections::BTreeSet;

impl StructureTemplate {
    /// Port of StructureTemplate#processBlockInfos. Capped processors disable
    /// early chunk clipping and finalize after every per-block processor.
    #[allow(clippy::too_many_arguments)]
    pub fn process_block_infos(
        &self,
        region: &impl crate::feature::LiveWorldGenLevel,
        position: [i32; 3],
        reference: [i32; 3],
        settings: &mut StructurePlaceSettings,
        processors: &[StructureProcessor],
        tags: &BlockTags,
        blocks: &[StructureBlockInfo],
        random: &mut impl crate::feature::FeatureRandomSource,
    ) -> Result<Vec<StructureBlockInfo>, Error> {
        let mut access =
            crate::feature::FeaturePlacementAccess::read(region, random.feature_random());
        self.process_block_infos_with_access(
            &mut access,
            position,
            reference,
            settings,
            processors,
            tags,
            blocks,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn process_block_infos_with_access(
        &self,
        access: &mut crate::feature::FeaturePlacementAccess<
            '_,
            impl crate::feature::LiveWorldGenLevel,
        >,
        position: [i32; 3],
        reference: [i32; 3],
        settings: &mut StructurePlaceSettings,
        processors: &[StructureProcessor],
        tags: &BlockTags,
        blocks: &[StructureBlockInfo],
    ) -> Result<Vec<StructureBlockInfo>, Error> {
        let process_only_in_chunk = !processors
            .iter()
            .any(StructureProcessor::evaluates_entire_piece_state);
        let mut original = Vec::new();
        let mut processed = Vec::new();
        for block in blocks {
            let p = offset(
                transform_block(
                    block.position,
                    settings.mirror,
                    settings.rotation,
                    settings.pivot,
                ),
                position,
            );
            if process_only_in_chunk && settings.bounding_box.is_some_and(|b| !b.contains(p)) {
                continue;
            }
            let mut value = Some(StructureBlockInfo {
                position: p,
                state: block.state,
                nbt: block.nbt.clone(),
            });
            for processor in processors {
                let Some(info) = value else {
                    break;
                };
                value = processor.process_block_with_access(
                    access,
                    position,
                    reference,
                    block.position,
                    info,
                    settings,
                    tags,
                )?;
            }
            if let Some(info) = value {
                original.push(block);
                processed.push(info);
            }
        }
        for processor in processors {
            processor.finalize_processing_with_access(
                access,
                position,
                reference,
                &original,
                &mut processed,
                settings,
                tags,
            )?;
        }
        Ok(processed)
    }
    /// Port of StructureTemplate#placeInWorld. Calls into game behavior stay
    /// synchronous: an unavailable liquid, shape, block-entity or entity
    /// implementation is an error rather than silently omitting that effect.
    #[allow(clippy::too_many_arguments)]
    pub fn place_in_world(
        &self,
        region: &mut impl crate::feature::LiveWorldGenLevel,
        position: [i32; 3],
        reference: [i32; 3],
        settings: &mut StructurePlaceSettings,
        processors: &[StructureProcessor],
        tags: &BlockTags,
        random: &mut impl crate::feature::FeatureRandomSource,
        flags: u32,
    ) -> Result<bool, Error> {
        let mut access = crate::feature::FeaturePlacementAccess::new(
            region,
            crate::feature::FeatureRandomSelector::External(random.feature_random()),
        );
        self.place_with_access(
            &mut access,
            position,
            reference,
            settings,
            processors,
            tags,
            flags,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn place_with_access(
        &self,
        access: &mut crate::feature::FeaturePlacementAccess<
            '_,
            impl crate::feature::LiveWorldGenLevel,
        >,
        position: [i32; 3],
        reference: [i32; 3],
        settings: &mut StructurePlaceSettings,
        processors: &[StructureProcessor],
        tags: &BlockTags,
        flags: u32,
    ) -> Result<bool, Error> {
        if self.palettes.is_empty() {
            return Ok(false);
        }
        let blocks = &self
            .random_palette(settings, position, &mut access.random()?)?
            .blocks;
        if (blocks.is_empty() && (settings.ignore_entities || self.entities.is_empty()))
            || self.size.iter().any(|&v| v < 1)
        {
            return Ok(false);
        }
        let apply_waterlogging = settings.liquid_settings == LiquidSettings::ApplyWaterlogging;
        let processed = self.process_block_infos_with_access(
            access, position, reference, settings, processors, tags, blocks,
        )?;
        let mut to_fill = Vec::new();
        let mut locked_fluids = Vec::new();
        let mut placed = Vec::new();
        let mut bounds: Option<BoundingBox> = None;
        for block in processed {
            let p = block.position;
            if settings.bounding_box.is_some_and(|b| !b.contains(p)) {
                continue;
            }
            let previous = if apply_waterlogging {
                Some(access.level().fluid(p)?)
            } else {
                None
            };
            let state = block
                .state
                .mirror(settings.mirror)
                .rotate(settings.rotation);
            if block.nbt.is_some() {
                access
                    .level_mut()?
                    .set_block(p, Block::BARRIER.default_state(), 820)?;
            }
            if access.level_mut()?.set_block(p, state, flags)? {
                let point = BoundingBox::point(p);
                bounds = Some(bounds.map_or(point, |b| b.encapsulating(point)));
                if let Some(tag) = block.nbt.clone()
                    && access.level().has_block_entity_at(p)?
                {
                    {
                        let (level, random) = access.parts_mut()?;
                        level.load_block_entity(p, tag, random)?;
                    }
                }
                placed.push((p, block.nbt.is_some()));
                if let Some(previous) = previous {
                    if state.data().fluid.source {
                        locked_fluids.push(p);
                    } else if state.get(ferraloom_data::properties::WATERLOGGED).is_some() {
                        access.level_mut()?.place_liquid(p, previous)?;
                        if !previous.source {
                            to_fill.push(p);
                        }
                    }
                }
            }
        }
        let directions = [
            Direction::Up,
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ];
        let mut filled = true;
        while filled && !to_fill.is_empty() {
            filled = false;
            let mut index = 0;
            while index < to_fill.len() {
                let p = to_fill[index];
                let mut fluid = access.level().fluid(p)?;
                for direction in directions {
                    if fluid.source {
                        break;
                    }
                    let neighbor = relative(p, direction);
                    let other = access.level().fluid(neighbor)?;
                    if other.source && !locked_fluids.contains(&neighbor) {
                        fluid = other;
                    }
                }
                if fluid.source
                    && access
                        .level()
                        .block(p)?
                        .get(ferraloom_data::properties::WATERLOGGED)
                        .is_some()
                {
                    access.level_mut()?.place_liquid(p, fluid)?;
                    filled = true;
                    to_fill.remove(index);
                } else {
                    index += 1;
                }
            }
        }
        if let Some(bounds) = bounds {
            if !settings.known_shape {
                update_shape_at_edge(
                    access.level_mut()?,
                    flags,
                    &placed.iter().map(|&(p, _)| p).collect(),
                    bounds,
                )?;
            }
            for (p, has_nbt) in placed {
                if !settings.known_shape {
                    let state = access.level().block(p)?;
                    let updated = access.level_mut()?.update_from_neighbor_shapes(p, state)?;
                    if state != updated {
                        access
                            .level_mut()?
                            .set_block(p, updated, (flags & !1) | 16)?;
                    }
                    access
                        .level_mut()?
                        .update_neighbors_at(p, updated.block())?;
                }
                if has_nbt {
                    // Protochunks already retain their pending NBT for saving;
                    // live receivers need this native late setChanged callback.
                    access.level_mut()?.mark_template_block_entity_changed(p)?;
                }
            }
        }
        if !settings.ignore_entities {
            self.place_entities(access.level_mut()?, position, settings)?;
        }
        Ok(true)
    }
    fn place_entities(
        &self,
        region: &mut impl crate::feature::LiveWorldGenLevel,
        position: [i32; 3],
        settings: &StructurePlaceSettings,
    ) -> Result<(), Error> {
        for entity in &self.entities {
            let block_pos = offset(
                transform_block(
                    entity.block_position,
                    settings.mirror,
                    settings.rotation,
                    settings.pivot,
                ),
                position,
            );
            if settings
                .bounding_box
                .is_some_and(|b| !b.contains(block_pos))
            {
                continue;
            }
            let relative = transform_entity(
                entity.position,
                settings.mirror,
                settings.rotation,
                settings.pivot,
            );
            let pos = std::array::from_fn::<_, 3, _>(|i| relative[i] + f64::from(position[i]));
            let mut tag = entity.nbt.clone();
            tag.insert("Pos", Tag::List(ListTag(pos.map(Tag::Double).to_vec())));
            tag.remove("UUID");
            if tag.contains_key("block_pos") {
                tag.insert("block_pos", Tag::IntArray(block_pos.to_vec()));
            }
            region.place_template_entity(
                [(pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4],
                tag,
                settings.mirror,
                settings.rotation,
                settings.finalize_entities,
            )?;
        }
        Ok(())
    }
}
pub(crate) fn offset(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| a[i].wrapping_add(b[i]))
}
pub(crate) fn relative(p: [i32; 3], direction: Direction) -> [i32; 3] {
    let d = direction.step();
    offset(p, [d.x, d.y, d.z])
}
/// Port of StructureTemplate#updateShapeAtEdge and
/// DiscreteVoxelShape#forAllFaces. Axis runs preserve callback/RNG order.
fn update_shape_at_edge(
    region: &mut impl crate::feature::LiveWorldGenLevel,
    flags: u32,
    filled: &BTreeSet<[i32; 3]>,
    bounds: BoundingBox,
) -> Result<(), Error> {
    for ([a, b, c], negative, positive) in [
        ([0, 1, 2], Direction::North, Direction::South),
        ([2, 0, 1], Direction::Down, Direction::Up),
        ([1, 2, 0], Direction::West, Direction::East),
    ] {
        for av in bounds.min[a]..=bounds.max[a] {
            for bv in bounds.min[b]..=bounds.max[b] {
                let mut last = false;
                for cv in bounds.min[c]..=bounds.max[c] + 1 {
                    let mut p = [0; 3];
                    p[a] = av;
                    p[b] = bv;
                    p[c] = cv;
                    let full = cv <= bounds.max[c] && filled.contains(&p);
                    if !last && full {
                        update_edge_face(region, flags, p, negative)?;
                    }
                    if last && !full {
                        p[c] -= 1;
                        update_edge_face(region, flags, p, positive)?;
                    }
                    last = full;
                }
            }
        }
    }
    Ok(())
}
fn update_edge_face(
    region: &mut impl crate::feature::LiveWorldGenLevel,
    flags: u32,
    p: [i32; 3],
    direction: Direction,
) -> Result<(), Error> {
    let neighbor = relative(p, direction);
    let state = region.block(p)?;
    let other = region.block(neighbor)?;
    let new_state = region.update_shape(p, direction, neighbor, other)?;
    if state != new_state {
        region.set_block(p, new_state, flags & !1)?;
    }
    let new_neighbor = region.update_shape(neighbor, direction.opposite(), p, new_state)?;
    if other != new_neighbor {
        region.set_block(neighbor, new_neighbor, flags & !1)?;
    }
    Ok(())
}
