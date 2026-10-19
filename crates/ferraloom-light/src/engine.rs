use crate::long_set::LongSet;
use crate::queue::QueueEntry;
use crate::storage::Storage;
use crate::{
    ChunkSkyLightSources, DataLayer, LightLayer, LightSectionUpdate, LightWorld, SectionType,
    is_empty_shape, shape_occludes,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::core::{BlockPos, ChunkPos, Direction, SectionPos};
use std::collections::{HashMap, VecDeque};

#[derive(Debug)]
struct LayerEngine {
    storage: Storage,
    checks: LongSet,
    decreases: VecDeque<(BlockPos, QueueEntry)>,
    increases: VecDeque<(BlockPos, QueueEntry)>,
    sources: HashMap<i64, ChunkSkyLightSources>,
}
impl LayerEngine {
    fn new(layer: LightLayer) -> Self {
        Self {
            storage: Storage::new(layer),
            checks: LongSet::new(512, 2),
            decreases: VecDeque::new(),
            increases: VecDeque::new(),
            sources: HashMap::new(),
        }
    }
    fn is_sky(&self) -> bool {
        self.storage.layer == LightLayer::Sky
    }
    fn emission(&self, pos: BlockPos, state: BlockState) -> u8 {
        if self.storage.enabled(ChunkPos::containing(pos)) {
            state.data().light_emission
        } else {
            0
        }
    }
    fn ensure_sources<W: LightWorld + ?Sized>(&mut self, world: &W, chunk: ChunkPos) {
        if world.has_chunk(chunk) {
            self.sources.entry(chunk.pack()).or_insert_with(|| {
                let mut sources = ChunkSkyLightSources::new(world.min_section() * 16);
                sources.fill_from(world, chunk);
                sources
            });
        }
    }
    fn lowest<W: LightWorld + ?Sized>(&mut self, world: &W, x: i32, z: i32, default: i32) -> i32 {
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        if !world.has_chunk(chunk) {
            return default;
        }
        if let Some(sources) = world.sky_light_sources(chunk) {
            return sources.lowest_source_y((x & 15) as u8, (z & 15) as u8);
        }
        self.ensure_sources(world, chunk);
        self.sources[&chunk.pack()].lowest_source_y((x & 15) as u8, (z & 15) as u8)
    }
    fn set_enabled<W: LightWorld + ?Sized>(&mut self, world: &W, chunk: ChunkPos, enable: bool) {
        self.storage.set_enabled(chunk, enable);
        if !enable || !self.is_sky() {
            return;
        }
        let highest = if let Some(sources) = world.sky_light_sources(chunk) {
            sources.highest_lowest_source_y()
        } else {
            self.ensure_sources(world, chunk);
            self.sources
                .get(&chunk.pack())
                .map_or(i32::MIN, ChunkSkyLightSources::highest_lowest_source_y)
        };
        let lowest_fully_source_section = (highest.wrapping_sub(1) >> 4) + 1;
        let bottom = self.storage.bottom.max(lowest_fully_source_section);
        for y in (bottom..self.storage.top(chunk)).rev() {
            if let Some(data) = self.storage.write(SectionPos::new(chunk.x, y, chunk.z))
                && data.is_empty()
            {
                data.fill(15);
            }
        }
    }
    fn has_work(&self) -> bool {
        self.storage.inconsistent
            || !self.checks.is_empty()
            || !self.decreases.is_empty()
            || !self.increases.is_empty()
    }
    fn run<W: LightWorld + ?Sized>(&mut self, world: &W) -> usize {
        let nodes = self.checks.clear_and_trim(512);
        // ChunkAccess's canonical buffer already reflects checked writes and
        // deliberately excludes raw bulk writes. Only legacy live snapshots
        // without that buffer still maintain a section-derived cache here.
        if self.is_sky() {
            for node in &nodes {
                let pos = BlockPos::of(*node);
                let chunk = ChunkPos::containing(pos);
                if world.sky_light_sources(chunk).is_some() {
                    continue;
                }
                self.ensure_sources(world, chunk);
                if let Some(sources) = self.sources.get_mut(&chunk.pack()) {
                    sources.update(world, pos);
                }
            }
        }
        for node in nodes {
            self.check_node(world, BlockPos::of(node));
        }
        let mut count = 0;
        while let Some((pos, entry)) = self.decreases.pop_front() {
            self.decrease(world, pos, entry);
            count += 1;
        }
        while let Some((pos, entry)) = self.increases.pop_front() {
            let mut level = self.storage.stored(pos);
            if entry.is_from_emission() && level < entry.level() {
                self.storage.set(pos, entry.level());
                level = entry.level();
            }
            if level == entry.level() {
                self.increase(world, pos, entry, level);
            }
            count += 1;
        }
        self.storage.reconcile();
        count
    }
    fn check_node<W: LightWorld + ?Sized>(&mut self, world: &W, pos: BlockPos) {
        let section = SectionPos::from_block(pos);
        if self.is_sky() {
            let lowest = if self.storage.enabled(section.chunk()) {
                self.lowest(world, pos.x, pos.z, i32::MAX)
            } else {
                i32::MAX
            };
            if lowest != i32::MAX {
                self.update_column(world, pos.x, pos.z, lowest);
            }
            if !self.storage.stores(section) {
                return;
            }
            if pos.y >= lowest {
                self.decreases
                    .push_back((pos, QueueEntry::decrease_skip(15, Direction::Up)));
                self.increases
                    .push_back((pos, QueueEntry::increase_skip(15, false, Direction::Up)));
            } else {
                let old = self.storage.stored(pos);
                if old > 0 {
                    self.storage.set(pos, 0);
                    self.decreases.push_back((pos, QueueEntry::decrease(old)));
                } else {
                    self.decreases.push_back((pos, QueueEntry::decrease(1)));
                }
            }
        } else if self.storage.stores(section) {
            let state = world.block_state(pos);
            let emission = self.emission(pos, state);
            let old = self.storage.stored(pos);
            if emission < old {
                self.storage.set(pos, 0);
                self.decreases.push_back((pos, QueueEntry::decrease(old)));
            } else {
                self.decreases.push_back((pos, QueueEntry::decrease(1)));
            }
            if emission > 0 {
                self.increases
                    .push_back((pos, QueueEntry::emission(emission, is_empty_shape(state))));
            }
        }
    }
    fn increase<W: LightWorld + ?Sized>(
        &mut self,
        world: &W,
        from: BlockPos,
        entry: QueueEntry,
        level: u8,
    ) {
        let mut from_state = None;
        let empty_below = self.empty_sections_below(from);
        for direction in Direction::VALUES.into_iter().filter(|d| entry.allows(*d)) {
            let to = from.relative(direction, 1);
            if !self.storage.stores(SectionPos::from_block(to)) {
                continue;
            }
            let old = self.storage.stored(to);
            if level.saturating_sub(1) <= old {
                continue;
            }
            let state = world.block_state(to);
            let new = level.saturating_sub(state.data().opacity.max(1));
            if new <= old {
                continue;
            }
            let from_state = *from_state.get_or_insert_with(|| {
                if entry.empty() {
                    Block::AIR.default_state()
                } else {
                    world.block_state(from)
                }
            });
            if shape_occludes(from_state, state, direction) {
                continue;
            }
            self.storage.set(to, new);
            if new > 1 {
                self.increases.push_back((
                    to,
                    QueueEntry::increase_skip(new, is_empty_shape(state), direction.opposite()),
                ));
            }
            self.propagate_from_empty_sections(to, direction, new, true, empty_below);
        }
    }
    fn decrease<W: LightWorld + ?Sized>(&mut self, world: &W, from: BlockPos, entry: QueueEntry) {
        let empty_below = self.empty_sections_below(from);
        for direction in Direction::VALUES.into_iter().filter(|d| entry.allows(*d)) {
            let to = from.relative(direction, 1);
            if !self.storage.stores(SectionPos::from_block(to)) {
                continue;
            }
            let level = self.storage.stored(to);
            if level == 0 {
                continue;
            }
            if level < entry.level() {
                let state = if self.is_sky() {
                    Block::AIR.default_state()
                } else {
                    world.block_state(to)
                };
                let emission = if self.is_sky() {
                    0
                } else {
                    self.emission(to, state)
                };
                self.storage.set(to, 0);
                if emission < level {
                    self.decreases
                        .push_back((to, QueueEntry::decrease_skip(level, direction.opposite())));
                }
                if emission > 0 {
                    self.increases
                        .push_back((to, QueueEntry::emission(emission, is_empty_shape(state))));
                }
                self.propagate_from_empty_sections(to, direction, level, false, empty_below);
            } else {
                self.increases
                    .push_back((to, QueueEntry::only(level, direction.opposite())));
            }
        }
    }
    fn empty_sections_below(&self, pos: BlockPos) -> i32 {
        if !self.is_sky()
            || pos.y & 15 != 0
            || (pos.x & 15 != 0 && pos.x & 15 != 15 && pos.z & 15 != 0 && pos.z & 15 != 15)
        {
            return 0;
        }
        let section = SectionPos::from_block(pos);
        let mut count = 0;
        while section.y - count > self.storage.bottom
            && !self.storage.stores(section.offset(0, -count - 1, 0))
        {
            count += 1;
        }
        count
    }
    fn propagate_from_empty_sections(
        &mut self,
        to: BlockPos,
        direction: Direction,
        level: u8,
        increase: bool,
        count: i32,
    ) {
        if count == 0 {
            return;
        }
        let crossed = match direction {
            Direction::North => to.z & 15 == 15,
            Direction::South => to.z & 15 == 0,
            Direction::West => to.x & 15 == 15,
            Direction::East => to.x & 15 == 0,
            _ => false,
        };
        if !crossed {
            return;
        }
        let section = SectionPos::from_block(to);
        for y in (section.y - count..section.y).rev() {
            if !self
                .storage
                .stores(SectionPos::new(section.x, y, section.z))
            {
                continue;
            }
            for local_y in (0..16).rev() {
                let pos = BlockPos::new(to.x, y * 16 + local_y, to.z);
                self.storage.set(pos, if increase { level } else { 0 });
                if increase {
                    if level > 1 {
                        self.increases.push_back((
                            pos,
                            QueueEntry::increase_skip(level, true, direction.opposite()),
                        ));
                    }
                } else {
                    self.decreases
                        .push_back((pos, QueueEntry::decrease_skip(level, direction.opposite())));
                }
            }
        }
    }
    fn update_column<W: LightWorld + ?Sized>(&mut self, world: &W, x: i32, z: i32, lowest: i32) {
        let bottom_y = self.storage.bottom.wrapping_mul(16);
        if lowest > bottom_y {
            let start = lowest - 1;
            'remove: for section_y in (self.storage.bottom..=(start >> 4)).rev() {
                if !self
                    .storage
                    .stores(SectionPos::new(x >> 4, section_y, z >> 4))
                {
                    continue;
                }
                for y in (section_y * 16..=(section_y * 16 + 15).min(start)).rev() {
                    let pos = BlockPos::new(x, y, z);
                    if self.storage.stored(pos) != 15 {
                        break 'remove;
                    }
                    self.storage.set(pos, 0);
                    let entry = if y == lowest - 1 {
                        QueueEntry::decrease(15)
                    } else {
                        QueueEntry::decrease_skip(15, Direction::Up)
                    };
                    self.decreases.push_back((pos, entry));
                }
            }
        }
        let neighbor_lowest = [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
            .into_iter()
            .map(|(x, z)| self.lowest(world, x, z, i32::MIN))
            .max()
            .unwrap();
        let start = lowest.max(bottom_y);
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        'add: for section_y in (start >> 4)..self.storage.top(chunk) {
            if !self
                .storage
                .stores(SectionPos::new(chunk.x, section_y, chunk.z))
            {
                continue;
            }
            for y in (section_y * 16).max(start)..=section_y * 16 + 15 {
                let pos = BlockPos::new(x, y, z);
                if self.storage.stored(pos) == 15 {
                    break 'add;
                }
                self.storage.set(pos, 15);
                if y < neighbor_lowest || y == lowest {
                    self.increases
                        .push_back((pos, QueueEntry::increase_skip(15, false, Direction::Up)));
                }
            }
        }
    }
    fn propagate_sources<W: LightWorld + ?Sized>(&mut self, world: &W, chunk: ChunkPos) {
        self.storage.set_enabled(chunk, true);
        if !self.is_sky() {
            if !world.has_chunk(chunk) {
                return;
            }
            for section_y in world.min_section()..world.max_section() {
                if let Some(section) = world.section(SectionPos::new(chunk.x, section_y, chunk.z)) {
                    if section.has_only_air() {
                        continue;
                    }
                    for y in 0..16 {
                        for z in 0..16 {
                            for x in 0..16 {
                                let state = section.get_block_state(x, y, z);
                                if state.data().light_emission > 0 {
                                    let pos = chunk.block_at(
                                        i32::from(x),
                                        section_y * 16 + i32::from(y),
                                        i32::from(z),
                                    );
                                    self.increases.push_back((
                                        pos,
                                        QueueEntry::emission(
                                            state.data().light_emission,
                                            is_empty_shape(state),
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            return;
        }
        // Capture neighbor heights before borrowing/writing a layer.
        let mut heights = [[i32::MIN; 256]; 5];
        for (values, (cx, cz)) in heights.iter_mut().zip([
            (chunk.x, chunk.z),
            (chunk.x, chunk.z - 1),
            (chunk.x, chunk.z + 1),
            (chunk.x - 1, chunk.z),
            (chunk.x + 1, chunk.z),
        ]) {
            for z in 0..16 {
                for x in 0..16 {
                    values[(z * 16 + x) as usize] =
                        self.lowest(world, cx * 16 + x, cz * 16 + z, i32::MIN);
                }
            }
        }
        for section_y in (self.storage.bottom..self.storage.top(chunk)).rev() {
            let pos = SectionPos::new(chunk.x, section_y, chunk.z);
            let Some(data) = self.storage.write(pos) else {
                continue;
            };
            let min_y = section_y * 16;
            let max_y = min_y + 15;
            let mut below = false;
            for z in 0..16 {
                for x in 0..16 {
                    let i = z * 16 + x;
                    let lowest = heights[0][i];
                    if lowest > max_y {
                        continue;
                    }
                    let neighbors = [
                        if z == 0 {
                            heights[1][15 * 16 + x]
                        } else {
                            heights[0][i - 16]
                        },
                        if z == 15 {
                            heights[2][x]
                        } else {
                            heights[0][i + 16]
                        },
                        if x == 0 {
                            heights[3][z * 16 + 15]
                        } else {
                            heights[0][i - 1]
                        },
                        if x == 15 {
                            heights[4][z * 16]
                        } else {
                            heights[0][i + 1]
                        },
                    ];
                    let highest_neighbor = *neighbors.iter().max().unwrap();
                    for y in (min_y.max(lowest)..=max_y).rev() {
                        data.set(x as u8, (y & 15) as u8, z as u8, 15);
                        if y == lowest || y < highest_neighbor {
                            let block = chunk.block_at(x as i32, y, z as i32);
                            self.increases.push_back((
                                block,
                                QueueEntry::sky([
                                    y == lowest,
                                    y < neighbors[0],
                                    y < neighbors[1],
                                    y < neighbors[2],
                                    y < neighbors[3],
                                ]),
                            ));
                        }
                    }
                    if lowest < min_y {
                        below = true;
                    }
                }
            }
            if !below {
                break;
            }
        }
    }
}

/// Port of `LevelLightEngine`, with caller-owned immutable world access and
/// owned section results instead of callbacks into live level state.
#[derive(Debug)]
pub struct LevelLightEngine {
    block: Option<LayerEngine>,
    sky: Option<LayerEngine>,
}

/// Published results of one vanilla decrease/increase/reconcile batch.
#[derive(Debug, Default)]
pub struct LightUpdates {
    pub queue_entries: usize,
    pub sections: Vec<LightSectionUpdate>,
    /// Vanilla `onLightUpdate` notifications include neighboring sections whose
    /// packet masks may change even when their own array was not written.
    pub affected_sections: Vec<(LightLayer, SectionPos)>,
}
impl LevelLightEngine {
    pub fn new(has_block_light: bool, has_sky_light: bool) -> Self {
        Self {
            block: has_block_light.then(|| LayerEngine::new(LightLayer::Block)),
            sky: has_sky_light.then(|| LayerEngine::new(LightLayer::Sky)),
        }
    }
    fn layers_mut(&mut self) -> impl Iterator<Item = &mut LayerEngine> {
        self.block.iter_mut().chain(self.sky.iter_mut())
    }
    fn layer(&self, layer: LightLayer) -> Option<&LayerEngine> {
        match layer {
            LightLayer::Block => self.block.as_ref(),
            LightLayer::Sky => self.sky.as_ref(),
        }
    }
    fn layer_mut(&mut self, layer: LightLayer) -> Option<&mut LayerEngine> {
        match layer {
            LightLayer::Block => self.block.as_mut(),
            LightLayer::Sky => self.sky.as_mut(),
        }
    }
    pub fn check_block(&mut self, pos: BlockPos) {
        for layer in self.layers_mut() {
            layer.checks.insert(pos.as_long());
        }
    }
    pub fn update_section_status(&mut self, pos: SectionPos, empty: bool) {
        for layer in self.layers_mut() {
            layer.storage.update_status(pos, empty);
        }
    }
    pub fn set_light_enabled<W: LightWorld + ?Sized>(
        &mut self,
        world: &W,
        chunk: ChunkPos,
        enabled: bool,
    ) {
        for layer in self.layers_mut() {
            layer.set_enabled(world, chunk, enabled);
        }
    }
    pub fn propagate_light_sources<W: LightWorld + ?Sized>(&mut self, world: &W, chunk: ChunkPos) {
        for layer in self.layers_mut() {
            layer.propagate_sources(world, chunk);
        }
    }
    pub fn retain_data(&mut self, chunk: ChunkPos, retain: bool) {
        for layer in self.layers_mut() {
            layer.storage.retain(chunk, retain);
        }
    }
    pub fn queue_section_data(
        &mut self,
        layer: LightLayer,
        pos: SectionPos,
        data: Option<DataLayer>,
    ) {
        if let Some(engine) = self.layer_mut(layer) {
            engine.storage.queue(pos, data);
        }
    }
    pub fn has_light_work(&self) -> bool {
        self.block
            .iter()
            .chain(self.sky.iter())
            .any(LayerEngine::has_work)
    }
    pub fn data_layer(
        &self,
        layer: LightLayer,
        pos: SectionPos,
    ) -> Option<&std::sync::Arc<DataLayer>> {
        self.layer(layer)?.storage.data(pos)
    }
    pub fn section_type(&self, layer: LightLayer, pos: SectionPos) -> SectionType {
        self.layer(layer)
            .map_or(SectionType::Empty, |e| e.storage.section_type(pos))
    }
    pub fn light_value(&self, layer: LightLayer, pos: BlockPos) -> u8 {
        self.layer(layer).map_or(0, |e| e.storage.value(pos))
    }
    pub fn raw_brightness(&self, pos: BlockPos, sky_dampen: u8) -> u8 {
        self.light_value(LightLayer::Block, pos).max(
            self.light_value(LightLayer::Sky, pos)
                .saturating_sub(sky_dampen),
        )
    }
    pub fn light_on_in_column(&self, chunk: ChunkPos) -> bool {
        self.block.as_ref().is_none_or(|b| {
            b.storage.enabled(chunk) && self.sky.as_ref().is_none_or(|s| s.storage.enabled(chunk))
        })
    }
    pub fn run_light_updates<W: LightWorld + ?Sized>(&mut self, world: &W) -> LightUpdates {
        let mut result = LightUpdates::default();
        for layer in self.layers_mut() {
            result.queue_entries += layer.run(world);
            layer
                .storage
                .publish(&mut result.sections, &mut result.affected_sections);
        }
        result
    }
    /// Port of `ThreadedLevelLightEngine#initializeLight`: establish nonempty
    /// section halos, apply queued disk data, then enable already-lighted chunks
    /// after the update pass. The scheduler owns the request/completion order.
    pub fn initialize_light<W: LightWorld + ?Sized>(
        &mut self,
        world: &W,
        chunk: ChunkPos,
        lighted: bool,
    ) -> LightUpdates {
        for y in world.min_section()..world.max_section() {
            let pos = SectionPos::new(chunk.x, y, chunk.z);
            if world.section(pos).is_some_and(|s| !s.has_only_air()) {
                self.update_section_status(pos, false);
            }
        }
        let result = self.run_light_updates(world);
        self.set_light_enabled(world, chunk, lighted);
        self.retain_data(chunk, false);
        result
    }
    /// Port of `ThreadedLevelLightEngine#lightChunk`. The caller marks its
    /// chunk light-correct only after integrating the returned updates.
    pub fn light_chunk<W: LightWorld + ?Sized>(
        &mut self,
        world: &W,
        chunk: ChunkPos,
        lighted: bool,
    ) -> LightUpdates {
        if !lighted {
            self.propagate_light_sources(world, chunk);
        }
        self.run_light_updates(world)
    }
    /// Invalidate source-height snapshots when replacing or unloading a chunk.
    pub fn forget_chunk_sources(&mut self, chunk: ChunkPos) {
        if let Some(sky) = &mut self.sky {
            sky.sources.remove(&chunk.pack());
        }
    }
}
