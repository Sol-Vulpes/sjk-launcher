//! Sith cloud: serves a folder of PK3s as a content repository in JoF's
//! manifest format, so the launcher (and SJK's `sjk-jof-content`) can read it.
//!
//! ```text
//! <root>/
//!   groups.json            optional: names, descriptions and types of the groups
//!   repository/
//!     maps/  *.pk3         one folder per group
//!     skins/ *.pk3
//! ```
//!
//! - `GET /manifest.json`: `{"data":{"assetgroups":[...]}}`, every PK3 with its
//!   size and MD5. Hashes are cached in `<root>/.sith-cloud-md5.json` by size
//!   and modification time, so a new or changed file is the only one hashed again.
//! - `GET /repository/<group>/<file>`: the file, with `Range` for resumed downloads.
//!
//! Drop files in with scp; the next manifest request (at most every 10 s) sees them.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const CACHE: &str = ".sith-cloud-md5.json";
const RESCAN: Duration = Duration::from_secs(10);

/// One group's settings in `groups.json`, keyed by folder.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct GroupSettings {
    name: Option<String>,
    description: String,
    /// `REQUIRED`, `RECOMMENDED` (the default) or `OPTIONAL`.
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Files' descriptions, by file name.
    files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Hashed {
    size: u64,
    modified: u64,
    md5: String,
}

struct Repository {
    root: PathBuf,
    cache: BTreeMap<String, Hashed>,
    manifest: Vec<u8>,
    scanned: Option<Instant>,
}

impl Repository {
    fn files(&self) -> PathBuf {
        self.root.join("repository")
    }

    /// The manifest, scanning the folder again when the last scan is old.
    fn manifest(&mut self) -> Vec<u8> {
        if self.scanned.is_none_or(|at| at.elapsed() > RESCAN) {
            self.scan();
        }
        self.manifest.clone()
    }

    fn scan(&mut self) {
        let settings: BTreeMap<String, GroupSettings> = std::fs::read(self.root.join("groups.json"))
            .ok()
            .and_then(|bytes| match serde_json::from_slice(&bytes) {
                Ok(settings) => Some(settings),
                Err(error) => {
                    eprintln!("groups.json: {error}");
                    None
                }
            })
            .unwrap_or_default();
        let mut groups = Vec::new();
        let mut seen = BTreeMap::new();
        for folder in sorted_entries(&self.files()).into_iter().filter(|path| path.is_dir()) {
            let Some(folder_name) = plain(&folder) else { continue };
            let group = settings.get(&folder_name).cloned().unwrap_or_default();
            let mut assets = Vec::new();
            for file in sorted_entries(&folder) {
                let Some(name) = plain(&file) else { continue };
                if !file.is_file() || !name.to_ascii_lowercase().ends_with(".pk3") {
                    continue;
                }
                let key = format!("{folder_name}/{name}");
                let Some(hashed) = self.hash(&key, &file) else { continue };
                assets.push(serde_json::json!({
                    "id": stable_id(&key),
                    "name": name,
                    "size": hashed.size,
                    "hash": hashed.md5,
                    "description": group.files.get(&name).cloned().unwrap_or_default(),
                    "targetfolder": "Base",
                    "localfolder": folder_name,
                }));
                seen.insert(key, hashed);
            }
            groups.push(serde_json::json!({
                "id": stable_id(&folder_name),
                "name": group.name.clone().unwrap_or_else(|| folder_name.clone()),
                "description": group.description,
                "type": group.kind.clone().unwrap_or_else(|| "RECOMMENDED".into()).to_ascii_uppercase(),
                "assets": assets,
            }));
        }
        if seen != self.cache {
            self.cache = seen;
            if let Ok(bytes) = serde_json::to_vec_pretty(&self.cache) {
                let _ = std::fs::write(self.root.join(CACHE), bytes);
            }
        }
        let manifest = serde_json::json!({ "status": "OK", "data": { "assetgroups": groups } });
        self.manifest = serde_json::to_vec(&manifest).unwrap_or_default();
        self.scanned = Some(Instant::now());
    }

    /// The file's MD5, from the cache while its size and time are unchanged.
    fn hash(&self, key: &str, path: &Path) -> Option<Hashed> {
        let meta = path.metadata().ok()?;
        let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
        if let Some(cached) = self.cache.get(key)
            && cached.size == meta.len()
            && cached.modified == modified
        {
            return Some(cached.clone());
        }
        let started = Instant::now();
        let mut file = File::open(path).ok()?;
        let mut hasher = Md5::new();
        std::io::copy(&mut file, &mut hasher).ok()?;
        let md5: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        println!("hashed {key} in {:.1}s", started.elapsed().as_secs_f32());
        Some(Hashed { size: meta.len(), modified, md5 })
    }
}

fn sorted_entries(folder: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    entries
}

/// The last component's name when it is one the manifest accepts.
fn plain(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_owned();
    plain_name(&name).then_some(name)
}

/// The same rule as `sjk-jof-content`: no separators, no `..`, nothing Windows refuses.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name != "."
        && name != ".."
        && !name.ends_with('.')
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// An id that stays the same while the folder or file keeps its name, so the
/// launcher's picks survive rescans (31 bits of the name's MD5).
fn stable_id(key: &str) -> u32 {
    let digest = Md5::digest(key.to_ascii_lowercase().as_bytes());
    u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) & 0x7fff_ffff
}

fn percent_decode(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = segment.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("plain ASCII header")
}

/// `bytes=N-` or `bytes=N-M`, clamped to the file.
fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let (start, end) = value.trim().strip_prefix("bytes=")?.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end = match end.trim() {
        "" => size.checked_sub(1)?,
        end => end.parse::<u64>().ok()?.min(size.checked_sub(1)?),
    };
    (start <= end).then_some((start, end))
}

fn serve_file(request: Request, path: &Path) -> std::io::Result<()> {
    let Ok(mut file) = File::open(path) else {
        return request.respond(Response::from_string("not found").with_status_code(404));
    };
    let size = file.metadata()?.len();
    let range = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .and_then(|header| parse_range(header.value.as_str(), size));
    let head = request.method() == &Method::Head;
    let (status, start, length) = match range {
        Some((start, end)) => (206, start, end - start + 1),
        None => (200, 0, size),
    };
    file.seek(SeekFrom::Start(start))?;
    let mut headers = vec![
        header("Content-Type", "application/octet-stream"),
        header("Accept-Ranges", "bytes"),
    ];
    if status == 206 {
        headers.push(header("Content-Range", &format!("bytes {start}-{}/{size}", start + length - 1)));
    }
    let body: Box<dyn Read + Send> = if head { Box::new(std::io::empty()) } else { Box::new(file.take(length)) };
    let response = Response::new(StatusCode(status), headers, body, Some(length as usize), None);
    request.respond(response)
}

fn handle(request: Request, repository: &Mutex<Repository>) -> std::io::Result<()> {
    if !matches!(request.method(), Method::Get | Method::Head) {
        return request.respond(Response::from_string("method not allowed").with_status_code(405));
    }
    let url = request.url().split('?').next().unwrap_or_default().to_owned();
    if url == "/manifest.json" {
        let manifest = repository.lock().unwrap_or_else(|error| error.into_inner()).manifest();
        let response = Response::from_data(manifest).with_header(header("Content-Type", "application/json"));
        return request.respond(response);
    }
    if url == "/" {
        return request.respond(Response::from_string("Sith cloud: /manifest.json"));
    }
    let segments: Option<Vec<String>> = url
        .strip_prefix("/repository/")
        .map(|rest| rest.split('/').map(percent_decode).collect::<Option<Vec<_>>>())
        .unwrap_or(None);
    match segments.as_deref() {
        Some([group, name]) if plain_name(group) && plain_name(name) => {
            let path = repository.lock().unwrap_or_else(|error| error.into_inner()).files().join(group).join(name);
            serve_file(request, &path)
        }
        _ => request.respond(Response::from_string("not found").with_status_code(404)),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_owned))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut address = "0.0.0.0:8080".to_owned();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().map(PathBuf::from).unwrap_or(root),
            "--listen" => address = args.next().unwrap_or(address),
            _ => {
                eprintln!("usage: sith-cloud [--root <folder>] [--listen 0.0.0.0:8080]");
                std::process::exit(2);
            }
        }
    }
    let _ = std::fs::create_dir_all(root.join("repository"));
    let cache = std::fs::read(root.join(CACHE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut repository = Repository { root: root.clone(), cache, manifest: Vec::new(), scanned: None };
    repository.scan();
    let repository = Arc::new(Mutex::new(repository));
    let server = match Server::http(&address) {
        Ok(server) => Arc::new(server),
        Err(error) => {
            eprintln!("cannot listen on {address}: {error}");
            std::process::exit(1);
        }
    };
    println!("Sith cloud serving {} on http://{address}", root.display());
    // A few workers, so one slow download does not hold up the manifest.
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let server = Arc::clone(&server);
            let repository = Arc::clone(&repository);
            std::thread::spawn(move || {
                for request in server.incoming_requests() {
                    let _ = handle(request, &repository);
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_read_as_downloads_send_them() {
        assert_eq!(parse_range("bytes=100-", 1000), Some((100, 999)));
        assert_eq!(parse_range("bytes=0-9", 1000), Some((0, 9)));
        assert_eq!(parse_range("bytes=990-5000", 1000), Some((990, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("lines=1-", 1000), None);
    }

    #[test]
    fn paths_cannot_leave_the_repository() {
        assert_eq!(percent_decode("%7BJoF%7D%27s-GWing.pk3").as_deref(), Some("{JoF}'s-GWing.pk3"));
        assert!(!plain_name(&percent_decode("..").unwrap()));
        assert!(!plain_name(&percent_decode("%2E%2E").unwrap()));
        assert!(!plain_name(&percent_decode("a%2Fb").unwrap()));
        assert!(!plain_name(".sith-cloud-md5.json"));
    }

    #[test]
    fn the_manifest_reads_as_jof_content_reads_it() {
        let root = std::env::temp_dir().join("sith-cloud-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("repository/maps")).unwrap();
        std::fs::write(root.join("repository/maps/{Sith} arena.pk3"), b"pk3").unwrap();
        std::fs::write(root.join("repository/maps/notes.txt"), b"x").unwrap();
        std::fs::write(
            root.join("groups.json"),
            r#"{"maps":{"name":"Sith Maps","description":"Arenas","type":"required"}}"#,
        )
        .unwrap();
        let mut repository = Repository { root: root.clone(), cache: BTreeMap::new(), manifest: Vec::new(), scanned: None };
        let manifest = sjk_jof_content::Manifest::parse(&repository.manifest()).unwrap();
        assert_eq!(manifest.skipped, 0);
        let group = &manifest.groups[0];
        assert_eq!(group.name, "Sith Maps");
        assert_eq!(group.kind, sjk_jof_content::GroupKind::Required);
        assert_eq!(group.assets.len(), 1, "only PK3s");
        let asset = &group.assets[0];
        assert_eq!(asset.md5, <[u8; 16]>::from(Md5::digest(b"pk3")));
        assert_eq!(asset.repository_path, ["maps"]);
        assert!(root.join(CACHE).exists());
    }
}
