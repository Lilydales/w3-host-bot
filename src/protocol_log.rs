//! TEMPORARY diagnostic logger for the Warcraft III local UI WebSocket.
//! Remove: delete this file, `pub mod protocol_log;` in lib.rs and the
//! `protocol_log::` calls in transport.rs. Disable without removing: set
//! `W3_PROTOCOL_LOG=0` before starting. Log file: `w3-protocol.log` in the
//! current working directory (override with `W3_PROTOCOL_LOG_PATH`).
use serde_json::Value;
use std::{fs::OpenOptions, io::Write, sync::Mutex, time::SystemTime};

static FILE_LOCK: Mutex<()> = Mutex::new(());

const SECRET_WORDS: &[&str] = &[
    "password", "passwd", "token", "cookie", "secret", "authorization",
    "accesstoken", "refreshtoken", "apikey", "api_key", "sessionkey", "ticket",
];

fn enabled() -> bool {
    std::env::var("W3_PROTOCOL_LOG").map_or(true, |v| v != "0")
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
            (kind, serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.into()))
        }
        Err(e) => ("<NON-JSON>".to_owned(), format!("(invalid JSON: {e})\n{raw}")),
    };
    let ts = timestamp();
    let line = "=".repeat(60);
    let entry = format!(
        "{line}\n{ts}\nDIRECTION: {direction}\nTYPE: {kind}\n\n{body}\n{line}\n"
    );
    println!(
        "[W3-PROTOCOL][{}][{direction}]\nType: {kind}\nRaw:\n{body}",
        &ts[11..]
    );
    let path = std::env::var("W3_PROTOCOL_LOG_PATH").unwrap_or_else(|_| "w3-protocol.log".into());
    let _guard = FILE_LOCK.lock();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(entry.as_bytes());
    }
}

/// Non-text frames (binary, ping, pong, close) are logged by kind only.
pub fn log_note(direction: &str, note: &str) {
    log(direction, &serde_json::json!({"messageType": note}).to_string());
}
