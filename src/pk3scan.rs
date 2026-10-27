//! Looking inside PK3s (ZIP archives) and flagging what could do harm, on disk
//! or on a server: a remote PK3 is read with HTTP `Range` requests, its
//! central directory and the few small text files worth reading, never whole.
//!
//! What counts:
//! - **Danger:** native code (`.dll`, `.so`, `.exe`…), which old engines load
//!   straight from a PK3; entries that would land outside the game folder
//!   (`..`, absolute paths); archives that expand to absurd sizes (ZIP bombs).
//! - **Warning:** scripts that are not game files (`.bat`, `.ps1`…), encrypted
//!   or nested archives, configs and menus that run risky console commands
//!   (`rcon`, `bind`, `quit`, `connect`…), an `autoexec.cfg`.
//! - **Info:** game code in QVM bytecode (sandboxed by the engine), menus
//!   and configs that replace the stock ones.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// The most of a central directory or a text entry read, against hostile sizes.
const DIRECTORY_LIMIT: u64 = 64 * 1024 * 1024;
const TEXT_LIMIT: u64 = 512 * 1024;
const TAIL: u64 = 65_557;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Warning,
    Danger,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub level: Level,
    /// The entry it is about, or empty for the whole archive.
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    compressed: u64,
    method: u16,
    encrypted: bool,
    local_offset: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub entries: Vec<Entry>,
    pub findings: Vec<Finding>,
    pub archive_size: u64,
    pub expanded_size: u64,
}

impl Report {
    pub fn level(&self) -> Option<Level> {
        self.findings.iter().map(|finding| finding.level).max()
    }

    #[cfg(test)]
    pub fn count(&self, level: Level) -> usize {
        self.findings.iter().filter(|finding| finding.level == level).count()
    }
}

/// Where the archive's bytes come from.
pub trait Source {
    fn len(&self) -> u64;
    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, String>;
}

pub struct LocalFile {
    file: File,
    length: u64,
}

impl LocalFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let length = file.metadata().map_err(|error| error.to_string())?.len();
        Ok(Self { file, length })
    }
}

impl Source for LocalFile {
    fn len(&self) -> u64 {
        self.length
    }

    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, String> {
        let mut buffer = vec![0; length as usize];
        self.file.seek(SeekFrom::Start(offset)).map_err(|error| error.to_string())?;
        self.file.read_exact(&mut buffer).map_err(|error| error.to_string())?;
        Ok(buffer)
    }
}

/// A PK3 on a server that honours `Range` (JoF Cloud, jofhub.eu and Sith cloud do).
pub struct Remote {
    agent: ureq::Agent,
    url: String,
    length: u64,
}

impl Remote {
    pub fn new(agent: ureq::Agent, url: String, length: u64) -> Self {
        Self { agent, url, length }
    }
}

impl Source for Remote {
    fn len(&self) -> u64 {
        self.length
    }

    fn read_at(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, String> {
        if length == 0 {
            return Ok(Vec::new());
        }
        let mut response = self
            .agent
            .get(&self.url)
            .header("Range", &format!("bytes={offset}-{}", offset + length - 1))
            .call()
            .map_err(|error| error.to_string())?;
        if response.status().as_u16() != 206 {
            return Err("the server does not send parts of files".into());
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(length + 1)
            .read_to_vec()
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 != length {
            return Err(format!("asked {length} bytes, got {}", bytes.len()));
        }
        Ok(bytes)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// The central directory's entries, ZIP64 included.
pub fn entries(source: &mut dyn Source) -> Result<Vec<Entry>, String> {
    let bad = || "not a ZIP archive (no central directory)".to_owned();
    let length = source.len();
    let tail_start = length.saturating_sub(TAIL);
    let tail = source.read_at(tail_start, length - tail_start)?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&at| u32_at(&tail, at) == Some(0x0605_4b50))
        .ok_or_else(bad)?;
    let mut count = u64::from(u16_at(&tail, end + 10).ok_or_else(bad)?);
    let mut directory_size = u64::from(u32_at(&tail, end + 12).ok_or_else(bad)?);
    let mut directory_offset = u64::from(u32_at(&tail, end + 16).ok_or_else(bad)?);
    // ZIP64: a locator just before the end record points at the larger one.
    if end >= 20 && u32_at(&tail, end - 20) == Some(0x0706_4b50) {
        let record_offset = u64_at(&tail, end - 12).ok_or_else(bad)?;
        let record = source.read_at(record_offset, 56)?;
        if u32_at(&record, 0) == Some(0x0606_4b50) {
            count = u64_at(&record, 32).ok_or_else(bad)?;
            directory_size = u64_at(&record, 40).ok_or_else(bad)?;
            directory_offset = u64_at(&record, 48).ok_or_else(bad)?;
        }
    }
    if directory_size > DIRECTORY_LIMIT || directory_offset + directory_size > length {
        return Err("the central directory is out of bounds".into());
    }
    let directory = source.read_at(directory_offset, directory_size)?;
    let mut entries = Vec::with_capacity(count.min(100_000) as usize);
    let mut at = 0;
    while at + 46 <= directory.len() && u32_at(&directory, at) == Some(0x0201_4b50) {
        let flags = u16_at(&directory, at + 8).ok_or_else(bad)?;
        let method = u16_at(&directory, at + 10).ok_or_else(bad)?;
        let mut compressed = u64::from(u32_at(&directory, at + 20).ok_or_else(bad)?);
        let mut size = u64::from(u32_at(&directory, at + 24).ok_or_else(bad)?);
        let name_length = usize::from(u16_at(&directory, at + 28).ok_or_else(bad)?);
        let extra_length = usize::from(u16_at(&directory, at + 30).ok_or_else(bad)?);
        let comment_length = usize::from(u16_at(&directory, at + 32).ok_or_else(bad)?);
        let mut local_offset = u64::from(u32_at(&directory, at + 42).ok_or_else(bad)?);
        let name_bytes = directory.get(at + 46..at + 46 + name_length).ok_or_else(bad)?;
        let extra = directory
            .get(at + 46 + name_length..at + 46 + name_length + extra_length)
            .ok_or_else(bad)?;
        // ZIP64 sizes and offset, in that order, for the fields set to all ones.
        let mut field = 0;
        while field + 4 <= extra.len() {
            let id = u16_at(extra, field).unwrap_or(0);
            let data_length = usize::from(u16_at(extra, field + 2).unwrap_or(0));
            if id == 0x0001 {
                let mut value = field + 4;
                for slot in [&mut size, &mut compressed, &mut local_offset] {
                    if *slot == 0xFFFF_FFFF
                        && let Some(wide) = u64_at(extra, value)
                    {
                        *slot = wide;
                        value += 8;
                    }
                }
            }
            field += 4 + data_length;
        }
        entries.push(Entry {
            name: String::from_utf8_lossy(name_bytes).into_owned(),
            size,
            compressed,
            method,
            encrypted: flags & 1 != 0,
            local_offset,
        });
        at += 46 + name_length + extra_length + comment_length;
    }
    Ok(entries)
}

/// An entry's bytes (stored or deflated), up to `limit` of them.
fn read_entry(source: &mut dyn Source, entry: &Entry, limit: u64) -> Result<Vec<u8>, String> {
    if entry.encrypted || entry.size > limit || entry.compressed > limit {
        return Err("too large or encrypted".into());
    }
    let header = source.read_at(entry.local_offset, 30)?;
    if u32_at(&header, 0) != Some(0x0403_4b50) {
        return Err("bad local header".into());
    }
    let skip = 30 + u64::from(u16_at(&header, 26).unwrap_or(0)) + u64::from(u16_at(&header, 28).unwrap_or(0));
    let data = source.read_at(entry.local_offset + skip, entry.compressed)?;
    match entry.method {
        0 => Ok(data),
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(&data[..])
                .take(limit)
                .read_to_end(&mut out)
                .map_err(|error| error.to_string())?;
            Ok(out)
        }
        other => Err(format!("compression method {other}")),
    }
}

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default()
}

/// Console commands a downloaded config or menu has no business running.
const RISKY: &[(&str, &str)] = &[
    ("rcon", "sends server admin commands"),
    ("rconpassword", "sets an admin password"),
    ("unbindall", "removes every key binding"),
    ("bind", "changes key bindings"),
    ("quit", "closes the game"),
    ("connect", "joins a server"),
    ("reconnect", "rejoins a server"),
    ("writeconfig", "writes a config file"),
    ("cl_allowdownload", "changes server downloads"),
    ("fs_game", "switches the mod folder"),
    ("fs_basepath", "moves the game folder"),
    ("fs_homepath", "moves the settings folder"),
    ("sv_pure", "changes pure-server checks"),
    ("vstr", "runs commands stored in a variable"),
    ("exec", "runs another config"),
];

/// The risky commands in a config or menu, one finding per command with its first line.
fn risky_commands(path: &str, text: &str, level: Level, findings: &mut Vec<Finding>) {
    let lower = text.to_ascii_lowercase();
    for (command, why) in RISKY {
        let line = lower.lines().map(str::trim).find(|line| {
            !line.starts_with("//")
                && line
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .any(|word| word == *command)
        });
        if let Some(line) = line {
            let shown: String = line.chars().take(120).collect();
            findings.push(Finding { level, path: path.to_owned(), text: format!("{command}: {why} — {shown}") });
        }
    }
}

/// Look inside the archive and judge it.
pub fn scan(source: &mut dyn Source) -> Result<Report, String> {
    let archive_size = source.len();
    let entries = entries(source)?;
    let mut findings = Vec::new();
    let mut add = |level, path: &str, text: String| findings.push(Finding { level, path: path.to_owned(), text });
    let expanded_size: u64 = entries.iter().map(|entry| entry.size).sum();
    if expanded_size > 8 * 1024 * 1024 * 1024 || (expanded_size > 256 * 1024 * 1024 && expanded_size / archive_size.max(1) > 100) {
        add(
            Level::Danger,
            "",
            format!("expands to {} from {} (a ZIP bomb?)", sjk_jof_content::human_size(expanded_size), sjk_jof_content::human_size(archive_size)),
        );
    }
    let mut texts = Vec::new();
    for entry in &entries {
        let name = entry.name.replace('\\', "/");
        let lower = name.to_ascii_lowercase();
        let file = lower.rsplit('/').next().unwrap_or_default();
        if name.starts_with('/') || name.contains(':') || name.split('/').any(|part| part == "..") {
            add(Level::Danger, &entry.name, "would be written outside the game folder".into());
        }
        if entry.encrypted {
            add(Level::Warning, &entry.name, "encrypted: cannot be checked".into());
        }
        match extension(&lower).as_str() {
            "dll" | "so" | "dylib" | "exe" | "com" | "scr" | "msi" | "sys" | "cpl" | "ocx" => {
                add(Level::Danger, &entry.name, "native program code: it can do anything your account can".into());
            }
            "bat" | "cmd" | "ps1" | "psm1" | "vbs" | "vbe" | "js" | "jse" | "wsf" | "hta" | "jar" | "lnk" | "sh" | "reg" | "py" => {
                add(Level::Warning, &entry.name, "a script, not a game file".into());
            }
            "zip" | "pk3" | "7z" | "rar" | "cab" => add(Level::Warning, &entry.name, "an archive inside the archive: not checked".into()),
            "qvm" => add(Level::Info, &entry.name, "game code as QVM bytecode (runs sandboxed)".into()),
            // A model's sounds.cfg lists its sounds; the game never runs it as commands.
            "cfg" if lower.starts_with("models/") => {}
            "cfg" => {
                let level = if file == "autoexec.cfg" { Level::Warning } else { Level::Info };
                add(level, &entry.name, format!("config file{}", if level == Level::Warning { " run at every start" } else { "" }));
                texts.push((entry.clone(), Level::Warning));
            }
            "menu" => texts.push((entry.clone(), Level::Info)),
            _ => {}
        }
    }
    let menus = texts.iter().filter(|(entry, _)| extension(&entry.name) == "menu").count();
    if menus > 0 {
        add(Level::Info, "", format!("{menus} menu file(s): changes the game's menus"));
    }
    for (entry, level) in texts {
        match read_entry(source, &entry, TEXT_LIMIT) {
            Ok(bytes) => risky_commands(&entry.name, &String::from_utf8_lossy(&bytes), level, &mut findings),
            Err(why) => findings.push(Finding { level: Level::Warning, path: entry.name.clone(), text: format!("could not be read ({why})") }),
        }
    }
    findings.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.path.cmp(&b.path)));
    Ok(Report { entries, findings, archive_size, expanded_size })
}

/// Where to read a manifest file from: on disk when it is here, else from its hosts.
#[derive(Clone, Debug)]
pub enum Location {
    Local(PathBuf),
    Remote { urls: Vec<String>, size: u64 },
}

pub fn scan_location(location: &Location) -> Result<Report, String> {
    match location {
        Location::Local(path) => scan(&mut LocalFile::open(path)?),
        Location::Remote { urls, size } => {
            let agent = crate::github::agent();
            let mut last = "no host".to_owned();
            for url in urls {
                match scan(&mut Remote::new(agent.clone(), url.clone(), *size)) {
                    Ok(report) => return Ok(report),
                    Err(error) => last = error,
                }
            }
            Err(last)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn archive(files: &[(&str, &[u8])]) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("sjk-launcher-scan-{}-{n}.pk3", std::process::id()));
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, body) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn scan_files(files: &[(&str, &[u8])]) -> Report {
        let path = archive(files);
        let report = scan(&mut LocalFile::open(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(path);
        report
    }

    #[test]
    fn a_plain_asset_pack_is_clean() {
        let report = scan_files(&[("models/players/sith/model.glm", b"x"), ("maps/arena.bsp", b"y")]);
        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.level(), None);
    }

    #[test]
    fn native_code_and_escapes_are_dangerous() {
        let report = scan_files(&[("jampgamex86.dll", b"MZ"), ("../../evil.txt", b"x")]);
        assert_eq!(report.count(Level::Danger), 2, "{:?}", report.findings);
    }

    #[test]
    fn configs_are_read_for_risky_commands() {
        let report = scan_files(&[
            ("autoexec.cfg", b"seta name Sith\nbind F1 \"rcon kick all\"\n// quit\n"),
            ("ui/main.menu", b"action { exec \"connect 1.2.3.4\" }"),
        ]);
        let texts: Vec<_> = report.findings.iter().map(|finding| finding.text.as_str()).collect();
        assert!(texts.iter().any(|text| text.starts_with("rcon")), "{texts:?}");
        assert!(texts.iter().any(|text| text.starts_with("bind")));
        assert!(!texts.iter().any(|text| text.starts_with("quit")), "commented out");
        assert!(texts.iter().any(|text| text.starts_with("connect")), "menus too");
        assert_eq!(report.level(), Some(Level::Warning));
    }

    #[test]
    fn not_a_zip_is_an_error() {
        let path = std::env::temp_dir().join("sjk-launcher-scan-not-zip.pk3");
        std::fs::write(&path, b"hello").unwrap();
        assert!(scan(&mut LocalFile::open(&path).unwrap()).is_err());
    }

    /// `cargo test -- --ignored jof_cloud --nocapture`: every file JoF publishes,
    /// read remotely through the mirror; prints what is flagged.
    #[test]
    #[ignore = "reads JoF's whole repository remotely"]
    fn jof_cloud_files_scan_remotely() {
        use sjk_jof_content::source::MIRROR;
        let agent = sjk_jof_content::download::agent("launcher-test");
        let (manifest, _) = sjk_jof_content::download::manifest(&agent, &[MIRROR]).unwrap();
        let assets: Vec<_> = manifest.assets().map(|(_, asset)| asset.clone()).collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let results = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(asset) = assets.get(index) else { return };
                        let location = Location::Remote { urls: vec![MIRROR.file_url(asset)], size: asset.size };
                        results.lock().unwrap().push((asset.name.clone(), scan_location(&location)));
                    }
                });
            }
        });
        let results = results.into_inner().unwrap();
        let failed: Vec<_> = results.iter().filter(|(_, result)| result.is_err()).collect();
        for (name, result) in &results {
            match result {
                Ok(report) if report.level() >= Some(Level::Warning) => {
                    println!("{name}: {:?}", report.level());
                    for finding in report.findings.iter().filter(|finding| finding.level >= Level::Warning) {
                        println!("    {:?} {} {}", finding.level, finding.path, finding.text);
                    }
                }
                Err(error) => println!("{name}: FAILED {error}"),
                _ => {}
            }
        }
        let entries: usize = results.iter().filter_map(|(_, result)| result.as_ref().ok()).map(|report| report.entries.len()).sum();
        println!("{} files, {entries} entries, {} failed", results.len(), failed.len());
        assert!(failed.is_empty());
    }
}