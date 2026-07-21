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
    let read_start = start.saturating_sub(1);
    file.seek(SeekFrom::Start(read_start))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;

    // Drop the partial leading record before decoding so an arbitrary byte offset cannot split UTF-8.
    let complete_lines = if start == 0 {
        bytes.as_slice()
    } else if bytes.first() == Some(&b'\n') {
        &bytes[1..]
    } else {
        let newline = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(bytes.len());
        &bytes[newline..]
    };
    let content = std::str::from_utf8(complete_lines)
        .map_err(|error| format!("Contenu UTF-8 invalide dans {} : {error}", path.display()))?;
    Ok(content.lines().map(str::to_string).collect())
}

pub fn clamp_percent(value: f64) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use super::*;

    fn temporary_file(name: &str, content: &[u8]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agent-deck-{name}-{}-{unique}.jsonl",
            std::process::id()
        ));
        fs::write(&path, content).expect("write temporary file");
        path
    }

    #[test]
    fn tail_lines_discards_a_record_when_offset_splits_utf8() {
        let content = "discard 😀 record\nfirst complete\nsecond complete\n";
        let path = temporary_file("utf8-tail", content.as_bytes());
        let emoji_offset = content.find('😀').expect("emoji offset") as u64;
        let max_bytes = content.len() as u64 - emoji_offset - 1;

        let lines = tail_lines(&path, max_bytes).expect("valid UTF-8 tail");

        fs::remove_file(path).ok();
        assert_eq!(lines, ["first complete", "second complete"]);
    }

    #[test]
    fn tail_lines_reports_invalid_utf8_in_complete_records() {
        let path = temporary_file("invalid-utf8", b"valid\ninvalid \xff\n");

        let error = tail_lines(&path, 1_024).expect_err("invalid UTF-8 should be reported");

        fs::remove_file(path).ok();
        assert!(error.contains("UTF-8 invalide"));
    }
}
