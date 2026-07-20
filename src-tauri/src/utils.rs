use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Local, TimeZone};

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

pub fn project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Projet local")
        .to_string()
}

pub fn relative_time(timestamp_ms: i64) -> String {
    let elapsed = (now_ms() - normalize_epoch_ms(timestamp_ms)).max(0) / 1_000;
    match elapsed {
        0..=44 => "maintenant".to_string(),
        45..=89 => "il y a 1 min".to_string(),
        90..=3_599 => format!("il y a {} min", elapsed / 60),
        3_600..=86_399 => format!("il y a {} h", elapsed / 3_600),
        _ => format!("il y a {} j", elapsed / 86_400),
    }
}

pub fn elapsed_label(start_ms: i64, end_ms: i64) -> String {
    let elapsed = (normalize_epoch_ms(end_ms) - normalize_epoch_ms(start_ms)).max(0) / 1_000;
    match elapsed {
        0..=59 => "< 1 min".to_string(),
        60..=3_599 => format!("{} min", elapsed / 60),
        3_600..=86_399 => format!(
            "{} h {}",
            elapsed / 3_600,
            format!("{:02}", (elapsed % 3_600) / 60)
        ),
        _ => format!("{} j", elapsed / 86_400),
    }
}

pub fn event_time(timestamp: Option<&str>, fallback_ms: i64) -> String {
    if let Some(value) = timestamp
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Local))
    {
        return value.format("%H:%M").to_string();
    }
    Local
        .timestamp_millis_opt(normalize_epoch_ms(fallback_ms))
        .single()
        .map(|value| value.format("%H:%M").to_string())
        .unwrap_or_default()
}

pub fn reset_label_from_epoch(timestamp_seconds: Option<i64>) -> String {
    let Some(timestamp) = timestamp_seconds else {
        return "réinitialisation non communiquée".to_string();
    };
    let target = Local.timestamp_opt(timestamp, 0).single();
    match target {
        Some(value) => format!("réinitialisation {}", value.format("%a %d à %H:%M")),
        None => "réinitialisation non communiquée".to_string(),
    }
}

pub fn reset_label_from_iso(timestamp: Option<&str>) -> String {
    timestamp
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            format!(
                "réinitialisation {}",
                value.with_timezone(&Local).format("%a %d à %H:%M")
            )
        })
        .unwrap_or_else(|| "réinitialisation non communiquée".to_string())
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
