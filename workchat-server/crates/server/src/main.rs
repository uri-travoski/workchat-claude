use workchat_server::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("workchat-server {}", proto::VERSION);
        return Ok(());
    }
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!(
            "workchat-server {}\n\nConfigured by environment variables (all optional):\n  WORKCHAT_BIND (0.0.0.0:8080)  WORKCHAT_DATA_DIR (./data)  WORKCHAT_PUBLIC_URL\n  WORKCHAT_ADMIN_DIR  WORKCHAT_BOOTSTRAP_ADMIN=user:password  WORKCHAT_NTFY_URL\n  WORKCHAT_MAX_UPLOAD_MB (100)  WORKCHAT_NAME\nSee README.md and docs/DEPLOY.md for the full list.",
            proto::VERSION
        );
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        // Used by the container HEALTHCHECK: plain HTTP GET on loopback, exit 0 when healthy.
        use std::io::{Read, Write};
        let bind = std::env::var("WORKCHAT_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let port = bind.rsplit(':').next().unwrap_or("8080");
        let ok = std::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .and_then(|mut s| {
                s.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
                s.write_all(b"GET /api/v1/health HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
                let mut buf = String::new();
                s.read_to_string(&mut buf)?;
                Ok(buf.starts_with("HTTP/1.0 200") || buf.starts_with("HTTP/1.1 200"))
            })
            .unwrap_or(false);
        std::process::exit(if ok { 0 } else { 1 });
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into()))
        .with_target(false)
        .init();
    let cfg = Config::from_env();
    let bind = cfg.bind.clone();
    let st = workchat_server::build_state(cfg).await?;
    workchat_server::spawn_background(st.clone());
    let app = workchat_server::router(st);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("workchat-server {} listening on {bind}", proto::VERSION);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            {
                let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("sigterm handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
