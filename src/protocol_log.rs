//! Diagnostic logger for the Warcraft III local UI WebSocket.
//! OFF by default (it grew to ~50 MB per match). Turn on only when investigating:
//! set `W3_PROTOCOL_LOG=1` before starting. When on:
//! - one compact line per frame (no pretty-printing, no console echo),
//! - a frame identical to the previous one of the same type is skipped,
//! - the file is capped at 5 MB and rotated once to `w3-protocol.previous.log`.
//! Log file: `w3-protocol.log` in the current working directory
//! (override with `W3_PROTOCOL_LOG_PATH`).
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    sync::Mutex,
    time::SystemTime,
};

/// Last logged body per (direction, type), to skip repeats.
static LAST: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

const MAX_BYTES: u64 = 5 * 1024 * 1024;

const SECRET_WORDS: &[&str] = &[
    "password", "passwd", "token", "cookie", "secret", "authorization",
    "accesstoken", "refreshtoken", "apikey", "api_key", "sessionkey", "ticket",
];

fn enabled() -> bool {
    std::env::var("W3_PROTOCOL_LOG").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("on"))
}

fn is_secret(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    SECRET_WORDS.iter().any(|w| k.contains(w))
}

fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if is_secret(key) {
                    *v = Value::String("<REDACTED>".into());
                } else {
                    redact(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

// UTC wall clock (no chrono dependency).
fn timestamp() -> String {
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let (secs, ms) = (d.as_secs(), d.subsec_millis());
    let (days, rem) = (secs / 86400, secs % 86400);
    // Civil-from-days (Howard Hinnant).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{ms:03}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Startup cleanup. With logging off, a leftover `w3-protocol.log` (and its rotation)
/// from an older version is deleted; with logging on, an oversized one is rotated now.
/// Returns a line for Activity when something was removed.
pub fn tidy() -> Option<String> {
    let path = std::env::var("W3_PROTOCOL_LOG_PATH").unwrap_or_else(|_| "w3-protocol.log".into());
    let previous = path.trim_end_matches(".log").to_owned() + ".previous.log";
    let size = |p: &str| fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    if !enabled() {
        let total = size(&path) + size(&previous);
        let removed = [&path, &previous]
            .into_iter()
            .filter(|p| fs::remove_file(p.as_str()).is_ok())
            .count();
        return (removed > 0).then(|| {
            format!("Removed old protocol log ({} MB). It is off by default now.", total / (1024 * 1024))
        });
    }
    if size(&path) >= MAX_BYTES {
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&path, &previous);
    }
    None
}

/// `raw` is the exact text frame (RECV: before deserialization; SEND: the
/// serialized frame). Unparseable text is still logged, verbatim.
pub fn log(direction: &str, raw: &str) {
    if !enabled() {
        return;
    }
    let (kind, body) = match serde_json::from_str::<Value>(raw) {
        Ok(mut v) => {
            let kind = v["messageType"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .unwrap_or("<unknown>")
                .to_owned();
            redact(&mut v);
            (kind, v.to_string())
        }
        Err(e) => ("<NON-JSON>".to_owned(), format!("(invalid JSON: {e}) {}", raw.replace(['\r', '\n'], " "))),
    };
    let mut guard = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let last = guard.get_or_insert_with(HashMap::new);
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        body.hash(&mut h);
        h.finish()
    };
    if last.insert(format!("{direction} {kind}"), hash) == Some(hash) {
        return; // same as the previous frame of this type
    }
    let path = std::env::var("W3_PROTOCOL_LOG_PATH").unwrap_or_else(|_| "w3-protocol.log".into());
    if fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_BYTES) {
        let previous = path.trim_end_matches(".log").to_owned() + ".previous.log";
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&path, &previous);
    }
    let entry = format!("{} {direction} {kind} {body}\n", timestamp());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(entry.as_bytes());
    }
}

/// Non-text frames (binary, ping, pong, close) are logged by kind only.
pub fn log_note(direction: &str, note: &str) {
    log(direction, &serde_json::json!({"messageType": note}).to_string());
}
