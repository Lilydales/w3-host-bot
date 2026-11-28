//! Optional sound cues. Put .wav files with these names in the `sounds` folder next to
//! the launcher's data (`%LOCALAPPDATA%\GHostReforged\sounds`). A missing or unreadable
//! file is silent; nothing is bundled.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    HostingStarted,
    LobbyCreated,
    PlayerJoined,
    PlayerLeft,
    Chat,
    GameStarted,
    GameEnded,
    PlayerKicked,
}

impl Sound {
    pub const ALL: [Sound; 8] = [
        Sound::HostingStarted,
        Sound::LobbyCreated,
        Sound::PlayerJoined,
        Sound::PlayerLeft,
        Sound::Chat,
        Sound::GameStarted,
        Sound::GameEnded,
        Sound::PlayerKicked,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            Sound::HostingStarted => "hosting_started.wav",
            Sound::LobbyCreated => "lobby_created.wav",
            Sound::PlayerJoined => "player_joined.wav",
            Sound::PlayerLeft => "player_left.wav",
            Sound::Chat => "chat.wav",
            Sound::GameStarted => "game_started.wav",
            Sound::GameEnded => "game_ended.wav",
            Sound::PlayerKicked => "player_kicked.wav",
        }
    }

    /// The built-in sound shipped inside the exe (`assets/sounds`).
    fn default_bytes(self) -> &'static [u8] {
        match self {
            Sound::HostingStarted => include_bytes!("../../assets/sounds/hosting_started.wav"),
            Sound::LobbyCreated => include_bytes!("../../assets/sounds/lobby_created.wav"),
            Sound::PlayerJoined => include_bytes!("../../assets/sounds/player_joined.wav"),
            Sound::PlayerLeft => include_bytes!("../../assets/sounds/player_left.wav"),
            Sound::Chat => include_bytes!("../../assets/sounds/chat.wav"),
            Sound::GameStarted => include_bytes!("../../assets/sounds/game_started.wav"),
            Sound::GameEnded => include_bytes!("../../assets/sounds/game_ended.wav"),
            Sound::PlayerKicked => include_bytes!("../../assets/sounds/player_kicked.wav"),
        }
    }

    pub fn when(self) -> &'static str {
        match self {
            Sound::HostingStarted => "Host clicked",
            Sound::LobbyCreated => "a new lobby is up",
            Sound::PlayerJoined => "a player joins",
            Sound::PlayerLeft => "a player leaves the lobby",
            Sound::Chat => "someone else chats in the lobby",
            Sound::GameStarted => "the match starts loading",
            Sound::GameEnded => "your match ends",
            Sound::PlayerKicked => "a player is kicked or banned",
        }
    }
}

pub fn folder(data: &Path) -> PathBuf {
    data.join("sounds")
}

/// Remembers which built-in sounds were already put in the folder, so a file you delete
/// (to silence that cue) is not put back on the next start.
const INSTALLED: &str = ".defaults-installed";

/// Creates the folder, copies each built-in sound once (never over your own file), and
/// writes a short README listing the file names. Errors are ignored: sounds are optional.
pub fn prepare(data: &Path) {
    let dir = folder(data);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let marker = dir.join(INSTALLED);
    let done = std::fs::read_to_string(&marker).unwrap_or_default();
    let mut record = done.clone();
    for sound in Sound::ALL {
        let name = sound.file_name();
        if done.lines().any(|line| line == name) {
            continue;
        }
        let path = dir.join(name);
        if path.exists() || std::fs::write(&path, sound.default_bytes()).is_ok() {
            record.push_str(name);
            record.push('\n');
        }
    }
    if record != done {
        let _ = std::fs::write(&marker, record);
    }
    let readme = dir.join("README.txt");
    if !readme.exists() {
        let mut text = String::from(
            "GHost Reforged sounds\r\n\r\nThe default sounds are installed here once. Replace a \
             file with your own .wav (PCM WAV, same name) to change it, or delete it to \
             silence that cue (it is not put back). Missing files are silent.\r\n\r\n",
        );
        for sound in Sound::ALL {
            text.push_str(&format!("{:<22} {}\r\n", sound.file_name(), sound.when()));
        }
        let _ = std::fs::write(readme, text);
    }
}

/// Plays cues, at most one of each kind per `GAP`, so a burst of joins is one sound.
pub struct Sounds {
    dir: PathBuf,
    last: HashMap<Sound, Instant>,
}

const GAP: Duration = Duration::from_millis(700);

impl Sounds {
    pub fn new(data: &Path) -> Self {
        Self {
            dir: folder(data),
            last: HashMap::new(),
        }
    }

    pub fn play(&mut self, sound: Sound, enabled: bool) {
        if !enabled {
            return;
        }
        let now = Instant::now();
        if self.last.get(&sound).is_some_and(|at| now.duration_since(*at) < GAP) {
            return;
        }
        self.last.insert(sound, now);
        let path = self.dir.join(sound.file_name());
        if path.is_file() {
            play_file(&path);
        }
    }
}

#[cfg(windows)]
fn play_file(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SND_NODEFAULT: an unreadable file stays silent instead of the system beep.
    unsafe {
        PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
fn play_file(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_installed_once_and_never_over_your_own_or_after_a_delete() {
        let dir = tempfile::tempdir().unwrap();
        let sounds = folder(dir.path());
        std::fs::create_dir_all(&sounds).unwrap();
        std::fs::write(sounds.join("chat.wav"), b"mine").unwrap();
        prepare(dir.path());
        for sound in Sound::ALL {
            assert!(sounds.join(sound.file_name()).is_file());
        }
        assert_eq!(std::fs::read(sounds.join("chat.wav")).unwrap(), b"mine", "your file is kept");
        assert_eq!(
            std::fs::read(sounds.join("lobby_created.wav")).unwrap(),
            Sound::LobbyCreated.default_bytes()
        );
        std::fs::remove_file(sounds.join("player_left.wav")).unwrap();
        prepare(dir.path());
        assert!(!sounds.join("player_left.wav").exists(), "a deleted sound stays deleted");
    }

    #[test]
    fn missing_files_are_silent_and_the_readme_lists_every_name() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path());
        let readme = std::fs::read_to_string(folder(dir.path()).join("README.txt")).unwrap();
        for sound in Sound::ALL {
            assert!(readme.contains(sound.file_name()));
        }
        // No files: must not panic or beep.
        let mut sounds = Sounds::new(dir.path());
        for sound in Sound::ALL {
            sounds.play(sound, true);
        }
    }
}
