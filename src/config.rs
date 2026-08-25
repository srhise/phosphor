//! Settings. A missing or malformed file is replaced with defaults
//! rather than reported: nobody should be shown a dialog about their
//! preferences file on the way into a writing app.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub effects: bool,
    pub dense: bool,
    pub fullscreen: bool,
    pub window: (u32, u32),
    pub recent: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            effects: true,
            dense: false,
            fullscreen: false,
            window: (1080, 810),
            recent: None,
        }
    }
}

fn path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("word").join("config.toml"))
}

pub fn parse(text: &str) -> Config {
    toml::from_str(text).unwrap_or_default()
}

pub fn render(c: &Config) -> String {
    toml::to_string_pretty(c).unwrap_or_default()
}

pub fn load() -> Config {
    path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn save(c: &Config) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(p, render(c));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_authentic_ones() {
        let c = Config::default();
        assert!(c.effects, "the CRT is the point");
        assert!(!c.dense, "80x25 is the default text mode");
        assert!(!c.fullscreen);
        assert_eq!(c.window, (1080, 810), "4:3");
    }

    #[test]
    fn a_config_round_trips() {
        let mut c = Config::default();
        c.effects = false;
        c.dense = true;
        c.window = (1440, 1080);
        c.recent = Some(PathBuf::from("/tmp/a.txt"));
        assert_eq!(parse(&render(&c)), c);
    }

    #[test]
    fn a_corrupt_file_yields_defaults_rather_than_an_error() {
        assert_eq!(parse("this is not toml {{{"), Config::default());
    }

    #[test]
    fn an_empty_file_yields_defaults() {
        assert_eq!(parse(""), Config::default());
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let c = parse("effects = false\nfuture_option = 42\n");
        assert!(!c.effects);
        assert!(!c.dense, "the rest stay at their defaults");
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let c = parse("dense = true\n");
        assert!(c.dense);
        assert!(c.effects, "unspecified keys stay default");
        assert_eq!(c.window, (1080, 810));
    }
}
