//! Reads Warcraft III replays (`.w3g`) for the facts the game's UI socket never sends:
//! who left a match, and when.
//!
//! Only the parts needed are decoded: the player list, the slot table (to skip
//! observers) and the leave records. Unknown blocks stop the scan without failing, so a
//! newer replay format still yields the players and any leaves read so far.
//!
//! Format notes (checked against a real 3.0 / build 10200 replay):
//! - Header: 28-byte magic, then at 0x1c `header size, compressed size, header version,
//!   decompressed size, block count` (u32 each). Build is the u32 at 0x34.
//! - Blocks: builds >= 10032 use a 12-byte block header (u32 size, u32 size, u32
//!   checksum), older ones 8 bytes (u16, u16, u32). Each block is a zlib stream that is
//!   flushed, not finished, so it is inflated without requiring a stream end.
//! - Data: 4 unknown bytes, the first player record, game name, an empty string, the
//!   encoded settings string, player count/game type/language (u32 x3), more player
//!   records (0x16, each followed by 4 bytes), optional 0x39 metadata sections, the slot
//!   table (0x19), then the action stream.
//! - Action stream: 0x17 leave (u32 reason, u8 player, u32 result, u32 unknown),
//!   0x1E/0x1F time slots (u16 length, u16 elapsed ms), 0x20 chat, 0x1A-0x1C, 0x22,
//!   0x23, 0x2F fixed-size records.
use flate2::{Decompress, FlushDecompress};
use std::{collections::BTreeMap, path::Path};

pub const FILE_NAME: &str = "LastReplay.w3g";
const MAGIC: &[u8] = b"Warcraft III recorded game\x1a\0";
const OBSERVER_TEAM: u8 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leave {
    pub name: String,
    /// Milliseconds after the match started.
    pub at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    /// Playing players (observers excluded), in replay order.
    pub players: Vec<String>,
    /// Every recorded departure of a playing player, in order.
    pub leaves: Vec<Leave>,
    /// Match length in milliseconds (the last time recorded in the replay).
    pub length_ms: u64,
}

impl Summary {
    /// Players who left at least `grace_ms` before the replay ends. At the end of a
    /// match everyone is recorded as leaving at the final time; those are not leavers.
    pub fn early_leavers(&self, grace_ms: u64) -> Vec<&Leave> {
        self.leaves
            .iter()
            .filter(|leave| leave.at_ms + grace_ms < self.length_ms)
            .collect()
    }
}

/// "m:ss" for a millisecond time.
pub fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn read(path: &Path) -> Result<Summary, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("The replay could not be read: {e}"))?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Summary, String> {
    let data = decompress(bytes)?;
    parse_data(&data)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn decompress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let bad = || "This is not a Warcraft III replay.".to_string();
    if !bytes.starts_with(MAGIC) {
        return Err(bad());
    }
    let header = u32_at(bytes, 0x1c).ok_or_else(bad)? as usize;
    let total = u32_at(bytes, 0x28).ok_or_else(bad)? as usize;
    let blocks = u32_at(bytes, 0x2c).ok_or_else(bad)? as usize;
    let build = u32_at(bytes, 0x34).unwrap_or(0);
    if total > 256 * 1024 * 1024 || blocks > 1_000_000 {
        return Err("The replay is too large.".into());
    }
    let wide = build >= 10032;
    let mut out = Vec::with_capacity(total);
    let mut at = header;
    for _ in 0..blocks {
        let (packed, unpacked, skip) = if wide {
            (
                u32_at(bytes, at).ok_or_else(bad)? as usize,
                u32_at(bytes, at + 4).ok_or_else(bad)? as usize,
                12,
            )
        } else {
            (
                u16_at(bytes, at).ok_or_else(bad)? as usize,
                u16_at(bytes, at + 2).ok_or_else(bad)? as usize,
                8,
            )
        };
        at += skip;
        let block = bytes.get(at..at + packed).ok_or_else(bad)?;
        at += packed;
        let mut inflate = Decompress::new(true);
        let mut chunk = Vec::with_capacity(unpacked.max(1));
        inflate
            .decompress_vec(block, &mut chunk, FlushDecompress::Sync)
            .map_err(|_| "A replay block could not be decompressed.".to_string())?;
        out.extend_from_slice(&chunk);
    }
    out.truncate(total.min(out.len()));
    Ok(out)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(value)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        (self.at + count <= self.bytes.len()).then(|| self.at += count)
    }

    fn text(&mut self) -> Option<String> {
        let rest = self.bytes.get(self.at..)?;
        let end = rest.iter().position(|b| *b == 0)?;
        self.at += end + 1;
        Some(String::from_utf8_lossy(&rest[..end]).into_owned())
    }

    /// Record id, player id, name, then a length-prefixed extra block.
    fn player(&mut self) -> Option<(u8, String)> {
        self.byte()?;
        let id = self.byte()?;
        let name = self.text()?;
        let extra = self.byte()? as usize;
        self.skip(extra)?;
        Some((id, name))
    }
}

fn parse_data(data: &[u8]) -> Result<Summary, String> {
    let broken = || "The replay header could not be read.".to_string();
    let mut c = Cursor { bytes: data, at: 4 };
    let mut names = BTreeMap::new();
    let (id, name) = c.player().ok_or_else(broken)?;
    names.insert(id, name);
    c.text().ok_or_else(broken)?; // game name
    c.text().ok_or_else(broken)?; // empty
    c.text().ok_or_else(broken)?; // encoded settings
    c.skip(12).ok_or_else(broken)?;
    while c.peek() == Some(0x16) {
        let (id, name) = c.player().ok_or_else(broken)?;
        names.insert(id, name);
        c.skip(4).ok_or_else(broken)?;
    }
    while c.peek() == Some(0x39) {
        c.skip(2).ok_or_else(broken)?;
        let length = u32_at(data, c.at).ok_or_else(broken)? as usize;
        c.skip(4 + length).ok_or_else(broken)?;
    }
    if c.byte() != Some(0x19) {
        return Err(broken());
    }
    let size = u16_at(data, c.at).ok_or_else(broken)? as usize;
    let table_end = c.at + 2 + size;
    let slots = *data.get(c.at + 2).ok_or_else(broken)? as usize;
    let mut observers = std::collections::BTreeSet::new();
    let mut playing = Vec::new();
    for slot in 0..slots {
        let at = c.at + 3 + slot * 9;
        let Some(record) = data.get(at..at + 9) else {
            break;
        };
        let (id, status, team) = (record[0], record[2], record[4]);
        if status != 2 {
            continue;
        }
        if team == OBSERVER_TEAM {
            observers.insert(id);
        } else if let Some(name) = names.get(&id) {
            playing.push(name.clone());
        }
    }
    c.at = table_end;
    let mut summary = Summary {
        players: playing,
        ..Summary::default()
    };
    let mut time = 0u64;
    while let Some(kind) = c.peek() {
        let advanced = match kind {
            0x17 => {
                let id = data.get(c.at + 5).copied();
                if let Some(id) = id
                    && !observers.contains(&id)
                    && let Some(name) = names.get(&id)
                {
                    summary.leaves.push(Leave {
                        name: name.clone(),
                        at_ms: time,
                    });
                }
                c.skip(14)
            }
            0x1A..=0x1C => c.skip(5),
            0x1E | 0x1F => match (u16_at(data, c.at + 1), u16_at(data, c.at + 3)) {
                (Some(length), Some(elapsed)) => {
                    time += u64::from(elapsed);
                    c.skip(3 + length as usize)
                }
                _ => None,
            },
            0x20 => u16_at(data, c.at + 2).and_then(|length| c.skip(4 + length as usize)),
            0x22 => data
                .get(c.at + 1)
                .copied()
                .and_then(|length| c.skip(2 + length as usize)),
            0x23 => c.skip(11),
            0x2F => c.skip(9),
            _ => None, // Padding (0) or an unknown block: keep what was read.
        };
        if advanced.is_none() {
            break;
        }
    }
    summary.length_ms = time;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn record(kind: u8, id: u8, name: &str) -> Vec<u8> {
        let mut out = vec![kind, id];
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&[0, 1, 0]);
        out
    }

    fn slot(id: u8, team: u8) -> [u8; 9] {
        [id, 100, 2, 0, team, 0, 0x20, 1, 100]
    }

    fn time(ms: u16) -> Vec<u8> {
        let mut out = vec![0x1F];
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&ms.to_le_bytes());
        out
    }

    fn leave(id: u8) -> Vec<u8> {
        let mut out = vec![0x17];
        out.extend_from_slice(&1u32.to_le_bytes());
        out.push(id);
        out.extend_from_slice(&7u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    /// A small replay in the same layout as a real 3.0 build 10200 replay.
    fn replay() -> Vec<u8> {
        let mut data = vec![0x10, 0x01, 0, 0];
        data.extend(record(0, 1, "Host#1"));
        data.extend_from_slice(b"w3c-game\0\0settings\0");
        data.extend_from_slice(&[0; 12]);
        for (id, name) in [(2, "Quitter#2"), (3, "Stayer#3"), (24, "FLO")] {
            data.extend(record(0x16, id, name));
            data.extend_from_slice(&[0; 4]);
        }
        let slots = [slot(1, 0), slot(2, 1), slot(3, 0), slot(24, 24)];
        data.push(0x19);
        data.extend_from_slice(&((1 + slots.len() * 9 + 6) as u16).to_le_bytes());
        data.push(slots.len() as u8);
        for s in slots {
            data.extend_from_slice(&s);
        }
        data.extend_from_slice(&[0; 6]);
        let mut chat = vec![0x20, 2];
        chat.extend_from_slice(&4u16.to_le_bytes());
        chat.extend_from_slice(&[0x10, b'g', b'g', 0]);
        // 4:00 of play, Quitter leaves, 6 more minutes, then everyone leaves at the end.
        for _ in 0..8 {
            data.extend(time(30_000));
        }
        data.extend(chat);
        data.extend(leave(2));
        for _ in 0..12 {
            data.extend(time(30_000));
        }
        for id in [24, 3, 1] {
            data.extend(leave(id));
        }
        data.extend_from_slice(&[0; 16]);
        let mut packed = Vec::new();
        for chunk in data.chunks(8192) {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(chunk).unwrap();
            let body = encoder.finish().unwrap();
            packed.extend_from_slice(&(body.len() as u32).to_le_bytes());
            packed.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            packed.extend_from_slice(&0u32.to_le_bytes());
            packed.extend(body);
        }
        let mut file = MAGIC.to_vec();
        for value in [68u32, 0, 1, data.len() as u32, data.chunks(8192).count() as u32] {
            file.extend_from_slice(&value.to_le_bytes());
        }
        file.extend_from_slice(b"PX3W");
        file.extend_from_slice(&10200u32.to_le_bytes());
        file.resize(68, 0);
        file.extend(packed);
        file
    }

    #[test]
    fn players_leaves_and_length_are_read_and_observers_skipped() {
        let summary = parse(&replay()).unwrap();
        assert_eq!(summary.players, ["Host#1", "Quitter#2", "Stayer#3"]);
        assert_eq!(summary.length_ms, 600_000);
        assert_eq!(summary.leaves[0], Leave { name: "Quitter#2".into(), at_ms: 240_000 });
        assert!(!summary.leaves.iter().any(|l| l.name == "FLO"));
        let early: Vec<_> = summary.early_leavers(3_000).iter().map(|l| l.name.clone()).collect();
        assert_eq!(early, ["Quitter#2"], "players leaving at the very end are not leavers");
        assert_eq!(clock(240_000), "4:00");
    }

    #[test]
    fn other_files_are_rejected_and_a_cut_replay_keeps_what_was_read() {
        assert!(parse(b"not a replay").is_err());
        let mut cut = replay();
        cut.truncate(cut.len() - 1);
        assert!(parse(&cut).is_err(), "a cut block is an error, never a guessed result");
    }
}
