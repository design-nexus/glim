#!/usr/bin/env bash
# Build glim and install the binary.
# From a clone:  ./install.sh
# Optional:      ./install.sh --prefix "$HOME/.local"

set -euo pipefail

PREFIX="${HOME}/.local"
REPO="https://github.com/design-nexus/glim.git"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      PREFIX="${2:?--prefix needs a directory}"
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#--prefix=}"
      shift
      ;;
    -h|--help)
      echo "Usage: install.sh [--prefix DIR]"
      echo "Installs the glim binary into DIR/bin. Default DIR is ~/.local."
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found. Install Rust from https://rustup.rs and run this again." >&2
  exit 1
fi

cleanup() {
  if [[ -n "${CLONE_DIR:-}" ]]; then
    rm -rf "$CLONE_DIR"
  fi
}
trap cleanup EXIT

source_dir=""
script_path="${BASH_SOURCE[0]:-}"
if [[ -n "$script_path" && -f "$script_path" ]]; then
  candidate="$(cd "$(dirname "$script_path")" && pwd)"
  if [[ -f "$candidate/Cargo.toml" ]] && grep -q 'name = "glim"' "$candidate/Cargo.toml"; then
    source_dir="$candidate"
  fi
fi

if [[ -z "$source_dir" ]]; then
  if ! command -v git >/dev/null 2>&1; then
    echo "git was not found, and this script is not inside a glim checkout." >&2
    exit 1
  fi
  CLONE_DIR="$(mktemp -d)"
  echo "Cloning $REPO"
  git clone --depth 1 "$REPO" "$CLONE_DIR/glim"
  source_dir="$CLONE_DIR/glim"
fi

echo "Building release binary"
cargo build --release --locked --manifest-path "$source_dir/Cargo.toml"

install -d "$PREFIX/bin"
install -m 755 "$source_dir/target/release/glim" "$PREFIX/bin/glim"

echo "Installed $PREFIX/bin/glim"
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *)
    echo "Add this to your shell profile if that directory is not already on PATH:"
    echo "  export PATH=\"$PREFIX/bin:\$PATH\""
    ;;
esac
echo "Run: glim"
