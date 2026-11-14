//! `karrelage-server --password <secret> [--bind 0.0.0.0:8787] [--db karrelage.sqlite]` (the password can come from the
//! environment variable `KARRELAGE_PASSWORD` instead, kept out of the process list). See the crate docs (lib.rs) and
//! `deploy/` for running it as a systemd service.
use anyhow::{bail, Context, Result};
use karrelage_server::{app, db::Db, AppState};

struct Args {
    password: String,
    bind: String,
    db: String,
}

fn args() -> Result<Args> {
    let mut a = Args { password: std::env::var("KARRELAGE_PASSWORD").unwrap_or_default(), bind: "0.0.0.0:8787".into(), db: "karrelage.sqlite".into() };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().with_context(|| format!("{k} needs a value"));
        match k.as_str() {
            "--password" => a.password = v()?,
            "--bind" => a.bind = v()?,
            "--db" => a.db = v()?,
            "-h" | "--help" => {
                println!(
                    "karrelage-server --password <secret> [--bind 0.0.0.0:8787] [--db karrelage.sqlite]\n(or the KARRELAGE_PASSWORD environment variable)"
                );
                std::process::exit(0);
            }
            _ => bail!("unknown argument {k} (--help)"),
        }
    }
    if a.password.len() < 8 {
        bail!("a password of at least 8 characters is needed (--password or KARRELAGE_PASSWORD)");
    }
    Ok(a)
}

#[tokio::main]
async fn main() -> Result<()> {
    let a = args()?;
    let db = Db::open(&a.db).with_context(|| format!("database {}", a.db))?;
    let listener = tokio::net::TcpListener::bind(&a.bind).await.with_context(|| format!("bind {}", a.bind))?;
    println!("karrelage-server {} listening on {} (database {})", env!("CARGO_PKG_VERSION"), a.bind, a.db);
    axum::serve(listener, app(AppState::new(&a.password, db)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
