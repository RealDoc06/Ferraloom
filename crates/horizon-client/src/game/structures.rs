//! Which structures contain a position (`StructureManager.getStructureAt`
//! and `getStructureWithPieceAt`), from the chunk's structure references and
//! the generator's cached starts.
use super::Game;
use horizon_client::chunk::Chunk;
use horizon_world::worldgen::structure::start::StructureStart;
use std::sync::Arc;

impl Game<'_> {
    /// Structures reaching `pos`: (id, inside a piece). `getStructureAt`
    /// tests the start's whole box, `getStructureWithPieceAt` a piece's.
    pub(super) fn structures_reaching(&self, pos: [i32; 3]) -> Vec<(String, bool)> {
        self.structures_reaching_where(pos, &|_| true)
    }
    /// The same, for the structures whose id `wanted` takes: the others'
    /// starts aren't looked up.
    pub(super) fn structures_reaching_where(
        &self,
        pos: [i32; 3],
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<(String, bool)> {
        let Some(chunk) = self.world.chunks.get(&(pos[0] >> 4, pos[2] >> 4)) else {
            return Vec::new();
        };
        if chunk.structure_refs.is_empty() {
            return Vec::new();
        }
        let Some(locator) = self.end.locator() else {
            return Vec::new();
        };
        structures_reaching_in(chunk, pos, wanted, |x, z| locator.starts_at(x, z))
    }
    /// The structures with a piece containing `pos`.
    pub(super) fn structures_at(&self, pos: [i32; 3]) -> Vec<String> {
        self.structures_reaching(pos)
            .into_iter()
            .filter(|(_, piece)| *piece)
            .map(|(n, _)| n)
            .collect()
    }
}
/// [`Game::structures_reaching_where`] in `chunk`, the column holding `pos`,
/// with `starts_at` giving the starts owned by a chunk (the generator's).
pub(super) fn structures_reaching_in<S: AsRef<[Arc<StructureStart>]>>(
    chunk: &Chunk,
    pos: [i32; 3],
    wanted: &dyn Fn(&str) -> bool,
    mut starts_at: impl FnMut(i32, i32) -> S,
) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    for (id, (sx, sz)) in &chunk.structure_refs {
        if !wanted(id) {
            continue;
        }
        for start in starts_at(*sx, *sz).as_ref() {
            if *start.name != **id || !start.bounding_box.is_inside(pos) {
                continue;
            }
            let piece = start.pieces.iter().any(|p| p.bounding_box().is_inside(pos));
            match out.iter_mut().find(|(n, _)| *n == start.name) {
                Some(e) => e.1 |= piece,
                None => out.push((start.name.clone(), piece)),
            }
        }
    }
    out
}
