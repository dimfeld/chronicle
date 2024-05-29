use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::Router;
use chronicle_proxy::database::Database;
use clap::Parser;
use config::{Configs, LocalServerConfig};
use database::init_database;
use error_stack::{Report, ResultExt};
use filigree::{
    errors::panic_handler,
    propagate_http_span::extract_request_parent,
    tracing_config::{configure_tracing, create_tracing_config, teardown_tracing, TracingProvider},
};
use futures::Future;
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
mod database;
mod error;
mod proxy;

use error::Error;

#[derive(Debug, Parser)]
#[command(version, about)]
pub(crate) struct Cli {
    /// The path to the configuration file or a directory containing it. If omitted,
    /// the default configuration path will be checked.
    #[clap(long, short = 'c')]
    config: Option<String>,

    /// Do not read the .env file
    #[clap(long)]
    no_dotenv: bool,

    /// The SQLite or PostgreSQL database to use, if any. This can also be set in the configuration file.
    /// Takes a file path for SQLite or a connection string for PostgreSQL
    #[clap(long = "db", env = "DATABASE_URL")]
    database: Option<String>,

    /// The IP host to bind to
    #[clap(long, env = "HOST")]
    host: Option<String>,

    /// The TCP port to listen on
    #[clap(long, env = "PORT")]
    port: Option<u16>,
}

pub(crate) async fn run(cmd: Cli) -> Result<(), Report<Error>> {
    let configs = find_configs(cmd.config.clone())?;
    let mut server_config = merge_server_config(&configs);

    // Must load configs and run dotenv before starting tracing, so that they can set destination and
    // service name.
    if !cmd.no_dotenv {
        let mut loaded_env = false;
        for (dir, config) in configs.cwd.iter().rev().chain(configs.cwd.iter().rev()) {
            if config.server_config.dotenv.unwrap_or(true) {
                dotenvy::from_path(dir.join(".env")).ok();
                loaded_env = true;
            }
        }

        if server_config.dotenv.unwrap_or(true) {
            dotenvy::dotenv().ok();
            loaded_env = true;
        }

        if loaded_env {
            // Reread with the environment variables in place
            let cmd = Cli::parse();

            if cmd.database.is_some() {
                server_config.database = cmd.database;
            }

            if cmd.host.is_some() {
                server_config.host = cmd.host;
            }

            if cmd.port.is_some() {
                server_config.port = cmd.port;
            }
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

    let db = init_database(server_config.database.clone())
        .await
        .change_context(Error::Db)?;

    let shutdown_signal = filigree::server::shutdown_signal();
    serve(server_config, configs, db, shutdown_signal).await
}

pub(crate) async fn serve(
    server_config: LocalServerConfig,
    all_configs: Configs,
    db: Option<Database>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Report<Error>> {
    let proxy = proxy::build_proxy(db, all_configs).await?;

    let mut state = Arc::new(ServerState { proxy });

    let app = Router::new()
        .merge(proxy::create_routes())
        .with_state(state.clone())
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

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
    .change_context(Error::ServerStart)?;

    tracing::info!("Shutting down proxy");
    Arc::get_mut(&mut state)
        .ok_or(Error::Shutdown)
        .attach_printable("Failed to get proxy reference for shutdown")?
        .proxy
        .shutdown()
        .await;

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
    error_stack::Report::set_color_mode(error_stack::fmt::ColorMode::None);
    let cli = Cli::parse();
    run(cli).await?;
    Ok(())
}
