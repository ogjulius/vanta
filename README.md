# VANTA

**One library. Every game.** VANTA is a Linux-first unified game launcher built with Rust, Tauri 2 and Svelte 5.
It brings your Steam, Epic Games, GOG, itch.io and locally installed games into one fast, good-looking library
and launches them through each platform's own supported mechanism.

VANTA reads **local client data only**. It never asks for, stores, or logs platform passwords or tokens.

---

## Installation

Prebuilt packages are on the [Releases page](https://github.com/ogjulius/vanta/releases/latest).
They're built on Ubuntu 22.04, so they run on any distro from 2022 onwards.
VANTA installs per-user data to `~/.local/share/app.vanta.launcher/`; uninstalling the app never deletes your library.

### Arch, CachyOS, EndeavourOS, Manjaro, Garuda

**Option A: PKGBUILD (recommended, managed by pacman)**
```bash
git clone https://github.com/ogjulius/vanta.git
cd vanta/packaging/arch
makepkg -si          # builds from the tagged source, runs the tests, installs with pacman
```
Uninstall with `sudo pacman -R vanta`.

**Option B: install script (no root, installs to `~/.local`)**
```bash
git clone https://github.com/ogjulius/vanta.git
cd vanta
./scripts/install.sh --deps   # --deps installs build dependencies via pacman (asks for sudo)
```
Update with `git pull && ./scripts/install.sh`. Uninstall with `./scripts/install.sh --uninstall`.

**Option C: AppImage.** See "Any distribution" below.

### Fedora, Nobara, Ultramarine
```bash
sudo dnf install https://github.com/ogjulius/vanta/releases/latest/download/VANTA-0.1.0-1.x86_64.rpm
```
Dependencies (WebKitGTK 4.1, GTK 3) are pulled in automatically. Uninstall with `sudo dnf remove vanta`.

On **Fedora Silverblue / Kinoite / Bazzite**, use the AppImage, or layer the rpm with
`rpm-ostree install <rpm-url>` and reboot.

### Debian, Ubuntu, Linux Mint, Pop!_OS, Zorin (Ubuntu 22.04+ / Debian 12+)
```bash
wget https://github.com/ogjulius/vanta/releases/latest/download/VANTA_0.1.0_amd64.deb
sudo apt install ./VANTA_0.1.0_amd64.deb
```
Uninstall with `sudo apt remove vanta`.

### openSUSE Tumbleweed
```bash
sudo zypper install --allow-unsigned-rpm https://github.com/ogjulius/vanta/releases/latest/download/VANTA-0.1.0-1.x86_64.rpm
```

### Any distribution: AppImage
```bash
wget https://github.com/ogjulius/vanta/releases/latest/download/VANTA_0.1.0_amd64.AppImage
chmod +x VANTA_0.1.0_amd64.AppImage
./VANTA_0.1.0_amd64.AppImage
```
If it complains about FUSE, install `fuse2` (Arch), `fuse` (Fedora) or `libfuse2` (Ubuntu), or run it with `--appimage-extract-and-run`.

### Build from source (any distro)
`./scripts/install.sh --deps` detects Arch, Fedora, Debian/Ubuntu and openSUSE families and installs the right build dependencies.
To install them by hand:

| Distro | Build dependencies |
|--------|--------------------|
| Arch family | `sudo pacman -S --needed base-devel rust nodejs npm webkit2gtk-4.1 gtk3 libsoup3 librsvg openssl libappindicator-gtk3 xdotool file` |
| Fedora family | `sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel gcc gcc-c++ make rust cargo nodejs npm curl wget file` |
| Debian / Ubuntu | `sudo apt install build-essential libwebkit2gtk-4.1-dev libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev nodejs npm curl wget file`, plus Rust from [rustup.rs](https://rustup.rs) |

Then run:
```bash
npm install
npm run app          # development build with hot reload
npm run release      # optimized binary + .deb/.rpm/.AppImage in src-tauri/target/release/bundle/
./scripts/install.sh # or build + install to ~/.local
```

> Packages you build on a rolling distro (Arch, Tumbleweed) link against its newer glibc and may not run on
> older distros. Use the CI-built release packages for sharing.

### Optional platform tooling

| Want | Install |
|------|---------|
| Steam games | `sudo pacman -S steam` and sign in once |
| Epic Games | `sudo pacman -S legendary` (or `pipx install legendary-gl`), then `legendary auth` and `legendary list` |
| GOG / Epic via GUI | Heroic Games Launcher (`yay -S heroic-games-launcher-bin`), sign in inside Heroic |
| itch.io | the itch app (`yay -S itch-setup-bin`) |
| Windows games outside stores | `sudo pacman -S wine` (or Proton installed through Steam) |
| Wrappers | `sudo pacman -S gamemode mangohud` |

None of these are required. VANTA reports each integration honestly as *Not installed*, *Detected* or *Connected*.

---

## Features

### Library
- One unified library with grid and list layouts.
- Search (press `/` or `Ctrl+K`), platform filters, an installed/not-installed filter and a tag filter.
- Sort by name, recently played, most played or recently added.
- Favorites, custom tags, custom descriptions, renaming and custom cover art.
- Removing a game never touches its files. Auto-discovered games are hidden instead of deleted, so rescans don't bring them back, and you can restore them in *Settings → Library*.
- Manual import of any native executable, script or Windows `.exe`.
- One-click import of games found in XDG desktop entries (the `Game` category).
- Generated fallback artwork, so games without metadata still look intentional.

### Platform integrations
Integrations are modular adapters implementing a common `PlatformAdapter` trait (`detect`, `discover`, `launch_plan`).
If one integration fails, the error is recorded and shown, and the others keep working.

| Platform | Discovery | Launch | Status |
|----------|-----------|--------|--------|
| **Steam** | `libraryfolders.vdf` and every `appmanifest_*.acf` across all library folders. Runtimes and Proton are filtered out. Playtime and last played come from `localconfig.vdf`. Artwork comes from Steam's local cache, then Steam's public CDN. | `steam steam://rungameid/<id>`, or `steam -applaunch <id> <args>` when launch arguments are set | Locally signed-in persona from `loginusers.vdf` |
| **Epic Games** | Legendary's `installed.json` and metadata cache, for standalone Legendary and for Heroic's bundled copy | `legendary launch <app>` with args, plus `--wine`/`--wine-prefix` from the profile; falls back to Heroic | Display name from Legendary's `user.json` (only that field is read) |
| **GOG** | Heroic's `gog_store/installed.json` and library cache; standalone GOG Linux installers (`~/GOG Games/*/gameinfo`) | Native builds run `start.sh` directly; Windows builds go through `heroic --no-gui heroic://launch?...` | Heroic GOG login state |
| **itch.io** | The itch app's `butler.db`, opened read-only. Prefers Linux builds and includes itch's playtime | Direct executable | Database presence |
| **Local** | Manual imports and desktop entries | Native, Wine or Proton | — |

### Launch engine
- Commands are always spawned with an explicit argv via `tokio::process`, **never through a shell**. Launch arguments and wrapper commands go through a quote-aware tokenizer that performs no expansion, so `; rm -rf ~` is passed through as plain text.
- Per-game profiles: launch arguments, environment variables, working directory, executable override, runner (auto, native, Wine or Proton), Wine binary, prefix, Proton version, and a wrapper such as `gamemoderun mangohud`.
- Wine prefixes that VANTA manages live in VANTA's own data directory. User files are never modified, and VANTA never runs `chmod` on them. A non-executable `.sh` is run through `/bin/sh` instead.
- Process tracking:
  - Direct launches: VANTA waits on the child process and records its exit code.
  - Steam: VANTA finds the game through its `reaper SteamLaunch AppId=<id>` process.
  - Legendary and Heroic: VANTA finds processes running from the game's install folder, including Wine `Z:\` paths.
- A non-zero exit within 10 seconds counts as a failed launch. The error includes the tail of the game's log (`logs/<game>.log`).
- The UI never blocks. Launching the same game twice is rejected, and *Stop* sends `SIGTERM`.
- A *Launch command* preview shows exactly what will be executed.

### Performance center
- CPU (overall and per thread), RAM, available memory, swap, load average and uptime, via `sysinfo`.
- GPU utilization, temperature, VRAM and power from `nvidia-smi` or amdgpu sysfs. If neither is available, the panel says so instead of showing fake values.
- CPU and memory of the running game's whole process tree.
- Metrics are sampled only while the Performance page is open, at the interval you choose. Nothing polls in the background.

### Statistics
Total tracked playtime, platform-reported playtime, sessions, failed launches, a last-7-days chart (with a table view), most played games, longest sessions and the full launch history. All of it is computed from real sessions; nothing is estimated. Steam's own counter already includes sessions VANTA started, so the displayed playtime is `max(platform, tracked)` and never double counted.

### Settings (every control is functional)
- **General:** start minimized, launch on startup (writes or removes `~/.config/autostart/vanta.desktop`), confirm before closing, default layout.
- **Appearance:** Midnight or Graphite dark theme, accent color (presets or a custom color), density, grid size, animation intensity (full, reduced or off), sidebar state.
- **Library:** automatic scan on startup, artwork downloading (turn it off to stay offline-only), default sort, metadata/artwork refresh, rescan, restore removed games.
- **Integrations:** enable or disable each platform (a disabled platform's games are hidden, not deleted), per-platform sync, connection diagnostics with the detected paths.
- **Performance:** monitoring interval, background tracking frequency (2, 5 or 15 s), artwork cache size and clearing.
- **Data:** database and folder locations, backup, validated restore (with an automatic safety copy) and JSON export.

### Data safety
- SQLite in WAL mode with `synchronous=FULL`, versioned migrations in transactions, foreign keys and unique `(platform, external_id)` keys.
- Multi-row writes (scans, tag edits, imports) run inside a single transaction.
- Corrupted settings are repaired field by field, and the bad value is kept under `app.corrupt`.
- An unreadable database is moved aside (never deleted), a fresh one is created, and a banner tells you where the old file is.
- A database written by a newer schema version is refused instead of being touched.
- Sessions interrupted by a crash are marked *interrupted* with zero duration rather than invented.
- Artwork is written atomically (temp file + rename).

### Keyboard shortcuts
| Keys | Action |
|------|--------|
| `Ctrl+1` … `Ctrl+8` | Home, Library, Installed, Favorites, Recently Played, Downloads, Statistics, Performance |
| `Ctrl+,` | Settings |
| `Ctrl+N` | Add a game |
| `/` or `Ctrl+K` | Focus search |
| `Alt+←`, `Backspace` (game page) | Back |

---

## Architecture

```
src-tauri/src/
  lib.rs            app setup, logging, database recovery, command registry
  commands.rs       Tauri command surface (structured AppError results)
  state.rs          shared state, background library scan + artwork refresh
  db.rs             SQLite schema, migrations, queries, stats, backup/restore
  models.rs         Game, Profile, Session, Statistics, PlatformStatus …
  settings.rs       validated settings with corruption recovery
  launcher.rs       argv tokenizer, profile validation, command building, process tracking
  artwork.rs        artwork cache (local first, then CDN; negative cache; atomic writes)
  monitor.rs        CPU/RAM/process metrics, NVIDIA/AMD GPU metrics
  vdf.rs            Valve KeyValues parser
  platforms/        steam, epic, gog, itch, local, desktop (XDG), runners (Wine/Proton)
src/
  lib/state.svelte.ts   global state (Svelte 5 runes), events, toasts
  lib/api.ts            typed command wrappers
  lib/views/            Home, Library, GameDetail, Downloads, Statistics, Performance, Settings
  lib/components/       Sidebar, GameCard, GameRow, Artwork, ProfileEditor, AddGameModal …
```

Data lives in `~/.local/share/app.vanta.launcher/` (`vanta.db`, `artwork/`, `logs/`, `backups/`, `prefixes/`).
Set `VANTA_LOG=debug` for verbose logs.

Diagnostic tool: `cd src-tauri && cargo run --example scan` prints what each adapter detects on your machine.

---

## Testing

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                # 49 tests
cd .. && npm run check    # svelte-check / TypeScript
npm run build
```

The tests use fixture directories and temporary databases. They never need a Steam or Epic account.
Coverage includes:
- database initialization, migrations, reopening, refusing newer schemas;
- inserts, updates, duplicate prevention, removal tombstones, search (including escaped `%`/`_`), filters and sorts;
- tags, profile persistence and reset, session tracking, interrupted-session recovery, backup/restore with junk-file rejection;
- settings corruption recovery;
- Steam, Epic, GOG and itch discovery from fixtures, including tool filtering, malformed manifests, absent platforms, incompatible schemas, and checking that tokens never leak into status;
- the argument tokenizer with shell metacharacters, profile validation, Wine wrapping and wrappers;
- missing-executable handling, the non-executable script fallback, real end-to-end child process tracking, quick-crash failure reporting with the log tail, and double-launch rejection;
- artwork caching with no re-download, missing artwork, an unavailable provider, and custom covers surviving a cache clear;
- NVIDIA output parsing and live system sampling.

---

## Known limitations

- **Steam:** only games that are installed (or were installed and later removed) are listed. Owned-but-never-installed games would require parsing the binary `appinfo.vdf` or using the Web API with a user key; neither is implemented. Environment variables, wrappers and the Proton version for Steam games stay in Steam's own launch options, because Steam launches the game.
- **Epic:** owned-but-not-installed games appear only after `legendary list` has populated Legendary's metadata cache. Epic and GOG don't expose playtime locally, so only VANTA-tracked time is shown for them.
- **GOG:** there is no official Linux client or public library API. Account-level ownership comes from Heroic's cache, and Windows GOG games require Heroic to launch.
- **itch.io:** the adapter reads the itch app's `butler.db`. It is tested against a fixture of the known schema; schema changes in future itch versions are reported as an integration error.
- **Session tracking** for Steam, Legendary and Heroic launches relies on process detection. If the game process can't be identified within 3 minutes (for example, Steam is downloading an update first), VANTA reports the session as untracked rather than guessing.
- **Downloads:** VANTA doesn't download or install games itself. The Downloads page shows live install and update progress read from Steam's manifests, plus sync status for each platform.
- **Language:** the interface is English only. No language selector is shown, because it would do nothing.
- **Not live-tested on this machine:** Legendary, Heroic, itch and Wine aren't installed here. Those adapters are covered by fixture-based tests only. The Steam launch path (`steam://rungameid` plus reaper detection) is implemented but wasn't exercised against a running game during development. Steam discovery, artwork and playtime import, and the full direct-launch tracking path *were* verified live.

## Not implemented (future releases)
Cloud accounts, social features, achievements, a plugin marketplace, controller/Big Picture UI, IGDB metadata (the artwork system is provider-based, so IGDB could be added as another provider), a system tray icon and UI translations.

## License

[MIT](LICENSE) © 2026 ogjulius
