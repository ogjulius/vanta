#!/usr/bin/env bash
# VANTA installer — builds from source and installs for the current user.
#
#   ./scripts/install.sh              build + install to ~/.local
#   ./scripts/install.sh --deps       also install build dependencies (uses sudo + your package manager)
#   ./scripts/install.sh --no-build   install the already-built release binary
#   ./scripts/install.sh --uninstall  remove the installed files (your library data is kept)
#
# Environment: PREFIX (default: ~/.local)
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$PREFIX/bin/vanta"
DESKTOP="$PREFIX/share/applications/vanta.desktop"
ICONS="$PREFIX/share/icons/hicolor"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/app.vanta.launcher"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '  \033[35m›\033[0m %s\n' "$*"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$*" >&2; }
die() { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

DO_DEPS=0
DO_BUILD=1
DO_UNINSTALL=0
for arg in "$@"; do
  case "$arg" in
    --deps) DO_DEPS=1 ;;
    --no-build) DO_BUILD=0 ;;
    --uninstall) DO_UNINSTALL=1 ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $arg (see --help)" ;;
  esac
done

uninstall() {
  bold "Uninstalling VANTA from $PREFIX"
  for f in "$BIN" "$DESKTOP" "$ICONS"/{32x32,64x64,128x128,256x256,512x512}/apps/vanta.png; do
    if [ -e "$f" ]; then rm -f -- "$f" && info "removed $f"; fi
  done
  command -v update-desktop-database >/dev/null && update-desktop-database -q "$PREFIX/share/applications" || true
  info "Your library, profiles and history were kept in $DATA_DIR"
  info "Delete that folder yourself if you want to remove them too."
}

if [ "$DO_UNINSTALL" = 1 ]; then uninstall; exit 0; fi

detect_distro() {
  local id like
  id="$(. /etc/os-release 2>/dev/null && echo "${ID:-}")"
  like="$(. /etc/os-release 2>/dev/null && echo "${ID_LIKE:-}")"
  case " $id $like " in
    *" arch "*) echo arch ;;
    *" fedora "*|*" rhel "*) echo fedora ;;
    *" debian "*|*" ubuntu "*) echo debian ;;
    *" suse "*|*" opensuse "*) echo suse ;;
    *) echo unknown ;;
  esac
}

install_deps() {
  local distro; distro="$(detect_distro)"
  bold "Installing build dependencies ($distro)"
  case "$distro" in
    arch)
      sudo pacman -S --needed base-devel rust nodejs npm webkit2gtk-4.1 gtk3 libsoup3 librsvg openssl \
        libappindicator-gtk3 xdotool file ;;
    fedora)
      sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel \
        librsvg2-devel libxdo-devel gcc gcc-c++ make rust cargo nodejs npm ;;
    debian)
      sudo apt-get update
      sudo apt-get install -y build-essential libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
        libayatana-appindicator3-dev librsvg2-dev curl wget file nodejs npm
      command -v cargo >/dev/null || warn "Install Rust with rustup: https://rustup.rs (distro Rust is often too old)" ;;
    suse)
      sudo zypper install -y -t pattern devel_basis
      sudo zypper install -y webkit2gtk3-devel libopenssl-devel curl wget file libappindicator3-1 librsvg-devel \
        rust cargo nodejs npm ;;
    *) die "unknown distribution — install Rust, Node.js 20+, and WebKitGTK 4.1 development files manually" ;;
  esac
}

check_deps() {
  local missing=()
  command -v cargo >/dev/null || missing+=("cargo (Rust)")
  command -v npm >/dev/null || missing+=("npm (Node.js)")
  pkg-config --exists webkit2gtk-4.1 2>/dev/null || missing+=("webkit2gtk-4.1 development files")
  if [ "${#missing[@]}" -gt 0 ]; then
    warn "missing: ${missing[*]}"
    die "run again with --deps to install them, or see README.md → Installation"
  fi
}

[ "$DO_DEPS" = 1 ] && install_deps

cd "$ROOT"
if [ "$DO_BUILD" = 1 ]; then
  check_deps
  bold "Building VANTA (release)"
  info "installing frontend packages"
  npm ci --no-audit --no-fund --loglevel=error
  info "compiling — the first build takes a few minutes"
  npm exec -- tauri build --no-bundle
fi

SRC="$ROOT/src-tauri/target/release/vanta"
[ -x "$SRC" ] || die "no release binary at $SRC — run without --no-build"

bold "Installing to $PREFIX"
install -Dm755 "$SRC" "$BIN" && info "binary   → $BIN"
icon_src() {
  case "$1" in
    32) echo "$ROOT/src-tauri/icons/32x32.png" ;;
    64) echo "$ROOT/src-tauri/icons/64x64.png" ;;
    128) echo "$ROOT/src-tauri/icons/128x128.png" ;;
    256) echo "$ROOT/src-tauri/icons/128x128@2x.png" ;;
    512) echo "$ROOT/src-tauri/icons/icon.png" ;;
  esac
}
for size in 32 64 128 256 512; do
  install -Dm644 "$(icon_src $size)" "$ICONS/${size}x${size}/apps/vanta.png"
done
info "icons    → $ICONS"

mkdir -p "$(dirname "$DESKTOP")"
cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=VANTA
GenericName=Game Launcher
Comment=One library for Steam, Epic, GOG, itch.io and local games
Exec="$BIN"
Icon=vanta
Terminal=false
Categories=Game;
Keywords=games;launcher;steam;epic;gog;itch;library;
StartupWMClass=vanta
EOF
info "launcher → $DESKTOP"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$(dirname "$DESKTOP")" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true

case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) warn "$PREFIX/bin is not in your PATH — the app menu entry works, but to run 'vanta' from a terminal add it to PATH" ;;
esac

bold "Done. Launch VANTA from your app menu, or run: vanta"
