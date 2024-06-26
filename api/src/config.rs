use std::path::{Path, PathBuf};

use chronicle_proxy::config::ProxyConfig;
use error_stack::{Report, ResultExt};
use etcetera::BaseStrategy;
use serde::Deserialize;

use crate::error::Error;

#[derive(Deserialize)]
pub struct LocalConfig {
    #[serde(flatten)]
    pub server_config: LocalServerConfig,

    #[serde(flatten)]
    pub proxy_config: ProxyConfig,
}

#[derive(Deserialize)]
pub struct LocalServerConfig {
    /// The path or URL to the database, if a database should be used.
    /// This can either be a file path, an sqlite:// URL, or a postgresql:// URL
    pub database: Option<String>,

    /// The port to listen on
    pub port: Option<u16>,

    /// The IP to bind to.
    pub host: Option<String>,

    /// Set to false to skip loading the .env file alongside this config.
    pub dotenv: Option<bool>,
}

pub fn merge_server_config(configs: &Configs) -> LocalServerConfig {
    let mut output = LocalServerConfig {
        database: None,
        port: None,
        host: None,
        dotenv: None,
    };

    // Apply the global configs, then the CWD configs on top of those
    for config in configs.global.iter().chain(configs.cwd.iter()) {
        if let Some(path) = &config.1.server_config.database {
            let full_path = config.0.join(path);
            output.database = Some(full_path.to_string_lossy().to_string());
        }

        if let Some(host) = &config.1.server_config.host {
            output.host = Some(host.clone());
        }

        if let Some(port) = &config.1.server_config.port {
            output.port = Some(*port);
        }

        if let Some(dotenv) = &config.1.server_config.dotenv {
            output.dotenv = Some(*dotenv);
        }
    }

    output
}

pub struct Configs {
    /// Global config directories that have a chronicle.toml
    pub global: Vec<(PathBuf, LocalConfig)>,
    /// Directories starting from the root directory up to the CWD that have a chronicle.toml and
    /// maybe a .env
    pub cwd: Vec<(PathBuf, LocalConfig)>,
}

pub fn find_configs(directory: Option<String>) -> Result<Configs, Report<Error>> {
    if let Some(directory) = directory {
        let path = PathBuf::from(directory);
        let config = read_config(&path, path.is_dir()).change_context(Error::Config)?;

        let Some(config) = config else {
            return Err(Report::new(Error::Config))
                .attach_printable_lazy(|| format!("No config found in path {}", path.display()));
        };

        return Ok(Configs {
            cwd: vec![config],
            global: vec![],
        });
    }

    let default_configs = find_default_configs()?;
    let cwd_configs = find_current_dir_configs()?;

    Ok(Configs {
        cwd: cwd_configs,
        global: default_configs,
    })
}

fn find_default_configs() -> Result<Vec<(PathBuf, LocalConfig)>, Report<Error>> {
    // search for configs in the .config/chronicle directory, and looking up from the current
    // directory
    let etc = etcetera::base_strategy::choose_native_strategy().unwrap();

    [
        etc.home_dir().join(".config").join("chronicle"),
        etc.config_dir().join("chronicle"),
    ]
    .into_iter()
    .filter_map(|dir| read_config(&dir, true).transpose())
    .collect::<Result<Vec<_>, Report<Error>>>()
}

fn find_current_dir_configs() -> Result<Vec<(PathBuf, LocalConfig)>, Report<Error>> {
    let Ok(current_dir) = std::env::current_dir() else {
        return Ok(Vec::new());
    };

    let mut configs = Vec::new();
    let mut search_dir = Some(current_dir.as_path());

    while let Some(dir) = search_dir {
        let config = read_config(dir, true)?;

        if let Some(config) = config {
            configs.push(config);
        }

        search_dir = dir.parent();
    }

    // Reverse the order so that we'll apply the innermost directory last.
    configs.reverse();

    Ok(configs)
}

fn read_config(
    path: &Path,
    is_directory: bool,
) -> Result<Option<(PathBuf, LocalConfig)>, Report<Error>> {
    let config_path = if is_directory {
        path.join("chronicle.toml")
    } else {
        path.to_path_buf()
    };

    let config_dir = if is_directory {
        path
    } else {
        let Some(p) = config_path.parent() else {
            return Ok(None);
        };

        p
    };

    let Ok(buf) = std::fs::read_to_string(&config_path) else {
        return Ok(None);
    };

    let config = toml::from_str::<LocalConfig>(&buf)
        .change_context(Error::Config)
        .attach_printable_lazy(|| format!("Error in config file {}", config_path.display()))?;
    tracing::info!("Loaded config at {}", config_path.display());
    Ok(Some((PathBuf::from(config_dir), config)))
}
