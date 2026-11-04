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

/// `cell` turned around `around` by quarter turns clockwise on screen (negative: counter clockwise), None off the map. A
/// clockwise quarter turn is (dx, dy) -> (dy, -dx): +x is down-right on screen, +y up-right.
pub fn turned(cell: usize, around: usize, quarters: i32) -> Option<usize> {
    let ((x, y), (cx, cy)) = (coords(cell), coords(around));
    let (dx, dy) = (x - cx, y - cy);
    let (tx, ty) = match quarters.rem_euclid(4) {
        0 => (dx, dy),
        1 => (dy, -dx),
        2 => (-dx, -dy),
        _ => (-dy, dx),
    };
    cell_at(cx + tx, cy + ty)
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

/// How `e` turns from `s` seen from `o`.
#[derive(PartialEq)]
enum Turn {
    Same,
    Opposite,
    Anticlockwise,
    Clockwise,
}

fn turn(o: (i32, i32), s: (i32, i32), e: (i32, i32)) -> Turn {
    let (a, b) = ((s.0 - o.0, s.1 - o.1), (e.0 - o.0, e.1 - o.1));
    match (a.0 * b.1 - a.1 * b.0).signum() {
        1 => Turn::Anticlockwise,
        -1 => Turn::Clockwise,
        _ if (a.0 >= 0) == (b.0 >= 0) && (a.1 >= 0) == (b.1 >= 0) => Turn::Same,
        _ => Turn::Opposite,
    }
}

/// The angle from `s` to `e` seen from `o`, anticlockwise, in 0..2π.
fn oriented_angle(o: (i32, i32), s: (i32, i32), e: (i32, i32)) -> f64 {
    let (a, b) = ((s.0 - o.0, s.1 - o.1), (e.0 - o.0, e.1 - o.1));
    let angle = || ((a.0 * b.1 - a.1 * b.0).abs() as f64).atan2((a.0 * b.0 + a.1 * b.1) as f64);
    match turn(o, s, e) {
        Turn::Same => 0.0,
        Turn::Opposite => std::f64::consts::PI,
        Turn::Anticlockwise => angle(),
        Turn::Clockwise => std::f64::consts::TAU - angle(),
    }
}

/// Eliotrope portals: the way through a network (`portals`: their cells, the open ones only) entered by `entrance` (one of
/// them, else nothing). From each portal to the nearest one not gone through yet (|dx| + |dy|); between equally near ones,
/// sorted by their angle from the (0, +1) direction anticlockwise, the one before the first clockwise turn of that order,
/// else the first (the game's rule, as Eliotopy-v2 reads it). Every portal is gone through: the last one is the exit.
/// Verified on a capture (the cast's `portals_id`: [2, 3, 1] then [2, 3, 1, 4]).
pub fn portal_path(portals: &[usize], entrance: usize) -> Vec<usize> {
    if !portals.contains(&entrance) {
        return vec![];
    }
    let mut left: Vec<usize> = portals.iter().copied().filter(|&p| p != entrance).collect();
    let mut path = vec![entrance];
    while !left.is_empty() {
        let o = coords(*path.last().unwrap());
        let near = left.iter().map(|&p| distance(path[path.len() - 1], p)).min().unwrap();
        let mut closest: Vec<usize> = (0..left.len()).filter(|&i| distance(path[path.len() - 1], left[i]) == near).collect();
        let pick = if closest.len() == 1 {
            closest[0]
        } else {
            let below = (o.0, o.1 + 1);
            closest.sort_by(|&i, &j| oriented_angle(o, below, coords(left[i])).total_cmp(&oriented_angle(o, below, coords(left[j]))).then(i.cmp(&j)));
            let n = closest.len();
            let mut prev = n - 1;
            let mut pick = closest[0];
            for i in 0..n {
                match turn(o, coords(left[closest[prev]]), coords(left[closest[i]])) {
                    Turn::Opposite if n > 2 => {
                        pick = closest[prev];
                        break;
                    }
                    Turn::Opposite => break,
                    Turn::Clockwise => {
                        pick = closest[prev];
                        break;
                    }
                    _ => prev = i,
                }
            }
            pick
        };
        path.push(left.remove(pick));
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The portals of work/capture_poison.jsonl (mark ids 1 to 4 on cells 188, 332, 265, 257): the game went 2, 3, 1 with
    /// three of them, 2, 3, 1, 4 with the fourth.
    #[test]
    fn portal_paths_follow_the_game() {
        assert_eq!(portal_path(&[188, 332, 265], 332), vec![332, 265, 188]);
        assert_eq!(portal_path(&[188, 332, 265, 257], 332), vec![332, 265, 188, 257]);
        assert_eq!(portal_path(&[188, 332], 188), vec![188, 332]);
        assert!(portal_path(&[188, 332], 5).is_empty(), "not entered by a portal");
        // equally near: a choice by angle, and always the same one
        let (o, a, b) = (cell_at(5, 1).unwrap(), cell_at(7, 1).unwrap(), cell_at(3, 1).unwrap());
        assert_eq!(portal_path(&[o, a, b], o).len(), 3);
        assert_eq!(portal_path(&[o, a, b], o), portal_path(&[o, b, a], o));
    }

    /// Spells cast under confusion on work/capture_harebourg.jsonl (cast request's cell -> where the server said it landed).
    #[test]
    fn spells_turn_as_in_the_game() {
        for (caster, clicked, confusion, landed) in
            [(225, 238, 1, 210), (225, 211, 1, 239), (225, 211, 2, 238), (225, 210, 2, 239), (159, 217, 1, 213), (267, 224, 1, 227), (240, 226, 1, 227)]
        {
            assert_eq!(turned(clicked, caster, confusion), Some(landed), "{clicked} around {caster} by {confusion}");
        }
        assert_eq!(turned(211, 225, -2), Some(238), "counter clockwise: the same half turn");
        assert_eq!(turned(238, 225, -3), Some(210), "three back = one forward");
        assert_eq!(turned(225, 225, 1), Some(225), "cast on oneself");
        assert_eq!(turned(211, 225, 0), Some(211));
        assert_eq!(turned(13, 0, 2), None, "off the map");
    }

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
