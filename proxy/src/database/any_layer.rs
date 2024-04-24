use chrono::{DateTime, Utc};

pub fn timestamp_value(datetime: &DateTime<Utc>) -> String {
    #[cfg(feature = "sqlite")]
    return datetime.timestamp().to_string();
    #[cfg(feature = "postgres")]
    return super::logging::Escaped(datetime.to_rfc3339()).to_string();
}
