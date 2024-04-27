use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{extract::FromRef, handler::Handler, routing::get, Router};
use chronicle_proxy::Proxy;
use error_stack::{Report, ResultExt};
use filigree::{
    auth::{
        oauth::providers::OAuthProvider, CorsSetting, ExpiryStyle, SessionBackend,
        SessionCookieBuilder,
    },
    error_reporting::ErrorReporter,
    errors::{panic_handler, ObfuscateErrorLayer, ObfuscateErrorLayerSettings},
    requests::MakeRequestUuidV7,
    server::FiligreeState,
};
use sqlx::PgPool;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    timeout::TimeoutLayer,
    trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer},
    ServiceBuilderExt,
};
use tracing::{event, Level};

use crate::{error::Error, proxy::build::build_proxy};

mod health;
mod meta;
#[cfg(test)]
mod tests;

/// Shared state used by the server
pub struct ServerStateInner {
    /// If the app is running in production mode. This should be used sparingly as there should be
    /// a minimum of difference between production and development to prevent bugs.
    pub production: bool,
    /// If the app is being hosted on plain HTTP
    pub insecure: bool,
    /// State for internal filigree endpoints
    pub filigree: Arc<FiligreeState>,
    /// The Postgres database connection pool
    pub db: PgPool,
    /// Secrets loaded from the environment
    pub secrets: Secrets,

    /// LLM providers
    pub proxy: Proxy,
}

impl ServerStateInner {
    pub fn site_scheme(&self) -> &'static str {
        if self.insecure {
            "http"
        } else {
            "https"
        }
    }
}

impl std::ops::Deref for ServerStateInner {
    type Target = FiligreeState;

    fn deref(&self) -> &Self::Target {
        &self.filigree
    }
}

impl std::fmt::Debug for ServerStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerStateInner")
            .field("production", &self.production)
            .field("insecure", &self.insecure)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct ServerState(Arc<ServerStateInner>);

impl std::ops::Deref for ServerState {
    type Target = ServerStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRef<ServerState> for Arc<FiligreeState> {
    fn from_ref(inner: &ServerState) -> Self {
        inner.0.filigree.clone()
    }
}

impl FromRef<ServerState> for PgPool {
    fn from_ref(inner: &ServerState) -> Self {
        inner.0.db.clone()
    }
}

impl FromRef<ServerState> for SessionBackend {
    fn from_ref(inner: &ServerState) -> Self {
        inner.0.session_backend.clone()
    }
}

pub struct Secrets {}

impl Secrets {
    /// Load the secrets from the environment
    pub fn from_env() -> Result<Secrets, Report<Error>> {
        Ok(Self {})
    }

    #[cfg(test)]
    /// Create a new Secrets struct with all the strings empty, for testing where we don't need any
    /// secrets.
    pub fn empty() -> Secrets {
        Secrets {}
    }
}

/// The server and related information
pub struct Server {
    /// The host the server is bound to
    pub host: String,
    /// The port the server is bound to
    pub port: u16,
    /// The server itself.
    pub app: Router<()>,
    /// The server state.
    pub state: ServerState,
    /// The server's TCP listener
    pub listener: tokio::net::TcpListener,

    /// Vite manifest watcher for replacing web builds at runtime
    manifest_watcher: Option<filigree::vite_manifest::watch::ManifestWatcher>,
}

impl Server {
    /// Run the server, and perform a graceful shutdown when receiving a ctrl+c (SIGINT or
    /// equivalent).
    pub async fn run(self) -> Result<(), Report<Error>> {
        let shutdown = filigree::server::shutdown_signal();
        self.run_with_shutdown_signal(shutdown).await
    }

    /// Run the server, and shut it down when `shutdown_rx` closes.
    pub async fn run_with_shutdown_signal(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), Report<Error>> {
        axum::serve(
            self.listener,
            self.app
                .into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await
        .change_context(Error::ServerStart)?;

        // Can do extra shutdown tasks here.

        Ok(())
    }
}

/// Create a TCP listener.
pub async fn create_tcp_listener(
    host: &str,
    port: u16,
) -> Result<tokio::net::TcpListener, Report<Error>> {
    let bind_ip = host.parse::<IpAddr>().change_context(Error::ServerStart)?;
    let bind_addr = SocketAddr::from((bind_ip, port));
    tokio::net::TcpListener::bind(bind_addr)
        .await
        .change_context(Error::ServerStart)
}

pub enum ServerBind {
    /// A host and port to bind to
    HostPort(String, u16),
    /// An existing TCP listener to use
    Listener(tokio::net::TcpListener),
}

pub struct ServeFrontend {
    pub port: Option<u16>,
    pub path: Option<String>,
    pub vite_manifest: Option<String>,
    pub watch_vite_manifest: bool,
    pub livereload: bool,
}

/// Configuration for the server
pub struct Config {
    /// The environment we're running in. Currently this just distinguishes between
    /// "development" and any other value.
    pub env: String,
    /// The host and port to bind to, or an existing TCP listener
    pub bind: ServerBind,
    /// The port and disk asset location of the frontend server.
    pub serve_frontend: ServeFrontend,
    /// True if the site is being hosted on plain HTTP. This should only be set in a development
    /// or testing environment.
    pub insecure: bool,
    /// How long to wait before timing out a request
    pub request_timeout: std::time::Duration,
    pub pg_pool: PgPool,

    pub cookie_configuration: SessionCookieBuilder,
    /// When user sessions should expire.
    pub session_expiry: ExpiryStyle,
    /// Flags controlling how new users are able to sign up or be invited.
    pub new_user_flags: filigree::server::NewUserFlags,
    /// The email sending service to use.
    pub email_sender: filigree::email::services::EmailSender,

    /// Whether or not to obfuscate details from internal server errors. If omitted,
    /// the default is to obfuscate when env != "development".
    pub obfuscate_errors: Option<bool>,

    pub hosts: Vec<String>,
    pub api_cors: filigree::auth::CorsSetting,

    /// The base URL for OAuth redirect URLs.
    pub oauth_redirect_url_base: String,
    /// Set the OAuth providers. If this is None, OAuth providers will be configured based on the
    /// environment variables present for each provider. See
    /// [filigree::auth::oauth::providers::create_supported_providers] for the logic there.
    ///
    /// OAuth can be disabled, regardless of environment variable settings, but passing `Some(Vec::new())`.
    pub oauth_providers: Option<Vec<Box<dyn OAuthProvider>>>,

    /// Secrets for the server. Most often this should be initialized using [Secrets::from_env].
    pub secrets: Secrets,
}

/// Create the server and return it, ready to run.
pub async fn create_server(config: Config) -> Result<Server, Report<Error>> {
    let production = config.env != "development" && !cfg!(debug_assertions);
    let obfuscate_errors = config.obfuscate_errors.unwrap_or(production);

    let host_values = config
        .hosts
        .iter()
        .map(|h| h.parse::<http::header::HeaderValue>())
        .collect::<Result<Vec<_>, _>>()
        .change_context(Error::ServerStart)
        .attach_printable("Unable to parse hosts list")?;

    let oauth_redirect_base = format!("{}/auth/oauth/login", config.oauth_redirect_url_base);
    let http_client = reqwest::Client::builder()
        .user_agent("chronicle")
        .build()
        .unwrap();

    let state = ServerState(Arc::new(ServerStateInner {
        production,
        filigree: Arc::new(FiligreeState {
            http_client,
            db: config.pg_pool.clone(),
            email: config.email_sender,
            new_user_flags: config.new_user_flags,
            hosts: config.hosts,
            user_creator: Box::new(crate::users::users::UserCreator),
            oauth_providers: config.oauth_providers.unwrap_or_else(|| {
                filigree::auth::oauth::providers::create_supported_providers(
                    "",
                    &oauth_redirect_base,
                )
            }),
            session_backend: SessionBackend::new(
                config.pg_pool.clone(),
                config.cookie_configuration,
                config.session_expiry,
            ),
            error_reporter: ErrorReporter::Noop,
        }),
        insecure: config.insecure,
        db: config.pg_pool.clone(),
        secrets: config.secrets,

        proxy: build_proxy(config.pg_pool.clone()).await?,
    }));

    let auth_queries = Arc::new(crate::auth::AuthQueries::new(
        config.pg_pool,
        config.session_expiry,
    ));

    let api_cors_layer = match config.api_cors {
        CorsSetting::None => CorsLayer::new(),
        CorsSetting::AllowAll => CorsLayer::permissive().max_age(Duration::from_secs(60 * 60)),
        CorsSetting::AllowHostList => CorsLayer::new()
            .allow_origin(host_values)
            .allow_methods(tower_http::cors::Any)
            .max_age(Duration::from_secs(60 * 60)),
    };

    let api_routes: Router<ServerState> = Router::new()
        .route("/healthz", get(health::healthz))
        .nest("/meta", meta::create_routes())
        .merge(filigree::auth::endpoints::create_routes())
        .merge(filigree::auth::oauth::create_routes())
        .merge(crate::models::create_routes())
        .merge(crate::users::users::create_routes())
        .merge(crate::auth::create_routes())
        .nest("/proxy", crate::proxy::create_routes())
        // Return not found here so we don't run the other non-API fallbacks
        .fallback(|| async { Error::NotFound("Route") });

    let web_routes = crate::pages::create_routes();

    let app = Router::new().nest("/api", api_routes).merge(web_routes);

    let ServeFrontend {
        port: mut web_port,
        path: mut web_dir,
        vite_manifest,
        watch_vite_manifest,
        livereload,
    } = config.serve_frontend;

    let manifest_path = vite_manifest.as_ref().map(std::path::Path::new);
    let manifest_watcher =
        crate::pages::layout::init_page_layout(manifest_path, watch_vite_manifest)
            .change_context(Error::ServerStart)?;

    web_port = web_port.filter(|p| *p != 0);
    web_dir = web_dir.filter(|p| !p.is_empty());

    let app = match (web_port, web_dir) {
        (Some(web_port), Some(web_dir)) => {
            let fallback = filigree::route_services::ForwardRequest::new(format!(
                "http://localhost:{web_port}"
            ));

            let serve_fs = tower_http::services::ServeDir::new(&web_dir)
                .precompressed_gzip()
                .precompressed_br()
                // Pass non-GET methods to the fallback instead of returning 405
                .call_fallback_on_method_not_allowed(true)
                .fallback(fallback.clone());

            app.merge(filigree::route_services::serve_immutable_files(&web_dir))
                .route_service("/", fallback)
                .fallback_service(serve_fs)
        }
        (Some(web_port), None) => {
            let fallback = filigree::route_services::ForwardRequest::new(format!(
                "http://localhost:{web_port}"
            ));

            app.fallback_service(fallback)
        }
        (None, Some(web_dir)) => {
            let serve_fs = tower_http::services::ServeDir::new(&web_dir)
                .precompressed_gzip()
                .precompressed_br()
                .append_index_html_on_directories(true)
                .fallback(crate::pages::not_found::not_found_fallback.with_state(state.clone()));

            app.merge(filigree::route_services::serve_immutable_files(&web_dir))
                .fallback_service(serve_fs)
        }
        (None, None) => app,
    };

    let app = app.with_state(state.clone()).layer(
        ServiceBuilder::new()
            .layer(panic_handler(production))
            .layer(ObfuscateErrorLayer::new(ObfuscateErrorLayerSettings {
                enabled: obfuscate_errors,
                ..Default::default()
            }))
            .set_x_request_id(MakeRequestUuidV7)
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

                        let request_id = req
                            .headers()
                            .get("X-Request-Id")
                            .and_then(|s| s.to_str().ok())
                            .unwrap_or("");

                        tracing::info_span!("request", ?request_id, %method, %uri, route)
                    })
                    .on_response(
                        DefaultOnResponse::new()
                            .level(Level::INFO)
                            .latency_unit(tower_http::LatencyUnit::Millis),
                    )
                    .on_request(DefaultOnRequest::new().level(Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
            )
            .layer(TimeoutLayer::new(config.request_timeout))
            .layer(api_cors_layer)
            .layer(tower_cookies::CookieManagerLayer::new())
            .propagate_x_request_id()
            .layer(CompressionLayer::new())
            .layer(filigree::auth::middleware::AuthLayer::new(auth_queries))
            .into_inner(),
    );

    let app = if livereload {
        // Attach the livereload status route outside all the middleware so it doesn't generate
        // traces and logs.
        use tokio_stream::StreamExt;
        app.route(
            "/__livereload_status",
            axum::routing::get(|| async {
                let stream = futures::stream::repeat_with(|| {
                    Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default().data("live"),
                    )
                })
                .throttle(std::time::Duration::from_secs(30));
                axum::response::sse::Sse::new(stream)
            }),
        )
    } else {
        app
    };

    let listener = match config.bind {
        ServerBind::Listener(l) => l,
        ServerBind::HostPort(host, port) => create_tcp_listener(&host, port).await?,
    };

    let actual_addr = listener.local_addr().change_context(Error::ServerStart)?;
    let port = actual_addr.port();
    let host = actual_addr.ip().to_string();
    event!(Level::INFO, "Listening on {host}:{port}");

    Ok(Server {
        host,
        port,
        app,
        state,
        listener,
        manifest_watcher,
    })
}
