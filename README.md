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
  both load them. Only files the launcher downloaded from it are ever removed. The
  optional SJK extras pack (`unofficial_sjk_extras.pk3`, in the `extras` group) also
  has a switch of its own at the top of the tab. SJK itself has the same server on its
  Sith cloud card (`sith.downloads`), sharing the launcher's record of what it fetched.
- **SJK chat:** *Chat* in the header (or *SJK chat* in the tray) opens SJK's chat in a
  window of its own, using SJK's `sjk-identity` crate. You are the same player as in
  SJK: the launcher reads `GameData/SJK/identity.key`, `cl_hubUrl` and your `name` from
  SJK's settings, and never makes or changes the key (start SJK once if you have
  none). It follows SJK's chat switch and your muted players (`chat-mutes.txt`). It
  reads the chat only while the window is open, and pauses while a game started from
  the launcher runs, since the game reads it then. Messages go out as
  `SJKLauncher/<version>`, which the hub marks `launcher` so SJK shows the launcher's
  icon beside them.
- **Tray and autostart:** closing the window keeps the launcher in the tray, and
  *Start with my computer* runs it hidden at login (`--minimized`). It checks GitHub
  every few hours and can install new versions by itself while the game is closed.

`GameData` is found in Steam's libraries, or picked with **Change…**. Settings live in
the platform config folder (`%APPDATA%\SJK\Launcher\config\launcher.json` on Windows).

## Build

```
./build.ps1          # Windows: build and pack into dist/
sh build.sh          # Linux and macOS
```

Add `-Test` / `--test` to run the tests first. If Windows refuses to run the script, use
`powershell -ExecutionPolicy Bypass -File build.ps1`.

To release, set the version in `Cargo.toml`, then push a tag `v<version>` (or use
*Run workflow* on the Build action and type the version). The action tests and builds on
all three OSes and publishes a GitHub release with the archives; a version with a `-`,
like `0.2.0-beta.1`, becomes a pre-release.

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
  repository\extras\    unofficial_sjk_extras.pk3 (OPTIONAL)
```

Upload a big PK3 under another name outside `repository` first and move it in once
it's whole, so the server never lists a half-copied file.

Add or replace PK3s with scp; the server picks them up within 10 seconds:

```
scp mymap.pk3 sith-cloud:C:/ProgramData/SithCloud/repository/maps/
```

`GET /manifest.json` lists every PK3 with its size and MD5 (cached in
`.sith-cloud-md5.json`); `GET /repository/<group>/<file>` serves it, with `Range` for
resumed downloads. Build it with `cargo build --release -p sith-cloud` (the Windows exe
links the C runtime statically, see `.cargo/config.toml`).