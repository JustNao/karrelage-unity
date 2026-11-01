//! Map cells (datapack/maps.bin, written by pipeline/extract_gamedata.py `maps`) and the grid geometry of a fight: cell
//! coordinates, distances and line of sight.
//!
//! A map is 560 cells in 40 staggered rows of 14 (odd rows shifted half a cell right). Cell `c` has the diagonal
//! coordinates of the game (x grows down-right, y up-right): `a = c / 28`, `r = c % 28`, `x = a + r / 14 + r % 14`,
//! `y = r % 14 - a`; the distance between two cells is |dx| + |dy|.
use crate::Data;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub const MAP_CELLS: usize = 560;
const ROW: i32 = 14;
const ROWS: i32 = 20;
/// Cell flags: walkable.
pub const MOV: u8 = 1;
/// Cell flags: lets the line of sight through.
pub const LOS: u8 = 2;
/// Cell flags: placement cell of the red / blue team.
pub const RED: u8 = 4;
pub const BLUE: u8 = 8;
const HEADER: u64 = 12;
const RECORD: u64 = 8 + MAP_CELLS as u64 / 2;

impl Data {
    /// The flags of every cell of a map (`MOV` / `LOS` / `RED` / `BLUE`), None when the pack has no such map (or no maps.bin).
    /// Reads the file (binary search over the records sorted by map id, a few small reads): not for the capture thread.
    pub fn map_cells(&self, id: i64) -> Option<Vec<u8>> {
        let mut f = File::open(self.dir.join("maps.bin")).ok()?;
        let mut head = [0u8; HEADER as usize];
        f.read_exact(&mut head).ok()?;
        if &head[..4] != b"KMAP" || u32::from_le_bytes(head[4..8].try_into().ok()?) != 1 {
            return None;
        }
        let (mut lo, mut hi) = (0u64, u32::from_le_bytes(head[8..12].try_into().ok()?) as u64);
        let mut key = [0u8; 8];
        while lo < hi {
            let mid = (lo + hi) / 2;
            f.seek(SeekFrom::Start(HEADER + mid * RECORD)).ok()?;
            f.read_exact(&mut key).ok()?;
            match i64::from_le_bytes(key).cmp(&id) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let mut packed = [0u8; MAP_CELLS / 2];
                    f.read_exact(&mut packed).ok()?;
                    return Some(packed.iter().flat_map(|b| [b & 0xF, b >> 4]).collect());
                }
            }
        }
        None
    }
}

/// Diagonal coordinates (x, y) of a cell.
pub fn coords(cell: usize) -> (i32, i32) {
    let c = cell as i32;
    let (a, r) = (c / (2 * ROW), c % (2 * ROW));
    (a + r / ROW + r % ROW, r % ROW - a)
}

/// The cell at diagonal coordinates (x, y), None outside the map.
pub fn cell_at(x: i32, y: i32) -> Option<usize> {
    let (a, odd) = ((x - y).div_euclid(2), (x - y).rem_euclid(2));
    let b = y + a;
    ((0..ROWS).contains(&a) && (0..ROW).contains(&b)).then_some((a * 2 * ROW + odd * ROW + b) as usize)
}

pub fn distance(a: usize, b: usize) -> i32 {
    let ((xa, ya), (xb, yb)) = (coords(a), coords(b));
    (xa - xb).abs() + (ya - yb).abs()
}

/// The points a line from the centre of `from` to the centre of `to` passes through, both ends left out. Exactly through a
/// corner = straight to the diagonal cell (the two cells beside the corner are not crossed).
fn between(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let (dx, dy) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
    let (sx, sy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    let (mut x, mut y, mut ix, mut iy) = (from.0, from.1, 0, 0);
    let mut out = vec![];
    while ix < dx || iy < dy {
        // which grid line comes first: the next x boundary at (2ix + 1) / 2dx, the next y one at (2iy + 1) / 2dy
        match ((1 + 2 * ix) * dy).cmp(&((1 + 2 * iy) * dx)) {
            std::cmp::Ordering::Equal => (x, y, ix, iy) = (x + sx, y + sy, ix + 1, iy + 1),
            std::cmp::Ordering::Less => (x, ix) = (x + sx, ix + 1),
            std::cmp::Ordering::Greater => (y, iy) = (y + sy, iy + 1),
        }
        if (x, y) != to {
            out.push((x, y));
        }
    }
    out
}

/// What a fighter standing on `from` reaches at distance `min..=max`: the walkable cells, split into those in line of sight
/// and those behind an obstacle. A line is blocked by a cell of the map that does not let sight through, or by an occupied
/// cell (fighters), the target's own cell excepted; points of the line outside the map block nothing.
pub fn sight(cells: &[u8], occupied: &HashSet<usize>, from: usize, min: i32, max: i32) -> (Vec<usize>, Vec<usize>) {
    let (mut seen, mut hidden) = (vec![], vec![]);
    if cells.len() != MAP_CELLS || from >= MAP_CELLS {
        return (seen, hidden);
    }
    let origin = coords(from);
    for to in 0..MAP_CELLS {
        let d = distance(from, to);
        if cells[to] & MOV == 0 || d < min || d > max {
            continue;
        }
        let clear = between(origin, coords(to)).into_iter().filter_map(|(x, y)| cell_at(x, y)).all(|c| cells[c] & LOS != 0 && !occupied.contains(&c));
        if clear {
            seen.push(to);
        } else {
            hidden.push(to);
        }
    }
    (seen, hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_round_trip() {
        assert_eq!(coords(0), (0, 0));
        assert_eq!(coords(1), (1, 1));
        assert_eq!(coords(14), (1, 0)); // first cell of the shifted row, between cells 0 and 1
        assert!((0..MAP_CELLS).all(|c| cell_at(coords(c).0, coords(c).1) == Some(c)));
        assert_eq!(distance(0, 14), 1);
        assert_eq!(distance(0, 1), 2);
        assert_eq!(distance(0, 28), 2); // two rows straight down
        assert_eq!(cell_at(-1, 0), None);
    }

    #[test]
    fn lines_go_through_corners_diagonally() {
        assert!(between((0, 0), (1, 0)).is_empty());
        assert!(between((0, 0), (1, 1)).is_empty(), "diagonal neighbour: through the corner");
        assert_eq!(between((0, 0), (3, 0)), vec![(1, 0), (2, 0)]);
        assert_eq!(between((0, 0), (3, 3)), vec![(1, 1), (2, 2)]);
        // a knight-like line crosses the two cells it straddles
        assert_eq!(between((0, 0), (2, 1)), vec![(1, 0), (1, 1)]);
        // symmetric
        let (a, b) = ((2, -3), (7, 1));
        let mut back = between(b, a);
        back.reverse();
        assert_eq!(between(a, b), back);
    }

    #[test]
    fn walls_and_fighters_block_the_sight() {
        let mut cells = vec![MOV | LOS; MAP_CELLS];
        let from = cell_at(5, 0).unwrap();
        let wall = cell_at(7, 0).unwrap();
        cells[wall] = 0;
        let (seen, hidden) = sight(&cells, &HashSet::new(), from, 1, 4);
        assert!(hidden.contains(&cell_at(8, 0).unwrap()) && hidden.contains(&cell_at(9, 0).unwrap()));
        assert!(seen.contains(&cell_at(6, 0).unwrap()) && !seen.contains(&wall) && !hidden.contains(&wall), "a wall is no target");
        // a fighter blocks what is behind it, not its own cell
        let fighter = cell_at(5, 2).unwrap();
        let (seen, hidden) = sight(&cells, &HashSet::from([fighter]), from, 1, 4);
        assert!(seen.contains(&fighter) && hidden.contains(&cell_at(5, 3).unwrap()));
        // a hole (not walkable, sight goes through) is no target and blocks nothing
        cells[fighter] = LOS;
        let (seen, hidden) = sight(&cells, &HashSet::new(), from, 1, 4);
        assert!(!seen.contains(&fighter) && !hidden.contains(&fighter) && seen.contains(&cell_at(5, 3).unwrap()));
    }
}
