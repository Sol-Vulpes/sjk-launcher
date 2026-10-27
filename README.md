# SJK Launcher

A small launcher for Jedi Academy that installs, updates and starts
[Sol JK](https://github.com/Sol-Vulpes/SJK) and
[JoF EternalJK](https://github.com/JediofFreedom/JoF_EJK), and downloads JoF content.

- **Games:** each card shows the version you play and the newest release on GitHub.
  Install, update or switch to any listed release, then hit the big **PLAY**. Archives
  are checked against GitHub's SHA-256 before they're extracted into `GameData`.
- **JoF content:** JoF's maps, skins, hilts and packs from JoF Cloud (then jofhub.eu),
  using SJK's own `sjk-jof-content` crate. Files go in `GameData/SJK/jof`, the same
  folder SJK's JoF content card uses; SJK loads them with *Use JoF content* on. Files
  JoF's launcher already put in `GameData/base` are recognised and not fetched again.
- **Sith cloud:** our own content server (`sith-cloud/`, on the VPS). Same manifest
  format and checks; its files go straight into `GameData/base` so SJK and EternalJK
  both load them. Only files the launcher downloaded from it are ever removed.
- **Tray and autostart:** closing the window keeps the launcher in the tray, and
  *Start with my computer* runs it hidden at login (`--minimized`). It checks GitHub
  every few hours and can install new versions by itself while the game is closed.

`GameData` is found in Steam's libraries, or picked with **Change…**. Settings live in
the platform config folder (`%APPDATA%\SJK\Launcher\config\launcher.json` on Windows).

## Build

```
cargo build --release
```

Linux needs `libgtk-3-dev libxdo-dev libayatana-appindicator3-dev` for the tray.

`cargo test -- --ignored` also asks GitHub for both games' releases and installs both
into a temporary folder.

## Sith cloud server

`sith-cloud` serves a folder as a JoF-style repository on port 8080:

```
C:\ProgramData\SithCloud\
  sith-cloud.exe
  groups.json           names, descriptions and types (REQUIRED/RECOMMENDED/OPTIONAL) per folder
  repository\maps\      one folder per group, PK3s inside
  repository\skins\
```

Add or replace PK3s with scp; the server picks them up within 10 seconds:

```
scp mymap.pk3 sith-cloud:C:/ProgramData/SithCloud/repository/maps/
```

`GET /manifest.json` lists every PK3 with its size and MD5 (cached in
`.sith-cloud-md5.json`); `GET /repository/<group>/<file>` serves it, with `Range` for
resumed downloads. Build it with `cargo build --release -p sith-cloud` (the Windows exe
links the C runtime statically, see `.cargo/config.toml`).