use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::DateTime;

use crate::domain::UiText;

pub fn home_dir() -> Result<std::path::PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Dossier personnel introuvable".to_string())
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

pub fn normalize_epoch_ms(value: i64) -> i64 {
    if value > 0 && value < 10_000_000_000 {
        value * 1_000
    } else {
        value
    }
}

pub fn truncate(value: &str, limit: usize) -> String {
    let clean = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= limit {
        return clean;
    }
    let shortened = clean
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    format!("{shortened}…")
}

pub fn project_name(path: &str) -> UiText {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(UiText::from)
        .unwrap_or_else(|| UiText::message("generated.localProject"))
}

pub fn event_timestamp_ms(timestamp: Option<&str>, fallback_ms: i64) -> i64 {
    timestamp
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis())
        .unwrap_or_else(|| normalize_epoch_ms(fallback_ms))
}

pub fn reset_at_ms_from_epoch(timestamp_seconds: Option<i64>) -> Option<i64> {
    timestamp_seconds.map(|value| normalize_epoch_ms(value))
}

pub fn reset_at_ms_from_iso(timestamp: Option<&str>) -> Option<i64> {
    timestamp
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis())
}

pub fn tail_lines(path: &Path, max_bytes: u64) -> Result<Vec<String>, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    let start = size.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|error| error.to_string())?;
    let mut lines = content.lines();
    if start > 0 {
        lines.next();
    }
    Ok(lines.map(str::to_string).collect())
}

pub fn clamp_percent(value: f64) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
}
