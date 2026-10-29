//! Combat: the monsters of the player's current fight, with the author's notes for each (warning / invulnerability / tips,
//! data pack `monsters.json`). Fighters come from the fight messages (shown at placement, synchronised every turn, summoned
//! during the fight); each one is an ActorPositionInformation whose MonsterFighter carries the creature gid, grade and level
//! as three unordered ints. Which int is which is learnt (`Layout`): a message whose fighters fit only one arrangement (gid =
//! a monster, grade = one of its grades, level = a plausible level) votes for it; the winner is kept in `layout.json` (per
//! field names, so a game update starts over) and then used for every fighter, even an unknown gid. Levels are often scaled
//! (dungeons, dimensions...), so they are not required to match the grade. Strictly passive, like everything else.
use crate::kit::{arg_i64, arg_str, unknown, Role, Store};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_data::Data;
use loutil_proto::util::{as_i64, int, messages, scalar_ints};
use loutil_proto::{DynamicMessage, PbValue, ReflectMessage};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The player enters a fight (placement starts): a new fight.
pub const JOIN: &str = "FightJoinEvent";
/// Reconnection to a running fight: a new fight too (the fighters follow).
pub const JOIN_RUNNING: &str = "FightJoinRunningEvent";
/// One fighter shown / refreshed (placement).
pub const SHOW: &str = "FightFighterShowEvent";
pub const REFRESH: &str = "FightFighterRefreshEvent";
/// Every fighter of the fight (start of each turn).
pub const SYNC: &str = "FightSynchronizeEvent";
/// Fight actions: only the summons (new fighters) are read.
pub const ACTION: &str = "GameActionFightEvent";
pub const END: &str = "FightEndEvent";
/// A buff / debuff put on a fighter (PA / PM poisons are watched), and its end.
pub const BOOST_ADD: &str = "FightBoostAddEvent";
pub const BOOST_REMOVE: &str = "FightBoostRemoveEvent";
/// Client requests at login: the character's id = the player's fighter id.
pub const SELECT: &str = "CharacterSelectionRequest";
pub const FIRST_SELECT: &str = "CharacterFirstSelectionRequest";
/// A fighter's turn starts (the player's: `combat:turn`, Automation « À votre tour »).
pub const TURN: &str = "FightTurnEvent";
/// Turn order, every fighter flagged dead or not (refreshed after each death): no turn sound for a dead player.
pub const TIMELINE: &str = "TimelineRefreshEvent";
/// A fighter's characteristics changed: the player's confusion is read (boss helpers).
pub const STATS: &str = "FightRefreshCharacterStatsEvent";
/// Characteristic `confusion`: the player's spells land this many quarter turns clockwise around them (negative = counter
/// clockwise). Comte Harebourg's Comtoise sets it at each turn start from the player's life (effects 770-775, ±1 to ±3), every
/// melee hit adds 1 (Pendultimatum, 771); the game sums them (seen: 1 then 2 after a melee hit, capture_harebourg).
const CONFUSION: i64 = 74;
/// A new round of the fight (its number: boss helpers count the rounds since a monster appeared).
pub const ROUND: &str = "FightNewRoundEvent";

/// Damage actions of a fixed element (effects table: 91-95 steals, 96-100 damage) -> element (0 neutral, 1 earth, 2 fire,
/// 3 water, 4 air): they teach which LifePointsLost int is the element (`Combat::damage`), read for the others ("pire
/// élément", spell-specific damage...).
const ELEMENT_OF: [(i64, i64); 10] = [(91, 3), (92, 1), (93, 4), (94, 2), (95, 0), (96, 3), (97, 1), (98, 4), (99, 2), (100, 0)];
/// Script-named messages carrying a character's name (fight placement / map actors), under its ActorPositionInformation:
/// the fallback when no login was seen (names typed in the Automation tab, `add_name`), and the known characters' names.
const NAMED: [&str; 2] = ["NamedFighterInformation", "NamedActor"];

/// PA / PM poisons: effects table ids ("#2 dommages <element> pour #1 PA / PM utilisé") -> (PA | PM, element).
const POISONS: [(i64, &str, &str); 10] = [
    (1131, "PA", "Air"),
    (1132, "PA", "Eau"),
    (1133, "PA", "Feu"),
    (1134, "PA", "Neutre"),
    (1135, "PA", "Terre"),
    (1136, "PM", "Air"),
    (1137, "PM", "Eau"),
    (1138, "PM", "Feu"),
    (1139, "PM", "Neutre"),
    (1140, "PM", "Terre"),
];

/// Obfuscated field names of the buff messages (bindings.json `boost`).
#[derive(Deserialize, Clone)]
struct BoostBindings {
    /// FightRemovableEffectExtendedInformation: the effect (FightRemovableEffect) and its action id
    effect: String,
    action: String,
    /// FightRemovableEffect: its int32 (uid, spell, duration...: same type, so which one is the uid is learnt, see
    /// `Layout::boost_votes`) and its int64 target
    ints: Vec<String>,
    target: String,
    /// FightBoostRemoveEvent: its only int32, the uid of the buff that ends
    remove_uid: String,
    /// CharacterSelectionRequest / CharacterFirstSelectionRequest character_id
    character: Vec<String>,
}

/// Obfuscated field names of the turn start (bindings.json `turn`): FightTurnEvent's character_id, TimelineFighter's
/// fighter_id and dead (absent in older packs: every turn rings).
#[derive(Deserialize, Clone)]
struct TurnBindings {
    fighter: String,
    #[serde(default)]
    timeline_id: Option<String>,
    #[serde(default)]
    timeline_dead: Option<String>,
}

/// The fighters a timeline flags dead (every TimelineFighter under `m`).
fn timeline_dead(id: &str, dead: &str, m: &DynamicMessage, out: &mut BTreeSet<i64>) {
    if m.descriptor().get_field_by_name(id).is_some() {
        if m.get_field_by_name(dead).and_then(|v| v.as_bool()) == Some(true) {
            out.insert(int(m, id));
        }
        return;
    }
    for c in children(m) {
        timeline_dead(id, dead, c, out);
    }
}

/// A characteristic of a fighter's stats refresh: (fighter id, value), None when the message does not carry it. Clear names
/// (fighter_id, stats -> characteristics -> characteristic_id); the value is `detailed` (its unnamed ints add up to the total)
/// or a plain `value` -> `total`.
fn characteristic(ctx: &Ctx, m: &DynamicMessage, id: i64) -> Option<(i64, i64)> {
    let field = |m: &DynamicMessage, clear: &str| ctx.proto.field_named(m.descriptor().full_name(), clear);
    let stats = m.get_field_by_name(&field(m, "stats")?)?.as_message()?.clone();
    let c = messages(&stats, &field(&stats, "characteristics")?).into_iter().find(|c| field(c, "characteristic_id").is_some_and(|f| int(c, &f) == id))?;
    let sub = |clear: &str| field(&c, clear).filter(|f| c.has_field_by_name(f)).and_then(|f| c.get_field_by_name(&f)?.as_message().cloned());
    let value = match (sub("detailed"), sub("value")) {
        (Some(d), _) => scalar_ints(&d).iter().map(|(_, v)| v).sum(),
        (None, Some(v)) => field(&v, "total").map_or(0, |f| int(&v, &f)),
        _ => 0,
    };
    Some((int(m, &field(m, "fighter_id")?), value))
}

/// A PA / PM poison running on a fighter: target and effect id (see `POISONS`).
#[derive(Clone, Debug, PartialEq)]
struct Poison {
    target: i64,
    action: i64,
}

/// Buff ints kept to learn which one is the uid (see `Fight::recent`).
const RECENT_BOOSTS: usize = 500;

/// The PA / PM poisons put by a message (every FightRemovableEffectExtendedInformation in it) into `f.poisons` (by the
/// buff's ints, see `BoostBindings::ints`); every buff's ints go to `f.recent`.
fn boosts_added(b: &BoostBindings, m: &DynamicMessage, f: &mut Fight) -> bool {
    let mut changed = false;
    if m.descriptor().get_field_by_name(&b.action).is_some() {
        if let Some(PbValue::Message(e)) = m.get_field_by_name(&b.effect).as_deref() {
            let ints: Vec<i64> = b.ints.iter().map(|x| int(e, x)).collect();
            let action = int(m, &b.action);
            if POISONS.iter().any(|p| p.0 == action) {
                changed |= f.poisons.insert(ints.clone(), Poison { target: int(e, &b.target), action }).is_none();
            }
            if f.recent.len() == RECENT_BOOSTS {
                f.recent.pop_front();
            }
            f.recent.push_back(ints);
        }
        return changed;
    }
    for c in children(m) {
        changed |= boosts_added(b, c, f);
    }
    changed
}

/// Obfuscated field names of the fighter messages (bindings.json `fight`).
#[derive(Deserialize, Clone)]
struct Bindings {
    actor_id: String,
    monster: Vec<String>,
    summoned: String,
    /// GameActionFightEvent's `death` member (absent in older packs)
    #[serde(default)]
    death: Option<String>,
    /// FightCharacteristics' summoner (actor id of whoever summoned it; absent in older packs)
    #[serde(default)]
    summoner: Option<String>,
    /// SpawnInformation's alive (the synchronisation lists the dead fighters too; absent in older packs)
    #[serde(default)]
    alive: Option<String>,
}

/// A monster fighter: the three unordered ints of its MonsterFighter (gid / grade / level, see `Layout`), whether it was
/// summoned during the fight, by whom (`summoner`, 0 = nobody) and whether it first appeared in a fight action (a summon).
#[derive(Clone, Debug, PartialEq)]
struct Fighter {
    ints: [i64; 3],
    summoned: bool,
    summoner: i64,
    spawned: bool,
    /// The message's spawn information says it is dead (None: no spawn information in it).
    dead: Option<bool>,
    /// Wave it belongs to (multi-wave fights: dungeons, Songes; 0 = none). None: not in the message.
    wave: Option<i64>,
    /// Summoned by a monster (`summoned_monsters`): its gid is known, `ints` = [gid, grade or level, 0] (no arrangement).
    gid: Option<i64>,
}

impl Fighter {
    /// It appeared during the fight (summon, double...), it was not placed at the start.
    fn summon(&self) -> bool {
        self.summoned || self.summoner != 0 || self.spawned
    }
}

/// The six ways the three ints can be (gid, grade, level).
pub(crate) const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

/// Learnt arrangement of the MonsterFighter ints: votes per `PERMS` entry, for the build whose field names are `fields`;
/// and which FightRemovableEffect int is the buff uid: votes per int of `boost_fields` (a FightBoostRemoveEvent uid found
/// in a single int of the buffs seen votes for it).
#[derive(Default)]
struct Layout {
    fields: Vec<String>,
    votes: [u32; 6],
    boost_fields: Vec<String>,
    boost_votes: Vec<u32>,
}

impl Layout {
    fn best(&self) -> Option<usize> {
        (0..6).filter(|&i| self.votes[i] > 0).max_by_key(|&i| self.votes[i])
    }

    fn boost_uid(&self) -> Option<usize> {
        (0..self.boost_votes.len()).filter(|&i| self.boost_votes[i] > 0).max_by_key(|&i| self.boost_votes[i])
    }
}

/// Highest level a monster can plausibly have in a fight (scaled ones included): the 99th percentile of the grade levels
/// + margin (a few special entities go up to 2400, which would let most gids pass for a level).
pub(crate) fn max_level(data: &Data) -> i64 {
    static TOP: OnceLock<i64> = OnceLock::new();
    *TOP.get_or_init(|| {
        let mut l: Vec<i64> = data.monsters.values().flat_map(|m| m.levels.iter().copied()).collect();
        l.sort_unstable();
        l.get(l.len() * 99 / 100).copied().unwrap_or(200) + 100
    })
}

/// `p` makes sense for these ints: the gid is a monster, the grade one of its grades and the level plausible.
pub(crate) fn fits(data: &Data, ints: &[i64; 3], p: usize, top: i64) -> bool {
    let [g, gr, l] = PERMS[p].map(|i| ints[i]);
    data.monsters.get(&g).is_some_and(|m| gr >= 1 && gr as usize <= m.levels.len()) && (1..=top).contains(&l)
}

/// The arrangements that fit every one of these fighters, the most exact first (level = the grade's level).
fn fitting(data: &Data, all: &[[i64; 3]]) -> Vec<usize> {
    let top = max_level(data);
    let exact = |p: usize| {
        all.iter()
            .filter(|x| {
                let [g, gr, l] = PERMS[p].map(|i| x[i]);
                data.monsters.get(&g).and_then(|m| m.levels.get((gr - 1) as usize)) == Some(&l)
            })
            .count()
    };
    let mut ok: Vec<usize> = (0..6).filter(|&p| !all.is_empty() && all.iter().all(|x| fits(data, x, p, top))).collect();
    ok.sort_by_key(|&p| std::cmp::Reverse(exact(p)));
    ok
}

#[derive(Default)]
struct Fight {
    /// a fight was seen (it may be over)
    seen: bool,
    started: f64,
    /// end time, None while it runs
    ended: Option<f64>,
    /// actor id -> monster
    fighters: BTreeMap<i64, Fighter>,
    /// actor ids of the monsters that died (death action, or left out of a synchronisation)
    dead: BTreeSet<i64>,
    /// PA / PM poisons running, by the buff's ints (one is the uid FightBoostRemoveEvent names when it ends)
    poisons: BTreeMap<Vec<i64>, Poison>,
    /// the ints of the latest buffs (any effect), to learn which one is the uid
    recent: VecDeque<Vec<i64>>,
    /// fighter id -> its confusion (quarter turns, see `CONFUSION`), as last refreshed
    confusion: BTreeMap<i64, i64>,
    /// current round (0 = placement / not seen yet)
    round: i64,
    /// actor id -> round it first appeared in (placement = 1)
    since: BTreeMap<i64, i64>,
    /// actor id -> last round its turn started in
    played: BTreeMap<i64, i64>,
    /// actor id -> element of the last damage it took that was not neutral (see `ELEMENT_OF`), since its turn started
    hit: BTreeMap<i64, i64>,
}

pub struct Combat {
    bind: Role<Bindings>,
    enabled: AtomicBool,
    /// the player's turn is announced (`combat:turn`), even with the fight following off
    turn: AtomicBool,
    turn_bind: Role<TurnBindings>,
    /// the fighters the latest timeline flags dead
    dead: Mutex<BTreeSet<i64>>,
    fight: Mutex<Fight>,
    /// `layout.json`, written in the background (a vote comes with most fighter messages)
    layout: Store<Layout>,
    boost_bind: Role<BoostBindings>,
    /// obfuscated name of the fighters' `wave` field, found in the first fighter message that has it
    wave: Mutex<Option<String>>,
    /// the player's characters, kept in `me.json`, so the poisons are told apart without the login being seen again
    me: Store<Me>,
    /// LifePointsLost int field -> votes for it being the element (see `ELEMENT_OF`), for the session
    element_votes: Mutex<BTreeMap<String, u32>>,
}

/// The player's characters (`me.json`).
#[derive(Default)]
struct Me {
    /// every character id seen at login or found by name (an id never changes), the last one seen last
    ids: Vec<i64>,
    /// their names, read from a fight they are in
    names: BTreeMap<i64, String>,
    /// names typed by the user (Automation tab) not found in a fight yet: the fighter named so joins `ids`
    pending: Vec<String>,
}

impl Me {
    fn add(&mut self, id: i64) {
        self.ids.retain(|x| *x != id);
        self.ids.push(id);
    }
}

impl Default for Combat {
    fn default() -> Self {
        Self::new()
    }
}

/// Child messages of `m` (message fields, lists and map values), borrowed: fight messages are walked for every action.
fn children(m: &DynamicMessage) -> Vec<&DynamicMessage> {
    let mut out = vec![];
    for (_, v) in m.fields() {
        match v {
            PbValue::Message(x) => out.push(x),
            PbValue::List(l) => out.extend(l.iter().filter_map(PbValue::as_message)),
            PbValue::Map(mp) => out.extend(mp.values().filter_map(PbValue::as_message)),
            _ => {}
        }
    }
    out
}

/// First message of the subtree (depth first, `m` included) that has the field `name` in its schema.
fn find<'a>(m: &'a DynamicMessage, name: &str) -> Option<&'a DynamicMessage> {
    if m.descriptor().get_field_by_name(name).is_some() {
        return Some(m);
    }
    children(m).into_iter().find_map(|c| find(c, name))
}

/// Every monster fighter of a message: (actor id, fighter). Actors that are not monsters (players, tax collectors) are left out.
/// `wave` = the FightFighterInformation `wave` field (obfuscated name), when known.
fn fighters(b: &Bindings, wave: Option<&str>, m: &DynamicMessage, out: &mut Vec<(i64, Fighter)>) {
    if m.descriptor().get_field_by_name(&b.actor_id).is_none() {
        for c in children(m) {
            fighters(b, wave, c, out);
        }
        return;
    }
    let Some(mon) = b.monster.first().and_then(|f| find(m, f)) else { return };
    let [Some(x), Some(y), Some(z)] = [0, 1, 2].map(|i| b.monster.get(i).map(|f| int(mon, f))) else { return };
    let summoned = find(m, &b.summoned).and_then(|s| s.get_field_by_name(&b.summoned).and_then(|v| as_i64(&v))).unwrap_or(0) != 0;
    let summoner = b.summoner.as_deref().and_then(|f| Some(int(find(m, f)?, f))).unwrap_or(0);
    // proto3: an alive fighter carries alive = true, a dead one a spawn information without it
    let dead = b.alive.as_deref().and_then(|f| Some(!find(m, f)?.get_field_by_name(f)?.as_bool()?));
    let wave = wave.and_then(|f| Some(int(find(m, f)?, f)));
    out.push((int(m, &b.actor_id), Fighter { ints: [x, y, z], summoned, summoner, spawned: false, dead, wave, gid: None }));
}

/// The creatures a monster summons during the fight (GameActionFightEvent summons by context: script-named
/// SummonContextInformation): its spawn information's Monster (two unordered ints: the gid = the one that is a pack monster,
/// the other its grade or level), once per actor spawned (its summons: SpawnInformation {alive, position -> actor_id});
/// summoned / summoner / wave as for the other fighters.
fn summoned_monsters(ctx: &Ctx, b: &Bindings, m: &DynamicMessage, out: &mut Vec<(i64, Fighter)>) {
    let name = m.descriptor().full_name().to_owned();
    if ctx.proto.message_name(&name).0 != "SummonContextInformation" {
        for c in children(m) {
            summoned_monsters(ctx, b, c, out);
        }
        return;
    }
    let Some(spawn) = ctx.proto.field_named(&name, "spawn_information").and_then(|f| m.get_field_by_name(&f)?.as_message().cloned()) else { return };
    // (gid, the other int): a pack monster whose other int is a plausible grade or level (both ints can be gids: 5815 level 270)
    fn monster(data: &Data, m: &DynamicMessage) -> Option<(i64, i64)> {
        let ints: Vec<i64> = scalar_ints(m).into_iter().map(|(_, v)| v).collect();
        let other = |k: usize| ints.iter().enumerate().find(|(i, v)| *i != k && **v != 0).map_or(0, |(_, v)| *v);
        let found = (0..ints.len()).find(|&k| data.monsters.contains_key(&ints[k]) && (0..=max_level(data)).contains(&other(k)));
        match found {
            Some(k) => Some((ints[k], other(k))),
            None => children(m).into_iter().find_map(|c| monster(data, c)),
        }
    }
    let Some((gid, other)) = monster(&ctx.data, &spawn) else { return };
    let summoned = find(m, &b.summoned).and_then(|s| s.get_field_by_name(&b.summoned).and_then(|v| as_i64(&v))).unwrap_or(0) != 0;
    let summoner = b.summoner.as_deref().and_then(|f| Some(int(find(m, f)?, f))).unwrap_or(0);
    let wave = ctx.proto.field_named(&name, "wave").map(|f| int(m, &f));
    let mut actors = vec![];
    actor_messages(&b.actor_id, m, &mut actors);
    for a in actors {
        out.push((int(a, &b.actor_id), Fighter { ints: [gid, other, 0], summoned, summoner, spawned: true, dead: None, wave, gid: Some(gid) }));
    }
}

/// Every message under `m` (itself included) that has the field `actor_id`.
fn actor_messages<'a>(actor_id: &str, m: &'a DynamicMessage, out: &mut Vec<&'a DynamicMessage>) {
    if m.descriptor().get_field_by_name(actor_id).is_some() {
        out.push(m);
        return;
    }
    for c in children(m) {
        actor_messages(actor_id, c, out);
    }
}

/// Obfuscated name of the fighters' `wave` field (clear name, so it survives updates): the first message under `m` that has one.
fn wave_field(ctx: &Ctx, m: &DynamicMessage) -> Option<String> {
    ctx.proto.field_named(m.descriptor().full_name(), "wave").or_else(|| children(m).into_iter().find_map(|c| wave_field(ctx, c)))
}

/// Every (actor id, strings) of a message's ActorPositionInformation that has a `NAMED` message: its non-empty strings
/// (NamedFighterInformation has two, the name told apart by value).
fn named_actors(ctx: &Ctx, actor_id: &str, m: &DynamicMessage, out: &mut Vec<(i64, Vec<String>)>) {
    if m.descriptor().get_field_by_name(actor_id).is_none() {
        for c in children(m) {
            named_actors(ctx, actor_id, c, out);
        }
        return;
    }
    if let Some(strings) = strings_of_named(ctx, m) {
        out.push((int(m, actor_id), strings));
    }
}

/// The non-empty strings of the first `NAMED` message under `m`.
fn strings_of_named(ctx: &Ctx, m: &DynamicMessage) -> Option<Vec<String>> {
    if NAMED.contains(&ctx.proto.message_name(m.descriptor().full_name()).0.as_str()) {
        return Some(m.fields().filter_map(|(_, v)| v.as_str().map(|s| s.trim().to_owned())).filter(|s| !s.is_empty()).collect());
    }
    children(m).into_iter().find_map(|c| strings_of_named(ctx, c))
}

/// The players' summons (Steamer turrets, Osamodas creatures...: monsters too): summoned by someone who is not a monster of
/// the fight (a character), or by such a summon.
fn ally_summons(fighters: &BTreeMap<i64, Fighter>) -> BTreeSet<i64> {
    let mut allies = BTreeSet::new();
    loop {
        let before = allies.len();
        for (actor, x) in fighters {
            if x.summoner != 0 && (!fighters.contains_key(&x.summoner) || allies.contains(&x.summoner)) {
                allies.insert(*actor);
            }
        }
        if allies.len() == before {
            return allies;
        }
    }
}

impl Combat {
    pub fn new() -> Self {
        Self {
            bind: Role::new(),
            enabled: AtomicBool::new(true),
            turn: AtomicBool::new(false),
            turn_bind: Role::new(),
            dead: Mutex::new(BTreeSet::new()),
            fight: Mutex::new(Fight::default()),
            layout: Store::default(),
            boost_bind: Role::new(),
            wave: Mutex::new(None),
            me: Store::default(),
            element_votes: Mutex::new(BTreeMap::new()),
        }
    }

    fn boosts(&self, ctx: &Ctx) -> Option<&BoostBindings> {
        self.boost_bind.get(ctx, "boost")
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.get(ctx, "fight")
    }

    /// The fighters of a message: those named like a typed name join the player's characters, the player's characters
    /// without a name get theirs (a single string: the other one is unknown). True when something changed (saved).
    fn name_fighters(&self, ctx: &Ctx, actor_id: &str, d: &DynamicMessage) -> bool {
        let wanted = {
            let me = self.me.lock();
            !me.pending.is_empty() || me.ids.iter().any(|id| !me.names.contains_key(id))
        };
        if !wanted {
            return false;
        }
        let mut found = vec![];
        named_actors(ctx, actor_id, d, &mut found);
        let mut me = self.me.lock();
        let mut changed = false;
        for (id, strings) in found.into_iter().filter(|(id, _)| *id > 0) {
            let typed = strings.iter().find_map(|s| me.pending.iter().position(|p| p.to_lowercase() == s.to_lowercase()).map(|k| (k, s.clone())));
            if let Some((k, name)) = typed {
                ctx.log(format!("combat: character « {name} » = {id}"));
                me.pending.remove(k);
                me.add(id);
                me.names.insert(id, name);
                changed = true;
            } else if let ([name], true) = (&strings[..], me.ids.contains(&id) && !me.names.contains_key(&id)) {
                me.names.insert(id, name.clone());
                changed = true;
            }
        }
        drop(me);
        if changed {
            self.me.touch();
        }
        changed
    }

    /// The buff int holding the uid: a removed uid found in a single int of the recent buffs votes for that int (saved).
    fn boost_uid(&self, ctx: &Ctx, f: &Fight, uid: i64) -> Option<usize> {
        let n = f.recent.front().map_or(0, Vec::len);
        let hits: Vec<usize> = (0..n).filter(|&k| f.recent.iter().any(|r| r.get(k) == Some(&uid))).collect();
        let mut l = self.layout.lock();
        if let ([k], true) = (&hits[..], l.boost_votes.len() == n) {
            let before = l.boost_uid();
            l.boost_votes[*k] += 1;
            if before != l.boost_uid() {
                ctx.log(format!("combat: buff uid = {}", l.boost_fields[*k]));
            }
            drop(l);
            self.layout.touch();
            l = self.layout.lock();
        }
        l.boost_uid()
    }

    /// A damage line of a fight action (GameActionFightEvent life_points_lost): (target, element). The element is the
    /// action's when fixed (`ELEMENT_OF`, which also votes for the LifePointsLost int holding it: the only one equal to it),
    /// else the learnt int's value.
    fn damage(&self, ctx: &Ctx, d: &DynamicMessage) -> Option<(i64, i64)> {
        let field = |m: &DynamicMessage, clear: &str| ctx.proto.field_named(m.descriptor().full_name(), clear);
        let lost_f = field(d, "life_points_lost").filter(|f| d.has_field_by_name(f))?;
        let lost = d.get_field_by_name(&lost_f)?.as_message()?.clone();
        let target_f = field(&lost, "target_id")?;
        let action = int(d, &field(d, "action_id")?);
        let fixed = ELEMENT_OF.iter().find(|x| x.0 == action).map(|x| x.1);
        let mut votes = self.element_votes.lock();
        if let Some(e) = fixed {
            let ints = scalar_ints(&lost);
            if let [(name, _)] = &ints.iter().filter(|(n, v)| *v == e && *n != target_f).collect::<Vec<_>>()[..] {
                *votes.entry(name.clone()).or_default() += 1;
            }
        }
        let learnt = votes.iter().max_by_key(|(_, v)| **v).map(|(n, _)| n.clone());
        let element = fixed.or_else(|| learnt.map(|n| int(&lost, &n)))?;
        Some((int(&lost, &target_f), element))
    }

    /// A message's fighters fit a single arrangement: one vote for it (saved).
    fn vote(&self, ctx: &Ctx, found: &[(i64, Fighter)]) {
        let ok = fitting(&ctx.data, &found.iter().filter(|(_, x)| x.gid.is_none()).map(|(_, x)| x.ints).collect::<Vec<_>>());
        if ok.len() != 1 {
            return;
        }
        let mut l = self.layout.lock();
        let before = l.best();
        l.votes[ok[0]] += 1;
        if before != l.best() && l.fields.len() == 3 {
            ctx.log(format!("combat: monster fields (gid, grade, level) = {:?}", PERMS[ok[0]].map(|i| l.fields[i].clone())));
        }
        drop(l);
        self.layout.touch();
    }

    /// The arrangement to read the fighters with: the learnt one, else the best fitting them all.
    fn arrangement(&self, data: &Data, f: &Fight) -> Option<usize> {
        self.layout
            .lock()
            .best()
            .or_else(|| fitting(data, &f.fighters.values().filter(|x| x.gid.is_none()).map(|x| x.ints).collect::<Vec<_>>()).first().copied())
    }

    /// The fight for the UI: {active, started, ended, fighters: [{actor, gid, name, grade, level, summoned, notes, tags}]}.
    /// Without any arrangement (nothing fits them all), a fighter's gid is its int that is a monster (grade / level 0).
    fn state(&self, data: &Data) -> Value {
        let f = self.fight.lock();
        let p = self.arrangement(data, &f);
        let read = |x: &Fighter| match (x.gid, p) {
            // a monster's summon: its other int is a grade when it is one of its grades, else a level
            (Some(g), _) => {
                let levels = data.monsters.get(&g).map(|m| m.levels.as_slice()).unwrap_or_default();
                match levels.get((x.ints[1] - 1).max(0) as usize).filter(|_| x.ints[1] >= 1) {
                    Some(l) => [g, x.ints[1], *l],
                    None => [g, 0, x.ints[1]],
                }
            }
            (None, Some(p)) => PERMS[p].map(|i| x.ints[i]),
            (None, None) => [x.ints.iter().copied().filter(|g| data.monsters.contains_key(g)).max().unwrap_or(x.ints[0]), 0, 0],
        };
        // doubles (Chafer Ronin...): summons of a monster that was placed at the start are left out
        let placed: BTreeSet<i64> = f.fighters.values().filter(|x| !x.summon()).map(|x| read(x)[0]).collect();
        let allies = ally_summons(&f.fighters);
        let list: Vec<Value> = f
            .fighters
            .iter()
            .filter_map(|(actor, x)| {
                let [gid, grade, level] = read(x);
                if allies.contains(actor) || (x.summon() && placed.contains(&gid)) {
                    return None;
                }
                let m = data.monsters.get(&gid);
                Some(json!({
                    "actor": actor, "gid": gid, "grade": grade, "level": level, "summoned": x.summon(), "dead": f.dead.contains(actor),
                    "since": f.since.get(actor), "played": f.played.get(actor), "hit": f.hit.get(actor),
                    "wave": x.wave.unwrap_or(0),
                    "name": m.and_then(|m| data.text(m.name_id)).map(str::to_owned).unwrap_or_else(|| format!("monstre #{gid}")),
                    "notes": m.and_then(|m| m.notes.as_ref()),
                    "tags": m.map(|m| m.tags.as_slice()).unwrap_or_default(),
                    "danger": m.map(|m| m.danger.as_slice()).unwrap_or_default(),
                }))
            })
            .collect();
        // PA / PM poisons on the player (none while the character is unknown: no login seen, no name found)
        let me = self.me.lock();
        let mine = me.ids.clone();
        let characters: Vec<Value> = me.ids.iter().map(|id| json!({"id": id, "name": me.names.get(id)})).collect();
        let pending = me.pending.clone();
        drop(me);
        let poisons: Vec<Value> = f
            .poisons
            .values()
            .filter(|p| mine.contains(&p.target))
            .filter_map(|p| {
                POISONS.iter().find(|x| x.0 == p.action).map(|&(a, kind, element)| json!({"action": a, "kind": kind, "element": element, "target": p.target}))
            })
            .collect();
        // the player's confusion; a character not known as theirs (no login seen, no name typed): the only character of the
        // fight that has one (solo fights)
        let others: Vec<&i64> = f.confusion.iter().filter(|(id, _)| **id > 0).map(|(_, v)| v).collect();
        let confusion = mine.iter().rev().find_map(|id| f.confusion.get(id)).or(match others[..] {
            [v] => Some(v),
            _ => None,
        });
        json!({"seen": f.seen, "active": f.seen && f.ended.is_none(), "started": f.started, "ended": f.ended, "fighters": list,
               "poisons": poisons, "confusion": confusion, "round": f.round, "me": mine.last(), "mine": mine,
               "characters": characters, "pending": pending})
    }
}

impl Combat {
    /// The player's characters changed (user action): saved now, the windows told.
    fn changed_me(&self, ctx: &Ctx) -> Result<Value> {
        self.me.flush();
        let s = self.state(&ctx.data);
        ctx.emit("combat", "fight", s.clone());
        Ok(s)
    }
}

impl Module for Combat {
    fn id(&self) -> &'static str {
        "combat"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[JOIN, JOIN_RUNNING, SHOW, REFRESH, SYNC, ACTION, END, BOOST_ADD, BOOST_REMOVE, SELECT, FIRST_SELECT, TURN, TIMELINE, STATS, ROUND])
    }

    fn wants(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) || self.turn.load(Ordering::Relaxed)
    }

    fn init(&self, ctx: &Ctx) {
        let saved = self.me.open(ctx, "combat", "me.json", |me| json!({"id": me.ids.last(), "ids": me.ids, "names": me.names, "pending": me.pending}));
        let saved = saved.unwrap_or_default();
        let mut ids: Vec<i64> = saved["ids"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
        // older file: the last character only
        if ids.is_empty() {
            ids.extend(saved["id"].as_i64());
        }
        let names = serde_json::from_value(saved["names"].clone()).unwrap_or_default();
        let pending = serde_json::from_value(saved["pending"].clone()).unwrap_or_default();
        *self.me.lock() = Me { ids, names, pending };
        let saved = self.layout.open(
            ctx,
            "combat",
            "layout.json",
            |l| json!({"fields": l.fields, "votes": l.votes, "boost_fields": l.boost_fields, "boost_votes": l.boost_votes}),
        );
        let mut l = self.layout.lock();
        if let Some(b) = self.boosts(ctx) {
            l.boost_fields = b.ints.clone();
            l.boost_votes = vec![0; b.ints.len()];
            // the votes of another build (other field names) are dropped
            if let Some(saved) = saved.as_ref().filter(|s| s["boost_fields"] == json!(b.ints)) {
                if let Ok(v) = serde_json::from_value::<Vec<u32>>(saved["boost_votes"].clone()) {
                    if v.len() == b.ints.len() {
                        l.boost_votes = v;
                    }
                }
            }
        }
        let Some(b) = self.bindings(ctx) else { return };
        l.fields = b.monster.clone();
        // the votes of another build (other field names) are dropped
        if let Some(saved) = saved {
            if saved["fields"] == json!(b.monster) {
                if let Ok(v) = serde_json::from_value::<[u32; 6]>(saved["votes"].clone()) {
                    l.votes = v;
                }
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let t = m.raw.t;
        // the character the player logs in with (client request): its id is the player's fighter id
        if m.name == SELECT || m.name == FIRST_SELECT {
            let id = self.boosts(ctx).zip(m.decoded.as_ref()).and_then(|(b, d)| b.character.iter().find(|f| d.has_field_by_name(f)).map(|f| int(d, f)));
            if let Some(id) = id.filter(|id| *id != 0) {
                self.me.update(|me| me.add(id));
                ctx.emit("combat", "fight", self.state(&ctx.data));
            }
            return;
        }
        // a new fight: nobody is dead yet (its timeline follows)
        if m.name == JOIN || m.name == JOIN_RUNNING {
            self.dead.lock().clear();
        }
        // the turn order lists every fighter (the whole list: replaces the dead ones)
        if m.name == TIMELINE {
            let b = self.turn_bind.get(ctx, "turn");
            if let (Some(id), Some(dead), Some(d)) = (b.and_then(|b| b.timeline_id.as_deref()), b.and_then(|b| b.timeline_dead.as_deref()), &m.decoded) {
                let mut out = BTreeSet::new();
                timeline_dead(id, dead, d, &mut out);
                *self.dead.lock() = out;
            }
            return;
        }
        // the player's turn starts (alive): {id, focused} (whether Dofus has the focus, the UI may ring only when it has not)
        if m.name == TURN {
            let id = self.turn_bind.get(ctx, "turn").zip(m.decoded.as_ref()).map(|(b, d)| int(d, &b.fighter));
            let mine = id.is_some_and(|id| self.me.lock().ids.contains(&id) && !self.dead.lock().contains(&id));
            if self.turn.load(Ordering::Relaxed) && mine {
                ctx.emit("combat", "turn", json!({"id": id, "focused": crate::sys::game_focused()}));
            }
            // whose turn it was this round (boss helpers: a check at the start of the boss's turn); the damage it took before
            // is forgotten (Kimbo: hit again every turn)
            if let (Some(id), true) = (id, self.enabled.load(Ordering::Relaxed)) {
                let mut f = self.fight.lock();
                let round = f.round;
                let changed = (f.played.insert(id, round) != Some(round)) | f.hit.remove(&id).is_some();
                drop(f);
                if changed {
                    ctx.emit("combat", "fight", self.state(&ctx.data));
                }
            }
            return;
        }
        // the characters named by the user, found among the fighters (placement): before their first turn
        if matches!(m.name.as_str(), JOIN | JOIN_RUNNING | SHOW | REFRESH | SYNC) {
            if let (Some(b), Some(d)) = (self.bindings(ctx), &m.decoded) {
                if self.name_fighters(ctx, &b.actor_id, d) {
                    ctx.emit("combat", "fight", self.state(&ctx.data));
                }
            }
        }
        // only the turns are wanted (fight following off)
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let damage = m.decoded.as_ref().filter(|_| m.name == ACTION).and_then(|d| self.damage(ctx, d));
        let changed = {
            let mut f = self.fight.lock();
            match m.name.as_str() {
                JOIN | JOIN_RUNNING => {
                    *f = Fight { seen: true, started: t, ..Default::default() };
                    // reconnection: the buffs already running come with it
                    if let (Some(b), Some(d)) = (self.boosts(ctx), &m.decoded) {
                        boosts_added(b, d, &mut f);
                    }
                    true
                }
                BOOST_ADD => {
                    let (Some(b), Some(d)) = (self.boosts(ctx), &m.decoded) else { return };
                    boosts_added(b, d, &mut f)
                }
                BOOST_REMOVE => {
                    let (Some(b), Some(d)) = (self.boosts(ctx), &m.decoded) else { return };
                    let uid = int(d, &b.remove_uid);
                    let k = self.boost_uid(ctx, &f, uid);
                    let before = f.poisons.len();
                    // until the uid int is learnt, a poison holding the uid in any int ends
                    f.poisons.retain(|ints, _| match k {
                        Some(k) => ints.get(k) != Some(&uid),
                        None => !ints.contains(&uid),
                    });
                    f.poisons.len() != before
                }
                ROUND => {
                    let Some(d) = &m.decoded else { return };
                    let Some(n) = ctx.proto.field_named(d.descriptor().full_name(), "round_number") else { return };
                    f.round = int(d, &n);
                    true
                }
                STATS => {
                    let Some((id, v)) = m.decoded.as_ref().and_then(|d| characteristic(ctx, d, CONFUSION)) else { return };
                    f.confusion.insert(id, v) != Some(v)
                }
                END => {
                    let open = f.seen && f.ended.is_none();
                    if open {
                        f.ended = Some(t);
                    }
                    // the fight's buffs end with it: no poison warning left once it is over
                    let had = !f.poisons.is_empty() || !f.confusion.is_empty();
                    f.poisons.clear();
                    f.confusion.clear();
                    open || had
                }
                // a damage line: the target's last element (neutral, or none: changes nothing, Kimbo)
                ACTION if damage.is_some() => {
                    let Some((target, element)) = damage else { return };
                    (1..=4).contains(&element) && f.hit.insert(target, element) != Some(element)
                }
                _ => {
                    let (Some(b), Some(d)) = (self.bindings(ctx), &m.decoded) else { return };
                    // a death action: its ids (target, and an unnamed one) that are fighters of this fight
                    let died: Vec<i64> = match (&b.death, m.name == ACTION) {
                        (Some(death), true) if d.has_field_by_name(death) => match d.get_field_by_name(death).as_deref() {
                            Some(PbValue::Message(x)) => x.fields().filter_map(|(_, v)| as_i64(v)).filter(|id| f.fighters.contains_key(id)).collect(),
                            _ => vec![],
                        },
                        _ => vec![],
                    };
                    if !died.is_empty() {
                        let before = f.dead.len();
                        f.dead.extend(died);
                        f.dead.len() != before
                    } else {
                        let mut found = vec![];
                        let wave = {
                            let mut w = self.wave.lock();
                            if w.is_none() && m.name != ACTION {
                                *w = wave_field(ctx, d);
                            }
                            w.clone()
                        };
                        fighters(b, wave.as_deref(), d, &mut found);
                        if m.name == ACTION {
                            summoned_monsters(ctx, b, d, &mut found);
                        }
                        if found.is_empty() {
                            return;
                        }
                        self.vote(ctx, &found);
                        // fighters without a fight start (app opened mid-fight) or after its end: a new fight
                        if !f.seen || f.ended.is_some() {
                            *f = Fight { seen: true, started: t, ..Default::default() };
                        }
                        // the synchronisation lists the fighters still in: the monsters it leaves out are dead (kept, flagged)
                        let mut dead = f.dead.clone();
                        if m.name == SYNC {
                            let listed: BTreeSet<i64> = found.iter().map(|(a, _)| *a).collect();
                            dead.extend(f.fighters.keys().filter(|a| !listed.contains(a)));
                        }
                        // fighters the game says are dead (killed before the app saw it: started mid-fight)
                        dead.extend(found.iter().filter(|(_, x)| x.dead == Some(true)).map(|(a, _)| *a));
                        let mut next = f.fighters.clone();
                        let round = f.round.max(1);
                        for (actor, mut x) in found {
                            f.since.entry(actor).or_insert(round);
                            // first seen in a fight action = appeared during the fight (summon, double)
                            x.spawned = next.get(&actor).map_or(m.name == ACTION, |old| old.spawned);
                            // a message without the summon details (a wave's refresh...) keeps those seen before
                            if let Some(old) = next.get(&actor) {
                                x.summoned |= old.summoned;
                                x.wave = x.wave.or(old.wave);
                                if x.summoner == 0 {
                                    x.summoner = old.summoner;
                                }
                            }
                            next.insert(actor, x);
                        }
                        let changed = next != f.fighters || dead != f.dead;
                        (f.fighters, f.dead) = (next, dead);
                        changed
                    }
                }
            }
        };
        if changed {
            ctx.emit("combat", "fight", self.state(&ctx.data));
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "state" => Ok(self.state(&ctx.data)),
            // the feature switch (Automation tab): off = the fight messages are not even decoded
            "set_enabled" => {
                let on = args["on"].as_bool().unwrap_or(true);
                self.enabled.store(on, Ordering::Relaxed);
                if !on {
                    *self.fight.lock() = Fight::default();
                }
                Ok(json!({"enabled": on}))
            }
            // Automation « À votre tour »: announce the player's turns
            "set_turn" => {
                let on = args["on"].as_bool().unwrap_or(false);
                self.turn.store(on, Ordering::Relaxed);
                Ok(json!({"turn": on}))
            }
            // Automation « Vos personnages »: a name looked for among the fighters (no login seen); known already = nothing
            "add_name" => {
                let name = arg_str(&args, "name")?.trim().to_owned();
                self.me.update(|me| {
                    let same = |x: &String| x.to_lowercase() == name.to_lowercase();
                    if !name.is_empty() && !me.pending.iter().any(same) && !me.names.values().any(same) {
                        me.pending.push(name.clone());
                    }
                });
                self.changed_me(ctx)
            }
            "remove_name" => {
                let name = arg_str(&args, "name")?.to_owned();
                self.me.update(|me| me.pending.retain(|x| *x != name));
                self.changed_me(ctx)
            }
            // a character no longer the player's (a login with it adds it back)
            "remove" => {
                let id = arg_i64(&args, "id")?;
                self.me.update(|me| {
                    me.ids.retain(|x| *x != id);
                    me.names.remove(&id);
                });
                self.changed_me(ctx)
            }
            "clear" => {
                *self.fight.lock() = Fight::default();
                let s = self.state(&ctx.data);
                ctx.emit("combat", "fight", s.clone());
                Ok(s)
            }
            _ => Err(unknown("combat", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_core::RawMessage;
    use loutil_proto::{FieldDescriptor, MessageDescriptor};
    use std::sync::Arc;

    /// A real fight recorded with pipeline/sniff.py (work/capture_poison.jsonl: local only, it holds the players' ids), replayed
    /// through the hub: monsters found, doubles / deaths applied, buffs read without error. Skipped without the file.
    #[test]
    fn replays_a_recorded_fight() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_poison.jsonl");
        if !rec.exists() {
            return;
        }
        let (hub, proto) = testkit::hub("loutil-combat-replay").unwrap();
        let logs: Arc<Mutex<Vec<String>>> = Default::default();
        let sink = logs.clone();
        let hub = crate::Hub::new(
            hub.ctx.data.clone(),
            proto,
            testkit::temp_dir("loutil-combat-replay2"),
            Arc::new(|_, _| {}),
            Arc::new(move |l| sink.lock().push(l)),
        );
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        for line in std::fs::read_to_string(&rec).unwrap().lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
            if h.len() / 2 < len as usize {
                continue; // truncated record (> 4 KB)
            }
            let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
            hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload: hex(h) });
        }
        let s = hub.call("combat", "state", Value::Null).unwrap();
        let names: Vec<String> = s["fighters"].as_array().unwrap().iter().map(|f| format!("{} dead={}", f["name"], f["dead"])).collect();
        eprintln!("recorded fight: {names:?} poisons={}", s["poisons"]);
        assert!(!names.is_empty(), "the recorded fight's monsters are found");
        assert!(names.iter().all(|n| !n.contains("monstre #")), "every monster is known");
        let learnt: Vec<String> = logs.lock().iter().filter(|l| l.contains("buff uid")).cloned().collect();
        eprintln!("{learnt:?}");
        assert_eq!(learnt.len(), 1, "the buff uid int is learnt from the removals, once");
    }

    /// The same recorded fight with the player's character known (Hinae, from its card): `combat:turn` is sent for its turn
    /// starts only (the recording holds one; the other players' and the monsters' are left out). Monster -3 is passed off as
    /// a character too: the game still starts its turns after its death (twice in the recording), which must not ring.
    /// Skipped without the file.
    #[test]
    fn announces_the_players_turns() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_poison.jsonl");
        if !rec.exists() {
            return;
        }
        let (hub, proto) = testkit::hub("loutil-combat-turn").unwrap();
        let dir = testkit::temp_dir("loutil-combat-turn2");
        std::fs::create_dir_all(dir.join("combat")).unwrap();
        std::fs::write(dir.join("combat/me.json"), r#"{"id": 40895840551, "ids": [-3, 40895840551]}"#).unwrap();
        let turns: Arc<Mutex<Vec<Value>>> = Default::default();
        let sink = turns.clone();
        let emit = Arc::new(move |e: &str, v: Value| {
            if e == "combat:turn" {
                sink.lock().push(v);
            }
        });
        let hub = crate::Hub::new(hub.ctx.data.clone(), proto, dir, emit, Arc::new(|_| {}));
        hub.call("combat", "set_turn", json!({"on": true})).unwrap();
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        for line in std::fs::read_to_string(&rec).unwrap().lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
            if h.len() / 2 < len as usize {
                continue;
            }
            hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir: "S->C", payload: hex(h) });
        }
        let ids: Vec<i64> = turns.lock().iter().filter_map(|v| v["id"].as_i64()).collect();
        assert_eq!(ids, vec![40895840551]);
        // the character known from its login gets its name from the fight (its card: Hinae)
        let s = hub.call("combat", "state", Value::Null).unwrap();
        assert_eq!(s["characters"][1], json!({"id": 40895840551i64, "name": "Hinae"}));
    }

    /// Two Comte Harebourg fights (work/capture_harebourg.jsonl, local only): the player's confusion goes from 1 quarter turn
    /// (Comtoise at the turn start, full life) to 2 (a melee hit: Pendultimatum), and is gone once each fight ends. Read with
    /// the character known, then with another one known instead (the only character of the fight is taken). Skipped without
    /// the file. The first fight's end also says the round (1), when each monster appeared and played, and the element of
    /// the last damage taken: on Crocodaille, a fire hit (action 99: teaches the element int) then "pire élément" (2832,
    /// element read from that int: earth). Dragueuse's summon is listed.
    #[test]
    fn follows_the_players_confusion() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_harebourg.jsonl");
        if !rec.exists() {
            return;
        }
        for (k, me) in [r#"{"ids": [19676332327]}"#, r#"{"ids": [97875525927]}"#].into_iter().enumerate() {
            let (hub, proto) = testkit::hub("loutil-combat-confusion").unwrap();
            let dir = testkit::temp_dir(&format!("loutil-combat-confusion{k}"));
            std::fs::create_dir_all(dir.join("combat")).unwrap();
            std::fs::write(dir.join("combat/me.json"), me).unwrap();
            let seen: Arc<Mutex<Vec<Value>>> = Default::default();
            let ended: Arc<Mutex<Option<Value>>> = Default::default();
            let (sink, end_sink) = (seen.clone(), ended.clone());
            let emit = Arc::new(move |e: &str, v: Value| {
                if e == "combat:fight" {
                    let mut end = end_sink.lock();
                    if end.is_none() && !v["ended"].is_null() {
                        *end = Some(v.clone());
                    }
                    let mut s = sink.lock();
                    if s.last() != Some(&v["confusion"]) {
                        s.push(v["confusion"].clone());
                    }
                }
            });
            let hub = crate::Hub::new(hub.ctx.data.clone(), proto, dir, emit, Arc::new(|_| {}));
            let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
            for line in std::fs::read_to_string(&rec).unwrap().lines() {
                let r: Value = serde_json::from_str(line).unwrap();
                let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
                if h.len() / 2 < len as usize {
                    continue;
                }
                let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
                hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload: hex(h) });
            }
            let end = ended.lock().clone().unwrap();
            let fighter = |actor: i64| end["fighters"].as_array().unwrap().iter().find(|x| x["actor"] == actor).unwrap().clone();
            assert_eq!(end["round"], 1);
            assert_eq!((fighter(-2)["hit"].clone(), fighter(-2)["since"].clone()), (json!(1), json!(1)), "earth hit");
            assert_eq!((fighter(-7)["played"].clone(), fighter(-5)["played"].clone()), (json!(1), Value::Null));
            // Dragueuse's summon (summons by context: gid + level, no MonsterFighter)
            let summon = fighter(-8);
            assert_eq!((summon["gid"].clone(), summon["summoned"].clone(), summon["since"].clone()), (json!(5815), json!(true), json!(1)));
            // the first two fights (later ones may be appended to the recording)
            assert_eq!(seen.lock()[..7], [Value::Null, json!(1), json!(2), Value::Null, json!(1), json!(2), Value::Null], "{me}");
        }
    }

    /// A PA poison put on the player, then its end (FightBoostRemoveEvent with the buff uid); one on a monster is ignored.
    #[test]
    fn follows_poisons_on_the_player() {
        let Some((hub, proto)) = testkit::hub("loutil-combat-poison") else { return };
        let Ok(b) = serde_json::from_value::<BoostBindings>(hub.ctx.data.bindings["boost"].clone()) else {
            eprintln!("skipped: pack without the boost bindings");
            return;
        };
        let (Some(add), Some(rem), Some(sel)) = (proto.obf_for_clear(BOOST_ADD), proto.obf_for_clear(BOOST_REMOVE), proto.obf_for_clear(SELECT)) else {
            eprintln!("skipped: pack without the buff message names");
            return;
        };
        let (add_md, rem_md, sel_md) = (proto.message(&add).unwrap(), proto.message(&rem).unwrap(), proto.message(&sel).unwrap());
        let to_info = path(&add_md, &b.action, &mut vec![]).unwrap();
        let boost = |uid: i64, target: i64, action: i64| {
            let mut m = DynamicMessage::new(add_md.clone());
            set(&mut m, &to_info, &b.action, PbValue::I32(action as i32));
            let mut info_path = to_info.clone();
            // the uid in one of the effect's ints (which one is learnt from the removals)
            info_path.push(path(to_info.last().unwrap().kind().as_message().unwrap(), &b.ints[1], &mut vec![]).unwrap().into_iter().next().unwrap());
            set(&mut m, &info_path, &b.ints[1], PbValue::I32(uid as i32));
            set(&mut m, &info_path, &b.target, PbValue::I64(target));
            m
        };
        let message = |name: &str, decoded: DynamicMessage| testkit::msg(name, Some(decoded), 1.0);
        let c = Combat::new();
        c.init(&hub.ctx);
        // no character known yet: no poison shown, whoever it is on
        c.on_message(&message(BOOST_ADD, boost(6, 41, 1135)), &hub.ctx);
        assert_eq!(c.call("state", Value::Null, &hub.ctx).unwrap()["poisons"], json!([]));
        let mut login = DynamicMessage::new(sel_md.clone());
        login.set_field_by_name(b.character.iter().find(|f| login.descriptor().get_field_by_name(f).is_some()).unwrap(), PbValue::I64(42));
        c.on_message(&message(SELECT, login), &hub.ctx);
        c.on_message(&message(BOOST_ADD, boost(7, 42, 1135)), &hub.ctx);
        c.on_message(&message(BOOST_ADD, boost(8, -3, 1136)), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["me"], json!(42));
        assert_eq!(s["poisons"], json!([{"action": 1135, "kind": "PA", "element": "Terre", "target": 42}]), "only the player's poison");
        // another of the player's characters logs in: both are the player's (multi-account, or the next session)
        let mut login2 = DynamicMessage::new(sel_md);
        login2.set_field_by_name(b.character.iter().find(|f| login2.descriptor().get_field_by_name(f).is_some()).unwrap(), PbValue::I64(43));
        c.on_message(&message(SELECT, login2), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["me"].clone(), s["mine"].clone()), (json!(43), json!([42, 43])));
        assert_eq!(s["poisons"].as_array().unwrap().len(), 1, "the first character's poison is still the player's");

        let mut end = DynamicMessage::new(rem_md);
        end.set_field_by_name(&b.remove_uid, PbValue::I32(7));
        c.on_message(&message(BOOST_REMOVE, end), &hub.ctx);
        assert_eq!(c.call("state", Value::Null, &hub.ctx).unwrap()["poisons"], json!([]));
        assert_eq!(c.layout.lock().boost_uid(), Some(1), "the removed uid was in the effect's second int only: learnt");

        // a poison still running when the fight ends goes with it
        c.on_message(&message(BOOST_ADD, boost(9, 42, 1135)), &hub.ctx);
        let end_md = proto.message(&proto.obf_for_clear(END).unwrap()).unwrap();
        c.on_message(&message(END, DynamicMessage::new(end_md)), &hub.ctx);
        assert_eq!(c.call("state", Value::Null, &hub.ctx).unwrap()["poisons"], json!([]));
    }

    /// Field path from `md` down to a message that has the field `target` (depth first, each message type once).
    fn path(md: &MessageDescriptor, target: &str, seen: &mut Vec<String>) -> Option<Vec<FieldDescriptor>> {
        if md.get_field_by_name(target).is_some() {
            return Some(vec![]);
        }
        if seen.contains(&md.full_name().to_string()) {
            return None;
        }
        seen.push(md.full_name().to_string());
        md.fields().find_map(|f| {
            let sub = f.kind().as_message()?.clone();
            let mut p = path(&sub, target, seen)?;
            p.insert(0, f);
            Some(p)
        })
    }

    /// Set `field` = `v` on the message at the end of `p` inside `m`, creating the messages on the way.
    fn set(m: &mut DynamicMessage, p: &[FieldDescriptor], field: &str, v: PbValue) {
        match p.split_first() {
            None => m.set_field_by_name(field, v),
            Some((f, rest)) => {
                let mut sub = match m.get_field(f).into_owned() {
                    PbValue::Message(x) if m.has_field(f) => x,
                    _ => DynamicMessage::new(f.kind().as_message().unwrap().clone()),
                };
                set(&mut sub, rest, field, v);
                m.set_field(f, PbValue::Message(sub));
            }
        }
    }

    /// A real fight (Pissenlit Diabolique 79, Minotoror 121, Wobot Kiafin 3462, Meupette 1046) with every monster scaled to
    /// level 200: no grade level matches, and taking the largest monster int as the gid read 79 and 121 as Dragodinde rousse
    /// sauvage (200 = their level). Only one arrangement fits the four; it is learnt and then reads a fight of the small ids alone.
    #[test]
    fn learns_the_monster_fields_from_a_scaled_fight() {
        let Some((hub, _)) = testkit::hub("loutil-combat-layout") else { return };
        let data = hub.ctx.data.clone();
        if [79, 121, 3462, 1046, 200].iter().any(|g| !data.monsters.contains_key(g)) {
            eprintln!("skipped: pack without these monsters");
            return;
        }
        // field order of this imaginary build: (grade, level, gid)
        let fighter =
            |gid: i64, grade: i64| Fighter { ints: [grade, 200, gid], summoned: false, summoner: 0, spawned: false, dead: None, wave: None, gid: None };
        let c = Combat::new();
        let found: Vec<(i64, Fighter)> =
            [(79, 3), (121, 2), (3462, 5), (1046, 1)].iter().enumerate().map(|(i, &(g, gr))| (-(i as i64) - 1, fighter(g, gr))).collect();
        c.vote(&hub.ctx, &found);
        *c.fight.lock() = Fight { seen: true, fighters: found.into_iter().collect(), ..Default::default() };
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        let gids: Vec<i64> = s["fighters"].as_array().unwrap().iter().map(|f| f["gid"].as_i64().unwrap()).collect();
        assert_eq!(gids, vec![1046, 3462, 121, 79], "actors -4..-1");
        assert!(s["fighters"].as_array().unwrap().iter().all(|f| f["level"] == json!(200)));

        // alone, 79 and 121 fit two arrangements (200 is a monster too): the learnt one decides
        let small = vec![(-1, fighter(79, 3)), (-2, fighter(121, 2))];
        c.vote(&hub.ctx, &small);
        *c.fight.lock() = Fight { seen: true, fighters: small.into_iter().collect(), ..Default::default() };
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["fighters"][0]["gid"], json!(121));
        assert_eq!(s["fighters"][1]["gid"], json!(79));
    }

    /// No login seen: the character named in the Automation tab is found among the fighters shown at placement.
    #[test]
    fn finds_the_player_by_name() {
        let Some((hub, proto)) = testkit::hub("loutil-combat-name") else { return };
        let b: Bindings = serde_json::from_value(hub.ctx.data.bindings["fight"].clone()).unwrap();
        let Some(named_md) = proto.obf_for_clear(NAMED[0]).and_then(|n| proto.message(&n)) else {
            eprintln!("skipped: pack without NamedFighterInformation");
            return;
        };
        let show_md = proto.message(&proto.obf_for_clear(SHOW).unwrap()).unwrap();
        let info = show_md.fields().find(|f| f.kind().as_message().is_some_and(|k| k.get_field_by_name(&b.actor_id).is_some())).unwrap();
        let actor_md = info.kind().as_message().unwrap().clone();
        let strings: Vec<String> = named_md.fields().filter(|f| f.kind() == loutil_proto::Kind::String).map(|f| f.name().to_string()).collect();
        let to_named = path(&actor_md, &strings[0], &mut vec![]).unwrap();
        let shown = |id: i64, name: &str| {
            let mut a = DynamicMessage::new(actor_md.clone());
            a.set_field_by_name(&b.actor_id, PbValue::I64(id));
            // the name in the last string field (the other one may come first)
            set(&mut a, &to_named, strings.last().unwrap(), PbValue::String(name.into()));
            let mut show = DynamicMessage::new(show_md.clone());
            show.set_field(&info, PbValue::Message(a));
            testkit::msg(SHOW, Some(show), 1.0)
        };
        let c = Combat::new();
        c.init(&hub.ctx);
        c.call("add_name", json!({"name": " Hinae "}), &hub.ctx).unwrap();
        c.call("add_name", json!({"name": "hinae"}), &hub.ctx).unwrap();
        assert_eq!(c.call("state", Value::Null, &hub.ctx).unwrap()["pending"], json!(["Hinae"]), "trimmed, once");
        c.on_message(&shown(77, "Someone"), &hub.ctx);
        assert_eq!(c.call("state", Value::Null, &hub.ctx).unwrap()["mine"], json!([]));
        c.on_message(&shown(4242, "hinae"), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["characters"], json!([{"id": 4242, "name": "hinae"}]), "case ignored");
        assert_eq!(s["pending"], json!([]), "found: no longer looked for");
        // a character seen at login gets its name from a fight
        c.me.update(|me| me.add(55));
        c.on_message(&shown(55, "Alt"), &hub.ctx);
        assert_eq!(c.call("state", Value::Null, &hub.ctx).unwrap()["characters"][1], json!({"id": 55, "name": "Alt"}));
        let s = c.call("remove", json!({"id": 4242}), &hub.ctx).unwrap();
        assert_eq!(s["mine"], json!([55]));
    }

    /// A synthetic fight: placement shows a monster, the turn synchronisation lists it plus a summon, then the fight ends.
    #[test]
    fn ally_summons_are_not_enemies() {
        let f = |summoner: i64| Fighter { ints: [1, 2, 3], summoned: summoner != 0, summoner, spawned: summoner != 0, dead: None, wave: None, gid: None };
        // -1 a monster, -2 its summon; -3 a turret of the player 77, -4 summoned by that turret
        let fighters = BTreeMap::from([(-1, f(0)), (-2, f(-1)), (-3, f(77)), (-4, f(-3))]);
        assert_eq!(ally_summons(&fighters), BTreeSet::from([-3, -4]));
    }

    #[test]
    fn follows_the_fight_monsters() {
        let Some((hub, proto)) = testkit::hub("loutil-combat") else { return };
        let data = hub.ctx.data.clone();
        if data.monsters.is_empty() {
            eprintln!("skipped: pack without monsters.json");
            return;
        }
        let b: Bindings = serde_json::from_value(data.bindings["fight"].clone()).unwrap();
        // a noted monster with several grades, and its grade 2 (ints given in a scrambled order)
        let (&gid, mon) = data.monsters.iter().filter(|(_, m)| m.notes.is_some() && m.levels.len() >= 2).min_by_key(|(g, _)| **g).unwrap();
        let level = mon.levels[1];
        // another monster, the summon
        let (&gid2, mon2) = data.monsters.iter().filter(|(g, m)| **g != gid && m.levels.len() >= 2).min_by_key(|(g, _)| **g).unwrap();
        let level2 = mon2.levels[1];

        let sync_md = proto.message(&proto.obf_for_clear(SYNC).unwrap()).unwrap();
        let list_field = sync_md.fields().next().unwrap();
        let actor_md = list_field.kind().as_message().unwrap().clone();
        assert!(actor_md.get_field_by_name(&b.actor_id).is_some(), "fighters are ActorPositionInformation");
        let to_monster = path(&actor_md, &b.monster[0], &mut vec![]).unwrap();
        let to_summoned = path(&actor_md, &b.summoned, &mut vec![]).unwrap();
        let actor = |id: i64, g: i64, summoned: bool| {
            let mut a = DynamicMessage::new(actor_md.clone());
            a.set_field_by_name(&b.actor_id, PbValue::I64(id));
            for (f, v) in b.monster.iter().zip([if g == gid { level } else { level2 }, g, 2]) {
                set(&mut a, &to_monster, f, PbValue::I32(v as i32));
            }
            set(&mut a, &to_summoned, &b.summoned, PbValue::Bool(summoned));
            a
        };
        let show_md = proto.message(&proto.obf_for_clear(SHOW).unwrap()).unwrap();
        let mut show = DynamicMessage::new(show_md.clone());
        let info = show_md.fields().find(|f| f.kind().as_message().is_some_and(|k| k.full_name() == actor_md.full_name())).unwrap();
        show.set_field(&info, PbValue::Message(actor(-1, gid, false)));
        let proto_action = proto.message(&proto.obf_for_clear(ACTION).unwrap()).unwrap();
        let mut sync = DynamicMessage::new(sync_md);
        // -2: a summon (another monster); -3: a double of -1 (same monster, summoned), left out of the list
        sync.set_field(
            &list_field,
            PbValue::List(vec![PbValue::Message(actor(-1, gid, false)), PbValue::Message(actor(-2, gid2, true)), PbValue::Message(actor(-3, gid, true))]),
        );
        let mut sync_one = DynamicMessage::new(sync.descriptor());
        sync_one.set_field(&list_field, PbValue::List(vec![PbValue::Message(actor(-2, gid2, true)), PbValue::Message(actor(-3, gid, true))]));
        let end = DynamicMessage::new(proto.message(&proto.obf_for_clear(END).unwrap()).unwrap());

        let message = |name: &str, decoded: DynamicMessage, t: f64| testkit::msg(name, Some(decoded), t);
        let c = Combat::new();
        c.on_message(&message(SHOW, show, 1.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["active"], json!(true), "a fighter starts a fight even without its join message");
        assert_eq!(s["fighters"].as_array().unwrap().len(), 1);
        let f = &s["fighters"][0];
        assert_eq!((f["gid"].as_i64(), f["grade"].as_i64(), f["level"].as_i64()), (Some(gid), Some(2), Some(level)));
        assert!(f["notes"].is_object(), "the monster's notes come along");

        c.on_message(&message(SYNC, sync, 2.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        let actors: Vec<i64> = s["fighters"].as_array().unwrap().iter().map(|f| f["actor"].as_i64().unwrap()).collect();
        assert_eq!(actors, vec![-2, -1], "the double -3 is left out");
        assert_eq!(s["fighters"][0]["summoned"], json!(true), "actor -2 is the summon");

        // the summon dies (death action: its id in one of the two ints), then a synchronisation leaves the other one out
        let action_md = proto_action.clone();
        let death_field = action_md.get_field_by_name(b.death.as_deref().unwrap()).unwrap();
        let death_md = death_field.kind().as_message().unwrap().clone();
        let mut death = DynamicMessage::new(death_md.clone());
        for (f, v) in death_md.fields().zip([-2i64, 7]) {
            death.set_field(&f, PbValue::I64(v));
        }
        let mut action = DynamicMessage::new(action_md);
        action.set_field(&death_field, PbValue::Message(death));
        c.on_message(&message(ACTION, action, 2.5), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["fighters"][0]["dead"].clone(), s["fighters"][1]["dead"].clone()), (json!(true), json!(false)), "actor -2 died");
        c.on_message(&message(SYNC, sync_one, 2.6), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["fighters"].as_array().unwrap().len(), 2, "dead monsters stay listed (flagged)");
        assert_eq!(s["fighters"][1]["dead"], json!(true), "actor -1, left out of the synchronisation = dead");

        c.on_message(&message(END, end, 3.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["active"].clone(), s["ended"].clone()), (json!(false), json!(3.0)), "the last fight stays shown, ended");

        // the app opened mid-fight: the synchronisation also lists the monsters killed before (spawn information, not alive)
        let alive_f = b.alive.as_deref().unwrap();
        let to_alive = path(&actor_md, alive_f, &mut vec![]).unwrap();
        let spawned = |id: i64, alive: bool| {
            let mut a = actor(id, gid, false);
            set(&mut a, &to_alive, alive_f, PbValue::Bool(alive));
            PbValue::Message(a)
        };
        let mut late = DynamicMessage::new(proto.message(&proto.obf_for_clear(SYNC).unwrap()).unwrap());
        late.set_field(&list_field, PbValue::List(vec![spawned(-5, false), spawned(-6, true)]));
        c.on_message(&message(SYNC, late, 4.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        let dead: Vec<(i64, bool)> = s["fighters"].as_array().unwrap().iter().map(|f| (f["actor"].as_i64().unwrap(), f["dead"] == json!(true))).collect();
        assert_eq!(dead, vec![(-6, false), (-5, true)], "killed before the app saw it: dead");

        // a multi-wave fight: each fighter tells its wave; a message without it (a refresh) keeps the one seen before
        let Some(wave_f) = named_field(&proto, &actor_md, "wave", &mut vec![]) else {
            eprintln!("skipped: pack without the wave field name");
            return;
        };
        let to_wave = path(&actor_md, &wave_f, &mut vec![]).unwrap();
        let in_wave = |id: i64, w: i64| {
            let mut a = actor(id, gid, false);
            set(&mut a, &to_wave, &wave_f, PbValue::I32(w as i32));
            PbValue::Message(a)
        };
        let mut waves = DynamicMessage::new(proto.message(&proto.obf_for_clear(SYNC).unwrap()).unwrap());
        waves.set_field(&list_field, PbValue::List(vec![in_wave(-5, 1), in_wave(-6, 2)]));
        c.on_message(&message(SYNC, waves, 5.0), &hub.ctx);
        let wave_of = |s: &Value| -> Vec<(i64, i64)> {
            s["fighters"].as_array().unwrap().iter().map(|f| (f["actor"].as_i64().unwrap(), f["wave"].as_i64().unwrap())).collect()
        };
        assert_eq!(wave_of(&c.call("state", Value::Null, &hub.ctx).unwrap()), vec![(-6, 2), (-5, 1)]);
    }

    /// Obfuscated name of the first field whose clear name is `clear` in `md` or the messages under it.
    fn named_field(proto: &loutil_proto::Proto, md: &MessageDescriptor, clear: &str, seen: &mut Vec<String>) -> Option<String> {
        if let Some(f) = proto.field_named(md.full_name(), clear) {
            return Some(f);
        }
        if seen.contains(&md.full_name().to_string()) {
            return None;
        }
        seen.push(md.full_name().to_string());
        md.fields().find_map(|f| named_field(proto, f.kind().as_message()?, clear, seen))
    }
}
