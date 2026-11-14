//! The shared prices, per game server, in SQLite. Per (server, gid) and per price (`unit` = read in the auction house, `avg` =
//! weekly average) the value observed last wins. Every accepted change gets a new `seq` (one counter for the whole database):
//! a client asks for what changed after the last `seq` it saw, whatever its clock says.
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

/// Observation times more than this far in the future (a client's clock ahead) are taken as the server's now.
pub const FUTURE_SLACK: i64 = 300;

/// One item's prices, on the wire `[gid, unit, unit_t, avg, avg_t]` (null = not known / not sent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PriceRow {
    pub gid: i64,
    pub unit: Option<i64>,
    pub unit_t: Option<i64>,
    pub avg: Option<i64>,
    pub avg_t: Option<i64>,
}

impl PriceRow {
    pub fn from_json(v: &Value) -> Option<PriceRow> {
        let a = v.as_array()?;
        let at = |i: usize| a.get(i).and_then(Value::as_i64);
        let gid = at(0).filter(|g| *g > 0)?;
        // a price needs its time; prices must be positive
        let pair = |p: usize| match (at(p).filter(|x| *x > 0), at(p + 1)) {
            (Some(x), Some(t)) => (Some(x), Some(t)),
            _ => (None, None),
        };
        let ((unit, unit_t), (avg, avg_t)) = (pair(1), pair(3));
        Some(PriceRow { gid, unit, unit_t, avg, avg_t })
    }

    pub fn to_json(self) -> Value {
        json!([self.gid, self.unit, self.unit_t, self.avg, self.avg_t])
    }
}

pub struct Db {
    conn: Connection,
    /// last `seq` given
    seq: i64,
}

impl Db {
    pub fn open(path: &str) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::with(conn)
    }

    pub fn memory() -> Result<Db> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(conn: Connection) -> Result<Db> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS prices(server INTEGER NOT NULL, gid INTEGER NOT NULL, unit INTEGER, unit_t INTEGER,
                 avg INTEGER, avg_t INTEGER, seq INTEGER NOT NULL, PRIMARY KEY(server, gid));
             CREATE INDEX IF NOT EXISTS prices_seq ON prices(server, seq);
             -- every accepted value, never rewritten: historic data for later
             CREATE TABLE IF NOT EXISTS observations(server INTEGER NOT NULL, gid INTEGER NOT NULL, kind TEXT NOT NULL,
                 value INTEGER NOT NULL, t INTEGER NOT NULL, received INTEGER NOT NULL);",
        )?;
        let seq = conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM prices", [], |r| r.get(0))?;
        Ok(Db { conn, seq })
    }

    pub fn seq(&self) -> i64 {
        self.seq
    }

    /// Merge pushed rows (newest observation per price wins); the rows that changed, as stored now.
    pub fn merge(&mut self, server: i64, rows: &[PriceRow], now: i64) -> Result<Vec<PriceRow>> {
        let tx = self.conn.transaction()?;
        let mut changed = vec![];
        for r in rows {
            let old: PriceRow = tx
                .query_row("SELECT unit, unit_t, avg, avg_t FROM prices WHERE server = ?1 AND gid = ?2", params![server, r.gid], |x| {
                    Ok(PriceRow { gid: r.gid, unit: x.get(0)?, unit_t: x.get(1)?, avg: x.get(2)?, avg_t: x.get(3)? })
                })
                .optional()?
                .unwrap_or(PriceRow { gid: r.gid, ..Default::default() });
            let clamp = |t: Option<i64>| t.map(|t| if t > now + FUTURE_SLACK { now } else { t });
            let mut new = old;
            let mut obs = vec![];
            for (kind, value, t, cur_v, cur_t) in
                [("unit", r.unit, clamp(r.unit_t), &mut new.unit, &mut new.unit_t), ("avg", r.avg, clamp(r.avg_t), &mut new.avg, &mut new.avg_t)]
            {
                if let (Some(v), Some(t)) = (value, t) {
                    if cur_t.is_none_or(|c| t > c) {
                        *cur_v = Some(v);
                        *cur_t = Some(t);
                        obs.push((kind, v, t));
                    }
                }
            }
            if obs.is_empty() {
                continue;
            }
            self.seq += 1;
            tx.execute(
                "INSERT INTO prices(server, gid, unit, unit_t, avg, avg_t, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(server, gid) DO UPDATE SET unit = ?3, unit_t = ?4, avg = ?5, avg_t = ?6, seq = ?7",
                params![server, new.gid, new.unit, new.unit_t, new.avg, new.avg_t, self.seq],
            )?;
            for (kind, v, t) in obs {
                tx.execute(
                    "INSERT INTO observations(server, gid, kind, value, t, received) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![server, new.gid, kind, v, t, now],
                )?;
            }
            changed.push(new);
        }
        tx.commit()?;
        Ok(changed)
    }

    /// The rows of a game server changed after `cursor` (all of them from 0). A cursor past the last `seq` (the database was
    /// replaced) starts over.
    pub fn pull(&self, server: i64, cursor: i64) -> Result<Vec<PriceRow>> {
        let cursor = if cursor > self.seq { 0 } else { cursor };
        let mut st = self.conn.prepare_cached("SELECT gid, unit, unit_t, avg, avg_t FROM prices WHERE server = ?1 AND seq > ?2 ORDER BY seq")?;
        let rows =
            st.query_map(params![server, cursor], |x| Ok(PriceRow { gid: x.get(0)?, unit: x.get(1)?, unit_t: x.get(2)?, avg: x.get(3)?, avg_t: x.get(4)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(gid: i64, unit: Option<(i64, i64)>, avg: Option<(i64, i64)>) -> PriceRow {
        PriceRow { gid, unit: unit.map(|u| u.0), unit_t: unit.map(|u| u.1), avg: avg.map(|a| a.0), avg_t: avg.map(|a| a.1) }
    }

    #[test]
    fn newest_wins_per_price() {
        let mut db = Db::memory().unwrap();
        db.merge(295, &[row(1, Some((100, 10)), Some((90, 5)))], 1000).unwrap();
        // older unit ignored, newer avg taken
        let ch = db.merge(295, &[row(1, Some((50, 9)), Some((95, 6)))], 1000).unwrap();
        assert_eq!(ch, vec![row(1, Some((100, 10)), Some((95, 6)))]);
        // nothing newer: nothing changes
        assert!(db.merge(295, &[row(1, Some((70, 10)), None)], 1000).unwrap().is_empty());
        // another game server is apart
        db.merge(291, &[row(1, Some((1, 20)), None)], 1000).unwrap();
        assert_eq!(db.pull(295, 0).unwrap(), vec![row(1, Some((100, 10)), Some((95, 6)))]);
    }

    #[test]
    fn pull_after_cursor() {
        let mut db = Db::memory().unwrap();
        db.merge(295, &[row(1, Some((100, 10)), None), row(2, Some((200, 10)), None)], 1000).unwrap();
        let cursor = db.seq();
        assert!(db.pull(295, cursor).unwrap().is_empty());
        db.merge(295, &[row(2, Some((210, 11)), None)], 1000).unwrap();
        assert_eq!(db.pull(295, cursor).unwrap(), vec![row(2, Some((210, 11)), None)]);
        // a cursor from another database starts over
        assert_eq!(db.pull(295, 999).unwrap().len(), 2);
    }

    #[test]
    fn future_times_clamped() {
        let mut db = Db::memory().unwrap();
        let ch = db.merge(295, &[row(1, Some((100, 5000)), None)], 1000).unwrap();
        assert_eq!(ch[0].unit_t, Some(1000));
        assert_eq!(PriceRow::from_json(&json!([3, 0, 10, 5, null])), Some(row(3, None, None)));
        assert_eq!(PriceRow::from_json(&json!([0, 1, 1, null, null])), None);
    }
}
