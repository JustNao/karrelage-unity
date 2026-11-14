//! Karrelage sync server: a few clients (the author and friends) share the prices they read, per game server.
//!
//! Protocol v1 (JSON, gzip accepted both ways, every request signed: see `auth`):
//! - `GET /v1/ping` -> `{ok, version}`: checks the address and the password (Paramètres « Tester la connexion »).
//! - `POST /v1/sync` `{server, cursors: {prices: seq}, push: {prices: [[gid, unit, unit_t, avg, avg_t]]}}` ->
//!   `{cursors: {prices: seq}, changes: {prices: [...]}}`: the client's new prices in, the others' changes since its cursor out
//!   (its own pushed rows aren't sent back). Unknown topics are ignored, so a newer client can add some.
pub mod auth;
pub mod bus;
pub mod db;

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::compression::CompressionLayer;
use tower_http::decompression::RequestDecompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;

/// Largest body accepted, compressed (the limit layer) and uncompressed (read by the signature check).
pub const BODY_LIMIT: usize = 8 << 20;

pub struct AppState {
    pub password: Vec<u8>,
    pub db: Mutex<db::Db>,
    pub bus: bus::Bus,
    /// the clock, replaceable in tests
    pub now: fn() -> i64,
}

pub fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl AppState {
    pub fn new(password: &str, db: db::Db) -> Arc<AppState> {
        Arc::new(AppState { password: password.as_bytes().to_vec(), db: Mutex::new(db), bus: bus::Bus::default(), now: unix_now })
    }
}

pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/ping", get(ping))
        .route("/v1/sync", post(sync))
        .layer(middleware::from_fn_with_state(state.clone(), signed))
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .with_state(state)
}

/// Refuses unsigned requests (401 with the reason); the body is read here, after decompression, to check its hash.
async fn signed(State(st): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let Ok(bytes) = to_bytes(body, BODY_LIMIT).await else { return (StatusCode::PAYLOAD_TOO_LARGE, "body too large").into_response() };
    let header = |n: &str| parts.headers.get(n).and_then(|v| v.to_str().ok());
    let ok = auth::check(&st.password, header(auth::TIME_HEADER), header(auth::SIG_HEADER), parts.method.as_str(), parts.uri.path(), &bytes, (st.now)());
    if let Err(why) = ok {
        println!("refused {} {}: {why}", parts.method, parts.uri.path());
        return (StatusCode::UNAUTHORIZED, why).into_response();
    }
    next.run(Request::from_parts(parts, Body::from(bytes))).await
}

async fn ping() -> Json<Value> {
    Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
}

async fn sync(State(st): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, (StatusCode, String)> {
    let server = body["server"].as_i64().filter(|s| *s > 0).ok_or((StatusCode::BAD_REQUEST, "server missing".into()))?;
    let cursor = body["cursors"]["prices"].as_i64().unwrap_or(0);
    let rows: Vec<db::PriceRow> = body["push"]["prices"].as_array().into_iter().flatten().filter_map(db::PriceRow::from_json).collect();
    let now = (st.now)();
    let err = |e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    let (changes, accepted, cursor) = {
        let mut db = st.db.lock().unwrap();
        // the others' changes first: the rows this request changes then get a seq at or under the new cursor (not sent back)
        let changes = db.pull(server, cursor).map_err(err)?;
        let accepted = db.merge(server, &rows, now).map_err(err)?;
        (changes, accepted, db.seq())
    };
    if !rows.is_empty() || !changes.is_empty() {
        println!("sync server {server}: {} pushed, {} accepted, {} sent", rows.len(), accepted.len(), changes.len());
    }
    if !accepted.is_empty() {
        st.bus.publish(server, json!({"topic": "prices", "rows": accepted.iter().map(|r| r.to_json()).collect::<Vec<_>>()}));
    }
    Ok(Json(json!({"cursors": {"prices": cursor}, "changes": {"prices": changes.into_iter().map(db::PriceRow::to_json).collect::<Vec<_>>()}})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::CONTENT_ENCODING;
    use tower::ServiceExt;

    const NOW: i64 = 1_800_000_000;

    fn state() -> Arc<AppState> {
        Arc::new(AppState { password: b"pw".to_vec(), db: Mutex::new(db::Db::memory().unwrap()), bus: bus::Bus::default(), now: || NOW })
    }

    fn request(method: &str, path: &str, body: &[u8], password: &[u8], time: i64, gzip: bool) -> Request {
        let sig = auth::sign(password, time, method, path, body);
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header(auth::TIME_HEADER, time.to_string())
            .header(auth::SIG_HEADER, sig)
            .header("content-type", "application/json");
        let payload = if gzip {
            use std::io::Write;
            b = b.header(CONTENT_ENCODING, "gzip");
            let mut e = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
            e.write_all(body).unwrap();
            e.finish().unwrap()
        } else {
            body.to_vec()
        };
        b.body(Body::from(payload)).unwrap()
    }

    async fn send(st: &Arc<AppState>, req: Request) -> (StatusCode, Value) {
        let res = app(st.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), BODY_LIMIT).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn signed_sync_round_trip() {
        let st = state();
        let (s, v) = send(&st, request("GET", "/v1/ping", b"", b"pw", NOW, false)).await;
        assert_eq!((s, v["ok"].clone()), (StatusCode::OK, json!(true)));
        // wrong password, stale time
        assert_eq!(send(&st, request("GET", "/v1/ping", b"", b"nope", NOW, false)).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(send(&st, request("GET", "/v1/ping", b"", b"pw", NOW - 3600, false)).await.0, StatusCode::UNAUTHORIZED);

        // client A pushes (gzipped): not sent back to it
        let a = json!({"server": 295, "cursors": {"prices": 0}, "push": {"prices": [[20819, 137, NOW - 5, null, null]]}});
        let (s, v) = send(&st, request("POST", "/v1/sync", a.to_string().as_bytes(), b"pw", NOW, true)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["changes"]["prices"], json!([]));
        let cursor_a = v["cursors"]["prices"].as_i64().unwrap();
        // client B pulls everything, then pushes its own
        let b = json!({"server": 295, "cursors": {"prices": 0}, "push": {"prices": [[1711, 5000, NOW - 1, null, null]]}});
        let (_, v) = send(&st, request("POST", "/v1/sync", b.to_string().as_bytes(), b"pw", NOW, false)).await;
        assert_eq!(v["changes"]["prices"], json!([[20819, 137, NOW - 5, null, null]]));
        // A gets B's row only
        let a = json!({"server": 295, "cursors": {"prices": cursor_a}});
        let (_, v) = send(&st, request("POST", "/v1/sync", a.to_string().as_bytes(), b"pw", NOW, false)).await;
        assert_eq!(v["changes"]["prices"], json!([[1711, 5000, NOW - 1, null, null]]));
    }
}
