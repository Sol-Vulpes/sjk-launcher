//! Finding Jedi Academy's `GameData`: Steam's libraries first, then the usual
//! install places. The player can always pick the folder by hand.

use std::path::{Path, PathBuf};

/// A folder is `GameData` when it holds `base/assets0.pk3`.
pub fn is_game_data(path: &Path) -> bool {
    path.join("base").join("assets0.pk3").is_file() || path.join("base").join("assets1.pk3").is_file()
}

/// The player picked the game's folder or `base` instead of `GameData`: find it.
pub fn normalise(path: &Path) -> PathBuf {
    for candidate in [path.to_owned(), path.join("GameData"), path.parent().map(Path::to_owned).unwrap_or_default()] {
        if is_game_data(&candidate) {
            return candidate;
        }
    }
    path.to_owned()
}

pub fn detect() -> Option<PathBuf> {
    steam_libraries()
        .into_iter()
        .map(|library| library.join("steamapps").join("common").join("Jedi Academy").join("GameData"))
        .chain(fixed_places())
        .find(|path| is_game_data(path))
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(folder) = std::env::var(variable) {
                roots.push(PathBuf::from(folder).join("Steam"));
            }
        }
    }
    if let Some(home) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned()) {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        roots.push(home.join("Library/Application Support/Steam"));
    }
    roots
}

/// Every Steam library, read from `libraryfolders.vdf` (`"path" "D:\\SteamLibrary"`).
fn steam_libraries() -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for root in steam_roots() {
        libraries.push(root.clone());
        let Ok(text) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) else { continue };
        for line in text.lines() {
            let mut parts = line.split('"').filter(|part| !part.trim().is_empty());
            if parts.next() == Some("path")
                && let Some(path) = parts.next()
            {
                libraries.push(PathBuf::from(path.replace("\\\\", "\\")));
            }
        }
    }
    libraries.dedup();
    libraries
}

fn fixed_places() -> Vec<PathBuf> {
    let mut places = Vec::new();
    #[cfg(windows)]
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(folder) = std::env::var(variable) {
            let folder = PathBuf::from(folder);
            places.push(folder.join("LucasArts/Star Wars Jedi Knight Jedi Academy/GameData"));
            places.push(folder.join("GOG Galaxy/Games/Star Wars Jedi Knight - Jedi Academy/GameData"));
        }
    }
    places.push(PathBuf::from("C:/GOG Games/Star Wars Jedi Knight - Jedi Academy/GameData"));
    places
}
