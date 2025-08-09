#!/usr/bin/env bash
set -euo pipefail

# Detect platform
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
# Normalize arch names to release artifact suffixes
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
esac

REPO="toppev/vecgrep"
BINARY="vecgrep"

# Try downloading a prebuilt binary from GitHub Releases first
LATEST_URL="https://github.com/${REPO}/releases/latest/download/${BINARY}-${OS}-${ARCH}.tar.gz"
TMPDIR="$(mktemp -d)"

echo "Attempting to download prebuilt ${BINARY} for ${OS}/${ARCH}..." >&2
if curl -fsSL "${LATEST_URL}" -o "${TMPDIR}/bin.tar.gz"; then
  tar -xzf "${TMPDIR}/bin.tar.gz" -C "${TMPDIR}"
  install -m 0755 "${TMPDIR}/${BINARY}" "${HOME}/.local/bin/${BINARY}" 2>/dev/null || {
    mkdir -p "${HOME}/.local/bin"
    install -m 0755 "${TMPDIR}/${BINARY}" "${HOME}/.local/bin/${BINARY}"
  }
  echo "Installed to ${HOME}/.local/bin/${BINARY}" >&2
  echo "" >&2
  echo "To use vecgrep, add ~/.local/bin to your PATH:" >&2
  echo "  For bash/zsh: echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc" >&2
  echo "  For fish: fish_add_path ~/.local/bin" >&2
  echo "" >&2
  echo "Then restart your terminal or run: source ~/.bashrc" >&2
  echo "Test with: vecgrep --help" >&2
  exit 0
fi

echo "No prebuilt binary; falling back to cargo build (needs Rust toolchain)." >&2
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust toolchain is required to build from source." >&2
  read -r -p "Install Rust via rustup now? [y/N] " ans
  case "$ans" in
    [yY][eE][sS]|[yY])
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
      export PATH="$HOME/.cargo/bin:$PATH"
      ;;
    *)
      echo "Aborting. Please install Rust from https://www.rust-lang.org/tools/install and re-run." >&2
      exit 1
      ;;
  esac
fi

# Build from source
SRC_DIR="$(mktemp -d)"
trap 'rm -rf "${SRC_DIR}"' EXIT

curl -fsSL "https://codeload.github.com/${REPO}/tar.gz/refs/heads/main" -o "${SRC_DIR}/src.tar.gz"
mkdir -p "${SRC_DIR}/repo"
tar -xzf "${SRC_DIR}/src.tar.gz" -C "${SRC_DIR}/repo" --strip-components=1
cd "${SRC_DIR}/repo"
cargo install --path .

echo "Installed successfully!" >&2
echo "" >&2
echo "To use vecgrep, add ~/.cargo/bin to your PATH:" >&2
echo "  For bash/zsh: echo 'export PATH=\"\$HOME/.cargo/bin:\$PATH\"' >> ~/.bashrc" >&2
echo "  For fish: fish_add_path ~/.cargo/bin" >&2
echo "" >&2
echo "Then restart your terminal or run: source ~/.bashrc" >&2
echo "Test with: vecgrep --help" >&2
