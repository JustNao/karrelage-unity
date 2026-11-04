//! Fight map (Combat tab): the cells of the map being fought on and where every fighter stands, for a minimap with line of
//! sight from a hovered cell.
//!
//! Map: FightMapInformationEvent's `map_id` during a fight (fights in dungeons happen on their own map), else the roleplay
//! map of MapComplementaryInformationEvent; its cells come from the pack (`Data::map_cells`, read in `call` only).
//! Fighters: every ActorPositionInformation {actor_id, disposition: EntityDisposition {cell_id}} of FightFighterShowEvent /
//! FightSynchronizeEvent (the whole list: anyone missing is gone) and of the summons of GameActionFightEvent, with their
//! FightFighterInformation's SpawnInformation {alive, team (its only enum)} (a summon's SpawnInformation holds its position).
//! Moves: MapMovementEvent (last cell of `cells`) and GameActionFightEvent slide (its two unnamed cells: the end is the one
//! that is not the target's cell), teleport_on_same_map, exchange_positions (caster and target swap), carry / throw / drop
//! character, death (its only int).
//! Marks on the ground and the Eliotrope's portals: `portals`.
//!
//! In-game overlay (FightMapOverlay.svelte, opened by a shortcut): `window` = the Dofus window's client area to cover,
//! `fightmap:turn` {id} at each turn start, `fightmap:escape` / `fightmap:click` when Escape / a mouse button is pressed while
//! `watch_escape` is on.
mod portals;

use crate::combat::children;
use crate::kit::{arg_i64, unknown, Role};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_data::maps::{sight, MAP_CELLS, MOV};
use loutil_proto::util::{as_i64, int, scalar_ints};
use loutil_proto::{DynamicMessage, Kind, PbValue, ReflectMessage};
use parking_lot::Mutex;
use portals::{simulate, Marks, Portal, Sim};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MAP: &str = "MapComplementaryInformationEvent";
const FIGHT_MAP: &str = "FightMapInformationEvent";
const JOIN: &str = "FightJoinEvent";
const SHOW: &str = "FightFighterShowEvent";
const REFRESH: &str = "FightFighterRefreshEvent";
const SYNC: &str = "FightSynchronizeEvent";
const ACTION: &str = "GameActionFightEvent";
const MOVE: &str = "MapMovementEvent";
const END: &str = "FightEndEvent";
/// A fighter's turn starts (role `turn`, shared with combat.rs): the overlay clears when the player's turn is over.
const TURN: &str = "FightTurnEvent";
const POSITION: &str = "ActorPositionInformation";
const FIGHTER: &str = "FightFighterInformation";
const SPAWN: &str = "SpawnInformation";
/// GameActionFightEvent summons by context (script-named).
const SUMMON: &str = "SummonContextInformation";

#[derive(Clone, Serialize)]
struct Fighter {
    id: i64,
    cell: i64,
    team: Option<i64>,
    /// player (named_fighter) / monster (ai_fighter) / creature (entity_fighter: companions...) / other.
    kind: &'static str,
    alive: bool,
    summoned: bool,
    /// Carried by another fighter: on its cell, blocks nothing of its own.
    carried: bool,
    /// The ints of its monster information that are pack monsters: its gid, maybe a grade or level too (they are unordered;
    /// enough to recognise a creature whose gid is no level, e.g. the Steamer's turrets).
    monsters: Vec<i64>,
    /// Who summoned it (0: nobody, or not told).
    summoner: i64,
}

#[derive(Default)]
struct State {
    map: Option<i64>,
    fight_map: Option<i64>,
    fight: bool,
    fighters: BTreeMap<i64, Fighter>,
    marks: Marks,
    /// Turns started in this fight.
    turn: u32,
}

impl State {
    fn map(&self) -> Option<i64> {
        self.fight_map.filter(|_| self.fight).or(self.map)
    }

    /// The cells fighters stand on.
    fn taken(&self) -> HashSet<i64> {
        self.fighters.values().filter(|f| f.alive && !f.carried).map(|f| f.cell).collect()
    }

    fn portals(&self) -> Vec<Portal> {
        self.marks.portals(&self.taken(), self.turn)
    }

    fn json(&self) -> Value {
        json!({"map": self.map(), "fight": self.fight, "fighters": self.fighters.values().collect::<Vec<_>>(), "portals": self.portals()})
    }
}

#[derive(Default)]
pub struct FightMap {
    state: Mutex<State>,
    /// The cells of the last map asked for (hovering asks for the line of sight again and again).
    cells: Mutex<Option<(i64, Option<Vec<u8>>)>>,
    turn: Role<TurnBindings>,
    /// Escape watched (the overlay is open): its polling thread runs while this is true.
    escape: Arc<AtomicBool>,
    /// The Dofus window the in-game view covers: it gets the focus back when the view closes.
    covered: Mutex<Option<isize>>,
}

#[derive(serde::Deserialize)]
struct TurnBindings {
    /// FightTurnEvent's character_id: the fighter whose turn starts.
    fighter: String,
}

fn name(ctx: &Ctx, m: &DynamicMessage) -> String {
    ctx.proto.message_name(m.descriptor().full_name()).0
}

/// Obfuscated name of the field of `m` whose exact clear name is `clear`.
fn field(ctx: &Ctx, m: &DynamicMessage, clear: &str) -> Option<String> {
    ctx.proto.field_named(m.descriptor().full_name(), clear)
}

fn named_int(ctx: &Ctx, m: &DynamicMessage, clear: &str) -> Option<i64> {
    field(ctx, m, clear).filter(|f| m.has_field_by_name(f)).map(|f| int(m, &f))
}

fn named_child<'a>(ctx: &Ctx, m: &'a DynamicMessage, clear: &str) -> Option<&'a DynamicMessage> {
    let f = m.descriptor().get_field_by_name(&field(ctx, m, clear)?)?;
    m.has_field(&f).then(|| m.get_field(&f)).and_then(|v| match v {
        std::borrow::Cow::Borrowed(PbValue::Message(x)) => Some(x),
        _ => None,
    })
}

/// First message of the subtree (`m` included) whose clear name is `wanted`.
fn find<'a>(ctx: &Ctx, m: &'a DynamicMessage, wanted: &str) -> Option<&'a DynamicMessage> {
    if name(ctx, m) == wanted {
        return Some(m);
    }
    children(m).into_iter().find_map(|c| find(ctx, c, wanted))
}

/// The ints of a subtree that are pack monsters (fighter positions left out).
fn monster_ints(ctx: &Ctx, m: &DynamicMessage, out: &mut Vec<i64>) {
    for (_, v) in scalar_ints(m) {
        if ctx.data.monsters.contains_key(&v) && !out.contains(&v) {
            out.push(v);
        }
    }
    for c in children(m) {
        if name(ctx, c) != POSITION {
            monster_ints(ctx, c, out);
        }
    }
}

/// What a SpawnInformation tells of the fighters it holds: alive, team, monster ints.
#[derive(Clone)]
struct Outer {
    alive: bool,
    team: Option<i64>,
    monsters: Vec<i64>,
}

/// SpawnInformation: (alive, team = its only enum field).
fn spawn(ctx: &Ctx, m: &DynamicMessage) -> (bool, Option<i64>) {
    let alive = named_int(ctx, m, "alive").unwrap_or(0) != 0;
    let enums: Vec<_> = m.descriptor().fields().filter(|f| matches!(f.kind(), Kind::Enum(_)) && !f.is_list()).collect();
    let team = match enums[..] {
        [ref f] => as_i64(&m.get_field(f)),
        _ => None,
    };
    (alive, team)
}

/// A fighter from its ActorPositionInformation (`outer` = the SpawnInformation holding it, for summons).
fn position(ctx: &Ctx, m: &DynamicMessage, outer: Option<&Outer>) -> Option<Fighter> {
    let id = named_int(ctx, m, "actor_id")?;
    let cell = named_child(ctx, m, "disposition").and_then(|d| named_int(ctx, d, "cell_id"))?;
    let info = find(ctx, m, FIGHTER);
    let (alive, team) =
        info.and_then(|i| named_child(ctx, i, "spawn_information")).map(|s| spawn(ctx, s)).or(outer.map(|o| (o.alive, o.team))).unwrap_or((true, None));
    let kind = info.and_then(|i| {
        [("named_fighter", "player"), ("ai_fighter", "monster"), ("entity_fighter", "creature")]
            .into_iter()
            .find(|(f, _)| named_child(ctx, i, f).is_some())
            .map(|(_, k)| k)
    });
    let stats = info.and_then(|i| named_child(ctx, i, "stats"));
    let summoned = stats.and_then(|s| named_int(ctx, s, "summoned")).unwrap_or(0) != 0;
    let summoner = stats.and_then(|s| named_int(ctx, s, "summoner")).unwrap_or(0);
    let mut monsters = vec![];
    if let Some(ai) = info.and_then(|i| named_child(ctx, i, "ai_fighter")) {
        monster_ints(ctx, ai, &mut monsters);
    }
    if monsters.is_empty() {
        monsters = outer.map(|o| o.monsters.clone()).unwrap_or_default();
    }
    Some(Fighter { id, cell, team, kind: kind.unwrap_or("other"), alive, summoned, carried: false, monsters, summoner })
}

/// Every fighter positioned in a message.
fn fighters(ctx: &Ctx, m: &DynamicMessage, outer: Option<&Outer>, out: &mut Vec<Fighter>) {
    match name(ctx, m).as_str() {
        POSITION => out.extend(position(ctx, m, outer)),
        SPAWN => {
            let (alive, team) = spawn(ctx, m);
            let mut monsters = vec![];
            monster_ints(ctx, m, &mut monsters);
            if monsters.is_empty() {
                monsters = outer.map(|o| o.monsters.clone()).unwrap_or_default();
            }
            let o = Outer { alive, team, monsters };
            children(m).into_iter().for_each(|c| fighters(ctx, c, Some(&o), out));
        }
        // a summon by context: its spawn information's monster (gid + grade or level) for every actor it spawns
        SUMMON => {
            let mut monsters = vec![];
            if let Some(sp) = named_child(ctx, m, "spawn_information") {
                monster_ints(ctx, sp, &mut monsters);
            }
            let o = Outer { monsters, ..outer.cloned().unwrap_or(Outer { alive: true, team: None, monsters: vec![] }) };
            children(m).into_iter().for_each(|c| fighters(ctx, c, Some(&o), out));
        }
        _ => children(m).into_iter().for_each(|c| fighters(ctx, c, outer, out)),
    }
}

impl FightMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a GameActionFightEvent that moves, kills or summons fighters; true when something changed.
    fn action(ctx: &Ctx, d: &DynamicMessage, s: &mut State) -> bool {
        if let Some(changed) = s.marks.action(ctx, d, s.turn) {
            return changed;
        }
        let cell_of = |s: &State, id: i64| s.fighters.get(&id).map(|f| f.cell);
        let set = |s: &mut State, id: i64, f: &dyn Fn(&mut Fighter)| s.fighters.get_mut(&id).map(f).is_some();
        if let Some(x) = named_child(ctx, d, "slide") {
            let Some(target) = named_int(ctx, x, "target_id") else { return false };
            let target_field = field(ctx, x, "target_id");
            let cells: Vec<i64> = scalar_ints(x).into_iter().filter(|(n, _)| Some(n) != target_field.as_ref()).map(|(_, v)| v).collect();
            let end = match (cells.as_slice(), cell_of(s, target)) {
                (&[a, b], Some(now)) if a == now => b,
                (&[a, b], Some(now)) if b == now => a,
                _ => return false,
            };
            return set(s, target, &|f| f.cell = end);
        }
        if let Some(x) =
            named_child(ctx, d, "teleport_on_same_map").or_else(|| named_child(ctx, d, "throw_character")).or_else(|| named_child(ctx, d, "drop_character"))
        {
            let (Some(target), Some(cell)) = (named_int(ctx, x, "target_id"), named_int(ctx, x, "cell")) else { return false };
            return set(s, target, &|f| (f.cell, f.carried) = (cell, false));
        }
        if let Some(x) = named_child(ctx, d, "carry_character") {
            let Some(target) = named_int(ctx, x, "target_id") else { return false };
            return set(s, target, &|f| f.carried = true);
        }
        if let Some(x) = named_child(ctx, d, "exchange_positions") {
            let (Some(caster), Some(target)) = (named_int(ctx, d, "source_id"), named_int(ctx, x, "target_id")) else { return false };
            let (Some(a), Some(b)) = (cell_of(s, caster), cell_of(s, target)) else { return false };
            return set(s, caster, &|f| f.cell = b) | set(s, target, &|f| f.cell = a);
        }
        if let Some(x) = named_child(ctx, d, "death") {
            let ids: Vec<i64> = scalar_ints(x).into_iter().map(|(_, v)| v).collect();
            return ids.into_iter().fold(false, |ch, id| set(s, id, &|f| f.alive = false) | ch);
        }
        if let Some(x) = named_child(ctx, d, "summons") {
            let mut found = vec![];
            fighters(ctx, x, None, &mut found);
            let changed = !found.is_empty();
            // the caster is the summoner when the summon does not say
            let caster = named_int(ctx, d, "source_id").unwrap_or(0);
            s.fighters.extend(found.into_iter().map(|f| (f.id, Fighter { summoner: if f.summoner == 0 { caster } else { f.summoner }, ..f })));
            return changed;
        }
        false
    }

    /// The Dofus window the view covers gets the focus back if it lost it.
    fn refocus(&self) -> bool {
        !crate::sys::game_focused() && crate::sys::focus_game(*self.covered.lock())
    }

    /// The cells of a map (pack file read once per map).
    fn cells(&self, ctx: &Ctx, map: i64) -> Option<Vec<u8>> {
        let mut c = self.cells.lock();
        match &*c {
            Some((m, cells)) if *m == map => cells.clone(),
            _ => {
                let cells = ctx.data.map_cells(map);
                *c = Some((map, cells.clone()));
                cells
            }
        }
    }
}

impl Module for FightMap {
    fn id(&self) -> &'static str {
        "fightmap"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[MAP, FIGHT_MAP, JOIN, SHOW, REFRESH, SYNC, ACTION, MOVE, END, TURN])
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(d) = m.decoded.as_ref() else { return };
        if m.name == TURN {
            if let Some(b) = self.turn.get(ctx, "turn") {
                ctx.emit("fightmap", "turn", json!({"id": int(d, &b.fighter)}));
            }
            // portals deactivated in the last turn open again
            let mut s = self.state.lock();
            s.turn += 1;
            let turn = s.turn;
            if s.marks.off_in(turn - 1) {
                ctx.emit("fightmap", "state", s.json());
            }
            return;
        }
        let mut s = self.state.lock();
        let changed = match m.name.as_str() {
            MAP => {
                let map = named_int(ctx, d, "map_id");
                std::mem::replace(&mut s.map, map) != map && !s.fight
            }
            FIGHT_MAP => {
                let map = named_int(ctx, d, "map_id");
                std::mem::replace(&mut s.fight_map, map) != map
            }
            JOIN => {
                (s.fight, s.fight_map, s.turn) = (true, None, 0);
                s.fighters.clear();
                s.marks.clear();
                true
            }
            // the map is only shown during a fight: nothing kept once it is over
            END => {
                let was = std::mem::replace(&mut s.fight, false);
                s.fight_map = None;
                s.fighters.clear();
                s.marks.clear();
                was
            }
            // fighters only matter during a fight
            _ if !s.fight => false,
            SHOW | REFRESH | SYNC => {
                let mut found = vec![];
                fighters(ctx, d, None, &mut found);
                if m.name == SYNC && !found.is_empty() {
                    s.fighters.clear();
                }
                let changed = !found.is_empty();
                for f in found {
                    // a refresh without spawn information keeps what is known
                    let f = match s.fighters.remove(&f.id) {
                        Some(old) => Fighter {
                            kind: if f.kind == "other" { old.kind } else { f.kind },
                            monsters: if f.monsters.is_empty() { old.monsters } else { f.monsters },
                            summoner: if f.summoner == 0 { old.summoner } else { f.summoner },
                            ..f
                        },
                        None => f,
                    };
                    s.fighters.insert(f.id, f);
                }
                changed
            }
            MOVE => {
                let (Some(id), Some(cells)) = (named_int(ctx, d, "character_id"), field(ctx, d, "cells")) else { return };
                let last = match d.get_field_by_name(&cells).as_deref() {
                    Some(PbValue::List(l)) => l.last().and_then(as_i64),
                    _ => None,
                };
                match (last, s.fighters.get_mut(&id)) {
                    (Some(cell), Some(f)) if f.cell != cell => {
                        f.cell = cell;
                        true
                    }
                    _ => false,
                }
            }
            ACTION => Self::action(ctx, d, &mut s),
            _ => false,
        };
        if changed {
            ctx.emit("fightmap", "state", s.json());
        }
    }

    /// `state`: {map, fight, fighters: [{id, cell, team, kind, alive, summoned, carried, monsters, summoner}]}; `cells` {map}: the flags of its
    /// 560 cells (bits 1 walkable, 2 sight goes through, 4 / 8 red / blue placement) or null; `sight` {from, min, max,
    /// ignore?: [fighter ids]}: {seen, hidden} cells of the current map at distance min..=max from `from`, the living
    /// fighters blocking the sight (but the ones of `ignore` and whoever stands on `from`). `window`: the Dofus window's client
    /// area {x, y, width, height} (screen pixels) or null; `watch_escape` {on}: `fightmap:escape` at each Escape press while on, `fightmap:click` at each left / right click;
    /// `refocus`: the Dofus window it covers gets the focus back if it lost it. `release`: the view closes: `watch_escape`
    /// off + `refocus`.
    /// State `portals`: [{cell, author, open}] in the order they were put down. Call `portals` {author, start?: a portal}: the
    /// Portail spell of that caster simulated (`simulate`), {base: not cast, sims: {cell: cast there}} for every free walkable
    /// cell, each {added, dropped, via, path: [cells], distance}.
    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "state" => Ok(self.state.lock().json()),
            "window" => {
                let Some(w) = crate::sys::game_window() else { return Ok(Value::Null) };
                *self.covered.lock() = Some(w.handle);
                Ok(json!({"x": w.x, "y": w.y, "width": w.width, "height": w.height}))
            }
            "release" => {
                self.escape.store(false, Ordering::Relaxed);
                Ok(json!(self.refocus()))
            }
            "refocus" => Ok(json!(self.refocus())),
            "watch_escape" => {
                let on = args["on"].as_bool().unwrap_or(false);
                if on && !self.escape.swap(true, Ordering::Relaxed) {
                    let emit = ctx.emitter();
                    let vks = vec![crate::sys::VK_ESCAPE, crate::sys::VK_LBUTTON, crate::sys::VK_RBUTTON];
                    crate::sys::watch_presses(self.escape.clone(), vks, move |vk| {
                        emit(if vk == crate::sys::VK_ESCAPE { "fightmap:escape" } else { "fightmap:click" }, Value::Null)
                    });
                } else if !on {
                    self.escape.store(false, Ordering::Relaxed);
                }
                Ok(Value::Null)
            }
            "cells" => Ok(self.cells(ctx, arg_i64(&args, "map")?).map_or(Value::Null, |c| json!(c))),
            "portals" => {
                let author = arg_i64(&args, "author")?;
                let start = args["start"].as_i64();
                let (map, net, taken, caster) = {
                    let s = self.state.lock();
                    let net: Vec<(i64, bool)> = s.portals().into_iter().filter(|p| p.author == author).map(|p| (p.cell, p.open)).collect();
                    let taken = s.taken();
                    let caster = s.fighters.get(&author).map(|f| f.cell).filter(|c| (0..MAP_CELLS as i64).contains(c));
                    (s.map(), net, taken, caster)
                };
                let cells = map.and_then(|m| self.cells(ctx, m));
                let free = |c: i64| (0..MAP_CELLS as i64).contains(&c) && !taken.contains(&c) && cells.as_ref().is_some_and(|m| m[c as usize] & MOV != 0);
                // every cell at once: hovering is then a lookup, with no call per cell
                let sims: BTreeMap<i64, Sim> = (0..MAP_CELLS as i64).filter(|&c| free(c)).map(|c| (c, simulate(&net, Some(c), &free, caster, start))).collect();
                Ok(json!({"base": simulate(&net, None, &free, caster, start), "sims": sims}))
            }
            "sight" => {
                let from = arg_i64(&args, "from")?;
                let (min, max) = (args["min"].as_i64().unwrap_or(1) as i32, args["max"].as_i64().unwrap_or(1) as i32);
                let ignore: HashSet<i64> = args["ignore"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
                let (map, occupied) = {
                    let s = self.state.lock();
                    let occupied: HashSet<usize> = s
                        .fighters
                        .values()
                        .filter(|f| f.alive && !f.carried && !ignore.contains(&f.id) && f.cell != from && (0..MAP_CELLS as i64).contains(&f.cell))
                        .map(|f| f.cell as usize)
                        .collect();
                    (s.map(), occupied)
                };
                let Some(cells) = map.and_then(|m| self.cells(ctx, m)) else { return Ok(Value::Null) };
                let (seen, hidden) = sight(&cells, &occupied, from as usize, min, max);
                Ok(json!({"seen": seen, "hidden": hidden}))
            }
            _ => Err(unknown("fightmap", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_core::RawMessage;
    use loutil_data::maps::portal_path;

    /// Recorded fights (pipeline/sniff.py captures, local only: they hold the players' ids) replayed through the hub: the
    /// fight map is found in the pack and every living fighter, after every message, stands on a walkable cell of it; summons
    /// carry their gid and summoner.
    /// Skipped without the files.
    #[test]
    fn fighters_stand_on_walkable_cells() {
        let Some(pack) = testkit::pack() else { return };
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        for name in ["capture_harebourg", "capture_bosses", "capture_poison"] {
            let rec = pack.join(format!("../work/{name}.jsonl"));
            if !rec.exists() {
                continue;
            }
            let (hub, _) = testkit::hub("loutil-fightmap-replay").unwrap();
            let (mut checked, mut moves) = (0, 0);
            let mut last = String::new();
            let mut fight_map = false;
            let mut turret = false;
            for line in std::fs::read_to_string(&rec).unwrap().lines() {
                let r: Value = serde_json::from_str(line).unwrap();
                let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
                if h.len() / 2 < len as usize {
                    continue; // truncated record
                }
                let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
                hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload: hex(h) });
                // the fight's own map comes a little after the fighters (until then: the roleplay map)
                match r["name"].as_str() {
                    Some(JOIN) => fight_map = false,
                    Some(FIGHT_MAP) => fight_map = true,
                    _ => {}
                }
                let s = hub.call("fightmap", "state", Value::Null).unwrap();
                if r["name"] == END {
                    assert!(s["fight"] == false && s["fighters"].as_array().unwrap().is_empty(), "{name}: cleared at the fight's end");
                }
                if s["fight"] != true || s["map"].is_null() || !fight_map {
                    continue;
                }
                let cells = hub.call("fightmap", "cells", json!({"map": s["map"]})).unwrap();
                let cells: Vec<u8> = serde_json::from_value(cells).unwrap_or_else(|_| panic!("{name}: map {} in the pack", s["map"]));
                let at: Vec<(i64, i64)> = s["fighters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|f| f["alive"] == true)
                    .map(|f| (f["id"].as_i64().unwrap(), f["cell"].as_i64().unwrap()))
                    .collect();
                for (id, cell) in &at {
                    assert!(cells[*cell as usize] & MOV != 0, "{name}: fighter {id} on cell {cell}, not walkable ({:?})", r["name"]);
                }
                let taken: HashSet<i64> = at.iter().map(|(_, c)| *c).collect();
                assert_eq!(taken.len(), at.len(), "{name}: two fighters on one cell {at:?} ({:?})", r["name"]);
                // a Steamer's turret (Harponneuse 5836) summoned by the player's character
                turret |= s["fighters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f["monsters"].as_array().unwrap().contains(&json!(5836)) && f["summoner"].as_i64() > Some(0));
                let now = format!("{at:?}");
                if now != last && !last.is_empty() {
                    moves += 1;
                }
                last = now;
                checked += 1;
            }
            eprintln!("{name}: {checked} fight states checked, {moves} position changes");
            assert!(checked > 0 && moves > 0, "{name}: fighters followed");
            assert!(turret || name != "capture_harebourg", "{name}: the player's Harponneuse, with its gid and summoner");
        }
    }

    /// work/capture_poison.jsonl: an Eliotrope puts down portals on 188, 332, 265 then 257 (mark ids 1 to 4, the first one
    /// deactivated / activated by mark id; the 4th by a Portail cast into portal 2 from 264); the game's casts went 2, 3, 1
    /// (, 4). Skipped without the file.
    #[test]
    fn portals_of_a_real_fight() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_poison.jsonl");
        if !rec.exists() {
            return;
        }
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        let (hub, _) = testkit::hub("loutil-fightmap-portals").unwrap();
        let eliotrope = 61885579559i64;
        let mut seen: Vec<Vec<i64>> = vec![];
        let mut predicted = false;
        let simulated = || hub.call("fightmap", "portals", json!({"author": eliotrope})).unwrap();
        for line in std::fs::read_to_string(&rec).unwrap().lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
            if h.len() / 2 < len as usize {
                continue;
            }
            let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
            hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload: hex(h) });
            let s = hub.call("fightmap", "state", Value::Null).unwrap();
            let cells: Vec<i64> = s["portals"].as_array().unwrap().iter().filter(|p| p["author"] == eliotrope).map(|p| p["cell"].as_i64().unwrap()).collect();
            if seen.last() != Some(&cells) {
                seen.push(cells.clone());
            }
            // the Portail cast into portal 2 (record 3666): its portal came out on 257
            let at = s["fighters"].as_array().unwrap().iter().find(|f| f["id"] == eliotrope).map(|f| f["cell"].clone());
            if cells.len() == 3 && at == Some(json!(264)) && !predicted {
                let all = simulated();
                let n = &all["sims"]["332"];
                assert!(n["via"] == true && n["added"] == 257 && n["dropped"].is_null(), "{n}");
                // the new network's way from the new portal: the game's 4 portal ways went 2, 3, 1, 4 from portal 2
                assert_eq!(n["path"], json!(portal_path(&[188, 332, 265, 257], 257)));
                predicted = true;
            }
            if cells.len() == 4 {
                let all = simulated();
                // into portal 2 from 264: out 4 cells past its exit (325, the casts' target: a monster stands there, no portal)
                let net = &all["sims"]["332"];
                assert!(net["via"] == true && net["added"].is_null() && net["dropped"].is_null() && net["path"] == json!([]));
                assert!(predicted, "the 4th portal predicted from the Portail cast");
                assert!(all["base"]["path"].as_array().unwrap().is_empty() && all["base"]["added"].is_null());
                // a fifth portal: the oldest (188) goes
                let sims = all["sims"].as_object().unwrap();
                let (_, n) = sims.iter().find(|(c, n)| n["added"].to_string() == **c && n["via"] == false).expect("a free cell for a fifth portal");
                assert_eq!(n["dropped"], json!(188));
                assert_eq!(n["path"].as_array().unwrap().len(), 4, "through every open portal");
                return;
            }
        }
        panic!("four portals never seen: {seen:?}");
    }
}
