//! Content collections, read through SJK's own `sjk-jof-content` crate: the
//! manifest format, MD5 checks and resumable downloads are the same for both.
//!
//! - **JoF content**: JoF Cloud, then jofhub.eu, into `GameData/SJK/jof`, the
//!   folder SJK's own JoF content card uses (SJK loads it with `jof.content` on).
//! - **Sith cloud**: the friends' own server, into `GameData/base`, so SJK and
//!   EternalJK both load it. Only files the launcher downloaded there are ever
//!   removed; the record of them is `GameData/sjk-jof-content.json`.

use crate::config::Config;
use sjk_jof_content::job::Settings;
use sjk_jof_content::{Selection, Source};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collection {
    Jof,
    Sith,
}

pub const COLLECTIONS: [Collection; 2] = [Collection::Jof, Collection::Sith];

impl Collection {
    pub fn name(self) -> &'static str {
        match self {
            Self::Jof => "JoF content",
            Self::Sith => "Sith cloud",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Self::Jof => {
                "JoF's maps, skins and packs, from JoF Cloud (then jofhub.eu). They go in GameData/SJK/jof, the folder \
                 SJK's own JoF content card uses; SJK loads them when Use JoF content is on in its settings. JoF chooses \
                 and publishes these files and is responsible for them."
            }
            Self::Sith => {
                "Our own assets, from the Sith cloud server. They go straight in GameData/base, so both SJK and \
                 EternalJK load them. Only files downloaded from here are ever removed."
            }
        }
    }

    pub fn settings(self, config: &Config, game_data: &Path) -> Settings {
        let version = format!("launcher-{}", env!("CARGO_PKG_VERSION"));
        match self {
            Self::Jof => Settings {
                root: game_data.join("SJK").join("jof"),
                game_base: Some(game_data.join("base")),
                sources: sjk_jof_content::source::SOURCES.to_vec(),
                version,
            },
            // The root is GameData itself, so its `base` is the game's.
            Self::Sith => Settings {
                root: game_data.to_owned(),
                game_base: None,
                sources: vec![Source {
                    name: "sith",
                    manifest: leak(&format!("{}/manifest.json", config.sith_url.trim_end_matches('/'))),
                    files: leak(&format!("{}/repository", config.sith_url.trim_end_matches('/'))),
                }],
                version,
            },
        }
    }

    pub fn selection(self, config: &Config) -> Selection {
        let (groups, files) = match self {
            Self::Jof => (&config.jof_groups, &config.jof_files),
            Self::Sith => (&config.sith_groups, &config.sith_files),
        };
        Selection::parse(groups).with_files(files)
    }

    pub fn save(self, config: &mut Config, selection: &Selection) {
        let (groups, files) = match self {
            Self::Jof => (&mut config.jof_groups, &mut config.jof_files),
            Self::Sith => (&mut config.sith_groups, &mut config.sith_files),
        };
        *groups = selection.saved();
        *files = selection.saved_files();
    }
}

/// The crate takes static addresses; one per refresh is a few bytes for the run.
fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SITH_TEST_URL=http://127.0.0.1:18080 cargo test -- --ignored sith`: a
    /// sync from a running Sith cloud lands its files in GameData/base, and a
    /// second look finds them installed.
    #[test]
    #[ignore = "needs a Sith cloud server"]
    fn a_sith_sync_fills_the_game_base() {
        use sjk_jof_content::job::{Job, Phase};
        let url = std::env::var("SITH_TEST_URL").expect("SITH_TEST_URL");
        let game_data = std::env::temp_dir().join("sjk-launcher-sith-test");
        let _ = std::fs::remove_dir_all(&game_data);
        let config = Config { sith_url: url, ..Config::default() };
        let wait = |job: &Job| {
            while job.busy() {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            job.progress()
        };
        let mut job = Job::default();
        job.sync(Collection::Sith.settings(&config, &game_data), Collection::Sith.selection(&config));
        let progress = wait(&job);
        let Phase::Finished(outcome) = &progress.phase else { panic!("{:?}", progress.phase) };
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert!(outcome.downloaded > 0);
        let files: Vec<_> = std::fs::read_dir(game_data.join("base")).unwrap().map(|e| e.unwrap().file_name()).collect();
        println!("downloaded {} into base: {files:?}", outcome.downloaded);
        job.refresh(Collection::Sith.settings(&config, &game_data));
        let snapshot = wait(&job).snapshot.unwrap();
        assert!(snapshot.states.iter().flatten().all(|state| state.present()));
    }
}
