#!/usr/bin/env bash
# Install bolo – download the correct pre-built binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/bharxhav/bolomoty/main/install.sh | bash
#
# Environment variables:
#   VERSION     – pin to a specific release (e.g. "0.2.0"); default: latest
#   INSTALL_DIR – where to place the binary (default: /usr/local/bin)
#   MAN_DIR     – where to place the man page  (default: /usr/local/share/man/man1)

set -euo pipefail

REPO="bharxhav/bolomoty"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
MAN_DIR="${MAN_DIR:-/usr/local/share/man/man1}"

# ── Helpers ───────────────────────────────────────────────────────

info()  { printf '\033[1;34m=>\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m=>\033[0m %s\n' "$*"; }
err()   { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || err "'$1' is required but not found"
}

# ── Detect platform ──────────────────────────────────────────────

detect_target() {
    local os arch target
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)
            case "$arch" in
                x86_64)  target="x86_64-unknown-linux-musl" ;;
                aarch64) target="aarch64-unknown-linux-musl" ;;
                arm64)   target="aarch64-unknown-linux-musl" ;;
                *)       err "unsupported Linux architecture: $arch" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64)  target="x86_64-apple-darwin" ;;
                arm64)   target="aarch64-apple-darwin" ;;
                *)       err "unsupported macOS architecture: $arch" ;;
            esac
            ;;
        *)
            err "unsupported OS: $os"
            ;;
    esac

    echo "$target"
}

# ── Resolve version ──────────────────────────────────────────────

resolve_version() {
    if [ -n "${VERSION:-}" ]; then
        echo "v$VERSION"
    else
        need curl
        local tag
        tag="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
               | grep '"tag_name"' | head -1 | sed 's/.*: "\(.*\)".*/\1/')"
        [ -n "$tag" ] || err "could not determine latest release"
        echo "$tag"
    fi
}

# ── Main ─────────────────────────────────────────────────────────

main() {
    need curl
    need tar

    local target tag url tmp

    target="$(detect_target)"
    tag="$(resolve_version)"

    info "installing bolo $tag ($target)"

    url="https://github.com/$REPO/releases/download/$tag/bolo-$target.tar.gz"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    info "downloading $url"
    curl -fsSL "$url" -o "$tmp/bolo.tar.gz"
    tar xzf "$tmp/bolo.tar.gz" -C "$tmp"

    info "installing binary to $INSTALL_DIR"
    install -d "$INSTALL_DIR"
    install -m 755 "$tmp/bolo" "$INSTALL_DIR/bolo"

    # Try to install man page (non-fatal if it fails)
    local man_url="https://github.com/$REPO/releases/download/$tag/bolo.1"
    if curl -fsSL "$man_url" -o "$tmp/bolo.1" 2>/dev/null; then
        install -d "$MAN_DIR"
        install -m 644 "$tmp/bolo.1" "$MAN_DIR/bolo.1"
        ok "man page installed to $MAN_DIR/bolo.1"
    fi

    # Warn if install dir is not on PATH
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            printf '\033[1;33mwarning:\033[0m %s is not on your PATH\n' "$INSTALL_DIR"
            printf '  add this to your shell profile:\n'
            printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
            ;;
    esac

    ok "bolo $tag installed to $INSTALL_DIR/bolo"
}

main
