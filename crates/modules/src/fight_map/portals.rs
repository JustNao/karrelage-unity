//! Marks on the ground of a fight and the Eliotrope's portals among them.
//!
//! GameActionFightEvent mark_cells of action 1181 (« Pose un portail ») = a portal: FightMark {author_id, cells: [MarkedCell]}
//! whose other ints are unnamed: the cell = the MarkedCell's int that the mark repeats (its impact cell), the mark id = the int
//! field that unmark_cells / activate_glyph_trap `mark_id` point at (learnt: a mark id found in a single field of a single mark
//! votes for it, every mark is followed for that). Active = any of its bools true (one is unnamed). Four per caster, the oldest
//! goes on a fifth. A portal is closed while a fighter stands on it or once deactivated in the current turn. Verified on
//! work/capture_poison.jsonl (mark ids 1 to 4, the casts' `portals_id` order).
use super::{field, named_child, named_int};
use crate::Ctx;
use loutil_data::maps::{cell_at, coords, distance, portal_path, MAP_CELLS};
use loutil_proto::util::{messages, scalar_ints};
use loutil_proto::{DynamicMessage, Kind, PbValue, ReflectMessage};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// GameActionFightEvent action (= effect id) of a mark that is an Eliotrope portal: « Pose un portail ».
const PORTAL_ACTION: i64 = 1181;
/// Portals an Eliotrope can have: the oldest goes when a fifth is put down.
const MAX_PORTALS: usize = 4;

/// A mark on the ground (glyph, trap, portal...).
struct Mark {
    /// FightMark's unnamed ints (field, value): the mark id among them.
    ints: Vec<(String, i64)>,
    cell: i64,
    author: i64,
    portal: bool,
    active: bool,
    /// Deactivated during this turn: closed until the next one.
    off_turn: Option<u32>,
}

#[derive(Serialize)]
pub(super) struct Portal {
    pub cell: i64,
    pub author: i64,
    pub open: bool,
}

#[derive(Default)]
pub(super) struct Marks {
    /// In the order they were put down.
    marks: Vec<Mark>,
    /// Votes for the FightMark field holding the mark id (kept for the session).
    id_votes: HashMap<String, u32>,
}

impl Marks {
    /// The fight is over or starts: none on the ground.
    pub fn clear(&mut self) {
        self.marks.clear();
    }

    /// Portals in the order they were put down (`taken`: the cells fighters stand on, `turn`: the current one).
    pub fn portals(&self, taken: &HashSet<i64>, turn: u32) -> Vec<Portal> {
        let open = |m: &Mark| !taken.contains(&m.cell) && m.off_turn != Some(turn);
        self.marks.iter().filter(|m| m.portal).map(|m| Portal { cell: m.cell, author: m.author, open: open(m) }).collect()
    }

    /// Whether a portal was deactivated during that turn.
    pub fn off_in(&self, turn: u32) -> bool {
        self.marks.iter().any(|m| m.portal && m.off_turn == Some(turn))
    }

    /// Applies a GameActionFightEvent if it is about a mark: whether the portals changed; None when it is not.
    pub fn action(&mut self, ctx: &Ctx, d: &DynamicMessage, turn: u32) -> Option<bool> {
        if let Some(x) = named_child(ctx, d, "mark_cells") {
            return Some(named_child(ctx, x, "mark").is_some_and(|m| self.put_down(ctx, m, named_int(ctx, d, "action_id") == Some(PORTAL_ACTION))));
        }
        if let Some(x) = named_child(ctx, d, "unmark_cells") {
            let i = named_int(ctx, x, "mark_id").and_then(|id| self.find(id));
            return Some(i.is_some_and(|i| self.marks.remove(i).portal));
        }
        if let Some(x) = named_child(ctx, d, "activate_glyph_trap") {
            let Some(i) = named_int(ctx, x, "mark_id").and_then(|id| self.find(id)) else { return Some(false) };
            let active = any_true(x);
            let m = &mut self.marks[i];
            m.off_turn = if m.active && !active {
                Some(turn)
            } else if active {
                None
            } else {
                m.off_turn
            };
            m.active = active;
            return Some(m.portal);
        }
        None
    }

    /// A new mark from its FightMark: whether it is a portal (no cell found: not followed).
    fn put_down(&mut self, ctx: &Ctx, mark: &DynamicMessage, portal: bool) -> bool {
        let author = named_int(ctx, mark, "author_id").unwrap_or(0);
        let author_field = field(ctx, mark, "author_id");
        let ints: Vec<(String, i64)> = set_ints(mark).into_iter().filter(|(f, _)| Some(f) != author_field.as_ref()).collect();
        // its cell: the MarkedCell's int (a cell) that the mark repeats as its impact cell
        let first = field(ctx, mark, "cells").and_then(|f| messages(mark, &f).into_iter().next());
        let cells: Vec<i64> =
            first.map(|c| set_ints(&c).into_iter().map(|(_, v)| v).filter(|v| (0..MAP_CELLS as i64).contains(v)).collect()).unwrap_or_default();
        let Some(cell) = cells.iter().copied().find(|c| ints.iter().any(|(_, v)| v == c)).or(cells.first().copied()) else { return false };
        if portal {
            let theirs: Vec<usize> = (0..self.marks.len()).filter(|&i| self.marks[i].portal && self.marks[i].author == author).collect();
            if theirs.len() >= MAX_PORTALS {
                self.marks.remove(theirs[0]);
            }
        }
        self.marks.push(Mark { ints, cell, author, portal, active: any_true(mark), off_turn: None });
        portal
    }

    /// The mark an unmark / activation points at: by the learnt id field, else the only mark holding that int.
    fn find(&mut self, id: i64) -> Option<usize> {
        let holds = |m: &Mark, f: &str| m.ints.iter().any(|(g, v)| g == f && *v == id);
        // a value in a single field of a single mark: that field holds the ids
        let fields: HashSet<&String> = self.marks.iter().flat_map(|m| m.ints.iter().filter(|(_, v)| *v == id).map(|(f, _)| f)).collect();
        let unique: Vec<&String> = fields.into_iter().filter(|f| self.marks.iter().filter(|m| holds(m, f)).count() == 1).collect();
        if let [f] = unique[..] {
            *self.id_votes.entry(f.clone()).or_default() += 1;
        }
        match self.id_votes.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))) {
            Some((f, _)) => self.marks.iter().position(|m| holds(m, f)),
            None => {
                let holding: Vec<usize> = (0..self.marks.len()).filter(|&i| self.marks[i].ints.iter().any(|(_, v)| *v == id)).collect();
                (holding.len() == 1).then(|| holding[0])
            }
        }
    }
}

/// The ints of `m` that are set (unset ones read 0).
fn set_ints(m: &DynamicMessage) -> Vec<(String, i64)> {
    scalar_ints(m).into_iter().filter(|(f, _)| m.has_field_by_name(f)).collect()
}

/// Whether any bool field of `m` is true.
fn any_true(m: &DynamicMessage) -> bool {
    m.descriptor().fields().any(|f| matches!(f.kind(), Kind::Bool) && !f.is_list() && matches!(m.get_field(&f).as_ref(), PbValue::Bool(true)))
}

/// Where a spell cast from `caster` into the portal `entrance` lands: it goes through the network (`open` portals) and comes
/// out of the exit with the offset it went in with: exit + (entrance - caster). Verified on work/capture_poison.jsonl (five
/// casts of different offsets, a Portail among them). None when the network leads nowhere or the cell is off the map.
fn redirect(open: &[usize], entrance: usize, caster: usize) -> Option<(Vec<usize>, usize)> {
    let path = portal_path(open, entrance);
    let exit = *path.last().filter(|_| path.len() > 1)?;
    let ((ex, ey), (nx, ny), (cx, cy)) = (coords(exit), coords(entrance), coords(caster));
    Some((path, cell_at(ex + nx - cx, ey + ny - cy)?))
}

/// The Portail spell simulated on a cell.
#[derive(Serialize)]
pub(super) struct Sim {
    /// The new portal.
    added: Option<i64>,
    /// The oldest portal, gone because of the new one (there were four).
    dropped: Option<i64>,
    /// Cast into one of the portals: the new portal comes out past the network's exit, if that cell is free.
    via: bool,
    /// The new network's way from the new portal (what a spell cast into it would take), or from the chosen start.
    path: Vec<usize>,
    /// Cells between the path's portals.
    distance: i32,
}

/// The Portail spell cast on `add` in a caster's network (`net`: (cell, open) from the oldest): on a `free` cell, a new portal
/// there; on one of the open portals, the spell goes through the network and the new portal lands past the exit (`redirect`,
/// needs the `caster`'s cell). `start`: the portal the way shown starts from instead of the new one (none when that one is gone
/// or closed; with no new portal, the way from it as the network is).
pub(super) fn simulate(net: &[(i64, bool)], add: Option<i64>, free: &dyn Fn(i64) -> bool, caster: Option<i64>, start: Option<i64>) -> Sim {
    let open_cells = |net: &[(i64, bool)]| net.iter().filter(|(_, o)| *o).map(|(c, _)| *c as usize).collect::<Vec<_>>();
    let mut net = net.to_vec();
    let open = open_cells(&net);
    let into = add.filter(|c| open.contains(&(*c as usize)));
    let landing = match (into, caster) {
        (Some(e), Some(from)) => redirect(&open, e as usize, from as usize).map(|(_, at)| at as i64),
        (Some(_), None) => None,
        _ => add,
    };
    let via = into.is_some() && landing.is_some();
    let (mut added, mut dropped) = (None, None);
    if let Some(c) = landing.filter(|c| free(*c) && !net.iter().any(|(p, _)| p == c)) {
        if net.len() >= MAX_PORTALS {
            dropped = Some(net.remove(0).0);
        }
        net.push((c, true));
        added = Some(c);
    }
    let entrance = match start {
        Some(s) => net.iter().any(|(p, o)| *p == s && *o).then_some(s),
        None => added,
    };
    let path = entrance.map(|e| portal_path(&open_cells(&net), e as usize)).unwrap_or_default();
    let distance = path.windows(2).map(|w| distance(w[0], w[1])).sum();
    Sim { added, dropped, via, path, distance }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The casts of work/capture_poison.jsonl: out of the exit with the offset they went in with.
    #[test]
    fn spells_come_out_past_the_exit() {
        assert_eq!(redirect(&[188, 332, 265], 332, 264), Some((vec![332, 265, 188], 257)), "the Portail cast");
        assert_eq!(redirect(&[188, 332, 265], 332, 333).map(|r| r.1), Some(187));
        assert_eq!(redirect(&[188, 332, 265], 332, 292).map(|r| r.1), Some(229));
        assert_eq!(redirect(&[188, 332, 265, 257], 332, 264), Some((vec![332, 265, 188, 257], 325)), "the game's way from portal 2");
        assert_eq!(redirect(&[188], 188, 264), None, "a lone portal leads nowhere");
        // the oldest goes when the redirected portal is a fifth, and the way shown is the new network's
        let free = |c: i64| c != 325;
        let n = simulate(&[(188, true), (332, true), (265, true), (257, true)], Some(188), &free, Some(160), None);
        let (_, at) = redirect(&[188, 332, 265, 257], 188, 160).expect("a landing cell");
        assert_eq!(n.added, Some(at as i64));
        assert_eq!(n.path, portal_path(&[332, 265, 257, at], at), "the new network's way from the new portal");
        assert!(n.via && n.dropped == Some(188));
        // from a chosen portal: the new network's way from it, none once it is the one that goes
        let net = [(188, true), (332, true), (265, true)];
        let n = simulate(&net, Some(257), &free, None, Some(332));
        assert_eq!(n.path, [332, 265, 188, 257], "the capture's way from portal 2 once the 4th is down");
        assert_eq!(simulate(&net, None, &free, None, Some(332)).path, [332, 265, 188], "no new portal: the way as it is");
        let four = [(188, true), (332, true), (265, true), (257, true)];
        assert!(simulate(&four, Some(100), &free, None, Some(188)).path.is_empty(), "the start goes with the fifth");
    }
}
