use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::Router;
use clap::Parser;
use error_stack::{Report, ResultExt};
use filigree::{
    errors::panic_handler,
    propagate_http_span::extract_request_parent,
    tracing_config::{configure_tracing, create_tracing_config, teardown_tracing, TracingProvider},
};
use sqlx::sqlite::SqliteConnectOptions;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    trace::{DefaultOnFailure, DefaultOnRequest, TraceLayer},
};
use tracing::{Level, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{
    config::{find_configs, merge_server_config},
    proxy::ServerState,
};

mod config;
mod error;
mod proxy;

use error::Error;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The path to the configuration file or a directory containing it. If omitted,
    /// the default configuration path will be checked.
    #[clap(long, short = 'c')]
    config: Option<String>,

    /// Do not read the .env file
    #[clap(long)]
    no_dotenv: bool,

    /// The SQLite database to use, if any. This can also be set in the configuration file.
    #[clap(long = "db", env = "DATABASE_PATH")]
    database_path: Option<String>,

    /// The IP host to bind to
    #[clap(long, env = "HOST")]
    host: Option<String>,

    /// The TCP port to listen on
    #[clap(long, env = "PORT")]
    port: Option<u16>,
}

async fn serve(cmd: Cli) -> Result<(), Report<Error>> {
    error_stack::Report::set_color_mode(error_stack::fmt::ColorMode::None);

    let configs = find_configs(cmd.config.clone())?;
    let server_config = merge_server_config(&cmd, &configs);

    // Must load configs and run dotenv before starting tracing, so that they can set destination and
    // service name.
    if !cmd.no_dotenv {
        for (dir, config) in configs.cwd.iter().rev().chain(configs.cwd.iter().rev()) {
            if config.server_config.dotenv.unwrap_or(true) {
                dotenvy::from_path(dir.join(".env")).ok();
            }
        }

        if server_config.dotenv.unwrap_or(true) {
            dotenvy::dotenv().ok();
        }
    }

    let tracing_config = create_tracing_config(
        "",
        "CHRONICLE_",
        TracingProvider::None,
        Some("chronicle".to_string()),
        None,
    )
    .change_context(Error::ServerStart)?;

    configure_tracing(
        "CHRONICLE_",
        tracing_config,
        tracing_subscriber::fmt::time::ChronoUtc::rfc_3339(),
        std::io::stdout,
    )
    .change_context(Error::ServerStart)?;

    for (dir, _) in configs.global.iter().chain(configs.cwd.iter()) {
        tracing::info!("Loaded config from {}", dir.display());
    }

    let db_pool = if let Some(database_path) = &server_config.database_path {
        tracing::info!("Opening database at {database_path}");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(database_path)
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                    .create_if_missing(true),
            )
            .await
            .change_context(Error::Db)?;

        Some(pool)
    } else {
        tracing::info!("No database configured");
        None
    };

    let proxy = proxy::build_proxy(db_pool, configs).await?;

    let app = Router::new()
        .merge(proxy::create_routes())
        .with_state(Arc::new(ServerState { proxy }))
        .layer(
            ServiceBuilder::new()
                .layer(panic_handler(false))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|req: &axum::extract::Request| {
                            let method = req.method();
                            let uri = req.uri();

                            // Add the matched route to the span
                            let route = req
                                .extensions()
                                .get::<axum::extract::MatchedPath>()
                                .map(|matched_path| matched_path.as_str());

                            let host = req.headers().get("host").and_then(|s| s.to_str().ok());

                            let request_id = req
                                .headers()
                                .get("X-Request-Id")
                                .and_then(|s| s.to_str().ok())
                                .unwrap_or("");

                            let span = tracing::info_span!("request",
                                request_id,
                                http.host=host,
                                http.method=%method,
                                http.uri=%uri,
                                http.route=route,
                                http.status_code = tracing::field::Empty,
                                error = tracing::field::Empty
                            );

                            let context = extract_request_parent(req);
                            span.set_parent(context);

                            span
                        })
                        .on_response(|res: &http::Response<_>, latency: Duration, span: &Span| {
                            let status = res.status();
                            span.record("http.status_code", status.as_u16());
                            if status.is_client_error() || status.is_server_error() {
                                span.record("error", "true");
                            }

                            tracing::info!(
                                latency = %format!("{} ms", latency.as_millis()),
                                http.status_code = status.as_u16(),
                                "finished processing request"
                            );
                        })
                        .on_request(DefaultOnRequest::new().level(Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
                )
                .layer(CompressionLayer::new())
                .into_inner(),
        );

    let bind_ip = server_config
        .host
        .as_deref()
        .unwrap_or("::1")
        .parse::<IpAddr>()
        .change_context(Error::ServerStart)?;
    let port = server_config.port.unwrap_or(9782);
    let bind_addr = SocketAddr::from((bind_ip, port));
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .change_context(Error::ServerStart)?;
    let actual_addr = listener.local_addr().change_context(Error::ServerStart)?;
    let port = actual_addr.port();
    let host = actual_addr.ip().to_string();
    tracing::info!("Listening on {host}:{port}");

    let shutdown_signal = filigree::server::shutdown_signal();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal)
    .await
    .change_context(Error::ServerStart)?;

    tracing::info!("Exporting remaining traces");
    teardown_tracing().await.change_context(Error::Shutdown)?;
    tracing::info!("Trace shut down complete");

    Ok(())
}

fn main() -> Result<(), Report<Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(actual_main())
}

pub async fn actual_main() -> Result<(), Report<Error>> {
    let cli = Cli::parse();
    serve(cli).await?;
    Ok(())
}
