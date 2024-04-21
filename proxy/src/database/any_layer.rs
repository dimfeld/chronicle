use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy)]
pub enum DbAbstraction {
    #[cfg(feature = "sqlite")]
    Sqlite,
    #[cfg(feature = "postgres")]
    Postgres,
}

impl DbAbstraction {
    #[cfg(feature = "any-db")]
    pub fn from_url(u: &url::Url) -> Self {
        let scheme = u.scheme();

        if scheme.starts_with("sqlite") {
            Self::Sqlite
        } else if scheme.starts_with("postgres") {
            Self::Postgres
        } else {
            panic!("Unsupported database scheme: {}", scheme);
        }
    }

    pub fn timestamp_value(&self, datetime: &DateTime<Utc>) -> String {
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite => datetime.timestamp().to_string(),
            #[cfg(feature = "postgres")]
            Self::Postgres => super::Escaped(datetime.to_rfc3339()).to_string(),
        }
    }
}
