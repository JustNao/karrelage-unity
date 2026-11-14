//! Request signing: the password never travels (the server may be reached over plain HTTP). Each request carries
//! `X-Karrelage-Time` (unix seconds) and `X-Karrelage-Sig` = hex(HMAC-SHA256(password, "<time>\n<METHOD><endpoint>\n<hex sha256(body)>")),
//! `endpoint` being the path from `/v1/` on (a reverse proxy may serve the server under a prefix). A request more than
//! `MAX_SKEW` off the server's clock is refused, so a recorded request can't be replayed later.
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub const TIME_HEADER: &str = "x-karrelage-time";
pub const SIG_HEADER: &str = "x-karrelage-sig";
/// Seconds a request's time may differ from the server's.
pub const MAX_SKEW: i64 = 300;

/// The part of a path the signature covers: from `/v1/` on (`/karrelage/v1/sync` -> `/v1/sync`).
pub fn endpoint(path: &str) -> &str {
    path.find("/v1/").map_or(path, |i| &path[i..])
}

fn mac(password: &[u8], time: i64, method: &str, path: &str, body: &[u8]) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(password).expect("hmac takes any key length");
    m.update(format!("{time}\n{}{}\n{}", method.to_ascii_uppercase(), endpoint(path), hex::encode(Sha256::digest(body))).as_bytes());
    m
}

/// The signature of a request (what a client sends).
pub fn sign(password: &[u8], time: i64, method: &str, path: &str, body: &[u8]) -> String {
    hex::encode(mac(password, time, method, path, body).finalize().into_bytes())
}

/// Why a request is refused, if it is.
pub fn check(password: &[u8], time: Option<&str>, sig: Option<&str>, method: &str, path: &str, body: &[u8], now: i64) -> Result<(), &'static str> {
    let time: i64 = time.and_then(|t| t.trim().parse().ok()).ok_or("missing time")?;
    if (time - now).abs() > MAX_SKEW {
        return Err("clock off");
    }
    let sig = sig.and_then(|s| hex::decode(s.trim()).ok()).ok_or("missing signature")?;
    mac(password, time, method, path, body).verify_slice(&sig).map_err(|_| "bad signature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures() {
        let s = sign(b"pw", 1000, "POST", "/v1/sync", b"{}");
        assert_eq!(check(b"pw", Some("1000"), Some(&s), "POST", "/v1/sync", b"{}", 1010), Ok(()));
        // behind a proxy prefix: same endpoint
        assert_eq!(check(b"pw", Some("1000"), Some(&s), "POST", "/karrelage/v1/sync", b"{}", 1010), Ok(()));
        assert_eq!(check(b"other", Some("1000"), Some(&s), "POST", "/v1/sync", b"{}", 1010), Err("bad signature"));
        assert_eq!(check(b"pw", Some("1000"), Some(&s), "POST", "/v1/sync", b"{ }", 1010), Err("bad signature"));
        assert_eq!(check(b"pw", Some("1000"), Some(&s), "POST", "/v1/sync", b"{}", 1000 + MAX_SKEW + 1), Err("clock off"));
        assert_eq!(check(b"pw", None, Some(&s), "POST", "/v1/sync", b"{}", 1000), Err("missing time"));
    }
}
