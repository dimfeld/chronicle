use chronicle_api::{cmd, db, emails, server, Error};
use clap::{Args, Parser, Subcommand};
use error_stack::{Report, ResultExt};
use filigree::{
    auth::{CorsSetting, SameSiteArg, SessionCookieBuilder},
    tracing_config::{configure_tracing, teardown_tracing, TracingProvider},
};
use tracing::{event, Level};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Util(cmd::util::UtilCommand),
    Db(cmd::db::DbCommand),
    Serve(ServeCommand),
}

#[derive(Args, Debug)]
struct ServeCommand {
    /// The PostgreSQL database to connect to
    #[clap(long = "db", env = "DATABASE_URL")]
    database_url: String,

    /// The IP host to bind to
    #[clap(long, env = "HOST", default_value_t = String::from("::1"))]
    host: String,

    /// The TCP port to listen on
    #[clap(long, env = "PORT", default_value_t = 9782)]
    port: u16,

    /// The port to forward non-API frontend requests to
    #[clap(long, env = "WEB_PORT")]
    frontend_port: Option<u16>,

    /// Serve frontend static assets from this directory
    #[clap(long, env = "WEB_ASSET_DIR")]
    frontend_asset_dir: Option<String>,

    /// Path to the frontend manifest file. Defaults to "<frontend_asset_dir>/.vite/manifest.json"
    /// if frontend_asset_dir is set.
    #[clap(long, env = "VITE_MANIFEST")]
    vite_manifest: Option<String>,

    /// Watch the filesystem for changes and enable live reload
    #[clap(long, env = "DEV")]
    dev: bool,

    /// The environment in which this server is running
    #[clap(long = "env", env = "ENV", default_value_t = String::from("development"))]
    env: String,

    /// Request timeout, in seconds
    #[clap(long, env = "REQUEST_TIMEOUT", default_value_t = 60)]
    request_timeout: u64,

    #[clap(long, env = "COOKIE_SAME_SITE", value_enum, default_value_t = SameSiteArg::Strict)]
    cookie_same_site: SameSiteArg,

    /// Set if the site is being accessed over HTTP
    #[clap(long, env = "INSECURE")]
    insecure: bool,

    /// Session expiry time, in days
    #[clap(long, env = "SESSION_EXPIRY", default_value_t = 14)]
    session_expiry: u64,

    /// Maintain at least this many connections to the database.
    #[clap(long, env = "DB_MIN_CONNECTIONS", default_value_t = 0)]
    db_min_connections: u32,

    /// Create no more than this many connections to the database.
    #[clap(long, env = "DB_MAX_CONNECTIONS", default_value_t = 100)]
    db_max_connections: u32,
    /// The email service to use
    #[clap(long, env="EMAIL_SENDER_SERVICE", default_value_t = String::from("none"))]
    email_sender_service: String,

    /// The API token for the email sending service
    #[clap(long, env="EMAIL_SENDER_API_TOKEN", default_value_t = String::from(""))]
    email_sender_api_token: String,

    /// The email address to use as the default sender
    #[clap(long, env="EMAIL_DEFAULT_FROM_ADDRESS", default_value_t = String::from("support@example.com"))]
    email_default_from_address: String,

    /// Allow users to sign up themselves
    #[clap(long, env = "ALLOW_PUBLIC_SIGNUP", default_value_t = true)]
    allow_public_signup: bool,

    /// Allow users to invite people to their team
    #[clap(long, env = "ALLOW_INVITE_TO_SAME_ORG", default_value_t = true)]
    allow_invite_to_same_org: bool,

    /// Allow users to invite people to the app, in their own new team
    #[clap(long, env = "ALLOW_INVITE_TO_NEW_ORG", default_value_t = true)]
    allow_invite_to_new_org: bool,

    /// Require email verification when inviting a user to the same org
    #[clap(
        long,
        env = "SAME_ORG_INVITES_REQUIRE_EMAIL_VERIFICATION",
        default_value_t = true
    )]
    same_org_invites_require_email_verification: bool,

    /// The hosts that this server can be reached from
    #[clap(long, env = "HOSTS")]
    hosts: Option<Vec<String>>,

    /// CORS configuration
    #[clap(long, env="API_CORS", value_enum, default_value_t = CorsSetting::None)]
    api_cors: CorsSetting,

    /// The base URL for OAuth redirect URLs. If omitted, `hosts[0]` is used.
    #[clap(long, env = "OAUTH_REDIRECT_URL_BASE")]
    oauth_redirect_host: Option<String>,

    /// Whether or not to obfuscate details from internal server errors. If omitted,
    /// the default is to obfuscate when env != "development".
    #[clap(long, env = "OBFUSCATE_ERRORS")]
    obfuscate_errors: Option<bool>,
}

async fn serve(cmd: ServeCommand) -> Result<(), Report<Error>> {
    error_stack::Report::set_color_mode(error_stack::fmt::ColorMode::None);

    let tracing_config = filigree::tracing_config::create_tracing_config(
        "",
        TracingProvider::None,
        Some("chronicle-api".to_string()),
        None,
    )
    .change_context(Error::ServerStart)?;

    configure_tracing(
        "",
        tracing_config,
        tracing_subscriber::fmt::time::ChronoUtc::rfc_3339(),
        std::io::stdout,
    )
    .change_context(Error::ServerStart)?;

    let pool_options = sqlx::postgres::PgPoolOptions::new()
        .min_connections(cmd.db_min_connections)
        .max_connections(cmd.db_max_connections);

    let pg_pool = if cmd.db_min_connections > 0 {
        pool_options.connect(&cmd.database_url).await
    } else {
        pool_options.connect_lazy(&cmd.database_url)
    };

    let pg_pool = pg_pool.change_context(Error::Db)?;

    db::run_migrations(&pg_pool).await?;

    let secure_cookies = !cmd.insecure;

    let email_service = filigree::email::services::email_service_from_name(
        &cmd.email_sender_service,
        cmd.email_sender_api_token,
    );
    let email_sender = filigree::email::services::EmailSender::new(
        cmd.email_default_from_address,
        emails::create_tera(),
        email_service,
    );

    let hosts = cmd.hosts.unwrap_or_else(|| vec!["localhost".to_string()]);

    let oauth_redirect_host = cmd.oauth_redirect_host.unwrap_or_else(|| {
        format!(
            "{}://{}",
            if cmd.insecure { "http" } else { "https" },
            hosts[0]
        )
    });

    let frontend_asset_dir = cmd
        .frontend_asset_dir
        .or_else(|| Some("web/build".to_string()));
    let vite_manifest = cmd.vite_manifest.or_else(|| {
        frontend_asset_dir
            .as_ref()
            .map(|base| format!("{base}/.vite/manifest.json"))
    });

    let server = server::create_server(server::Config {
        env: cmd.env,
        bind: server::ServerBind::HostPort(cmd.host, cmd.port),
        serve_frontend: server::ServeFrontend {
            port: cmd.frontend_port,
            path: frontend_asset_dir,
            vite_manifest,
            watch_vite_manifest: cmd.dev,
            livereload: cmd.dev,
        },
        insecure: cmd.insecure,
        request_timeout: std::time::Duration::from_secs(cmd.request_timeout),
        cookie_configuration: SessionCookieBuilder::new(secure_cookies, cmd.cookie_same_site),
        session_expiry: filigree::auth::ExpiryStyle::AfterIdle(std::time::Duration::from_secs(
            cmd.session_expiry * 24 * 60 * 60,
        )),
        email_sender,
        hosts,
        api_cors: cmd.api_cors,
        obfuscate_errors: cmd.obfuscate_errors,
        // This will build OAuth providers based on the environment variables present.
        oauth_providers: None,
        oauth_redirect_url_base: oauth_redirect_host,
        new_user_flags: filigree::server::NewUserFlags {
            allow_public_signup: cmd.allow_public_signup,
            allow_invite_to_same_org: cmd.allow_invite_to_same_org,
            allow_invite_to_new_org: cmd.allow_invite_to_new_org,
            same_org_invites_require_email_verification: cmd
                .same_org_invites_require_email_verification,
        },
        pg_pool,
        secrets: server::Secrets::from_env()?,
    })
    .await?;

    server.run().await?;

    event!(Level::INFO, "Exporting remaining traces");
    teardown_tracing().await.change_context(Error::Shutdown)?;
    event!(Level::INFO, "Trace shut down complete");

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
    let read_dotenv = std::env::var("READ_DOTENV")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);

    if read_dotenv {
        dotenvy::dotenv().ok();
    }

    let cli = Cli::parse();

    match cli.command {
        Command::Db(cmd) => cmd.handle().await?,
        Command::Serve(cmd) => serve(cmd).await?,
        Command::Util(cmd) => cmd.handle().await?,
    }

    Ok(())
}
