use crate::{commands::CommandSettings, engine::Engine, welcome::Welcome};

/// Settings that the controller can change without replacing the current lobby.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveSettings {
    pub minimum: u8,
    pub full: bool,
    pub rehost: bool,
    pub rehost_delay: u64,
    pub empty_refresh: u64,
    pub commands: CommandSettings,
    pub welcome: Welcome,
    /// Direct Strike filter minimums (0 = off). Changing them re-checks the current lobby.
    pub min_ds_level: u32,
    pub min_ds_matches: u32,
}

impl LiveSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=23).contains(&self.minimum)
            || !(5..=3600).contains(&self.rehost_delay)
            || self.empty_refresh > 86400
            || self.min_ds_level > 100_000
            || self.min_ds_matches > 1_000_000
        {
            return Err("Hosting settings are outside supported bounds.".into());
        }
        self.welcome.validate()
    }

    pub(super) fn apply(&self, engine: &mut Engine, now: u64) -> Result<(), String> {
        self.validate()?;
        engine.update_start_rule(self.minimum, self.full);
        engine.update_rehost(self.rehost, self.rehost_delay, now);
        engine.config.empty_rehost_seconds = self.empty_refresh;
        engine.config.commands = self.commands.clone();
        engine.config.welcome = self.welcome.clone();
        engine.config.min_ds_level = self.min_ds_level;
        engine.config.min_ds_matches = self.min_ds_matches;
        Ok(())
    }
}
