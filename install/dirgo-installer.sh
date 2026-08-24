#!/bin/sh
set -eu

REPOSITORY="RudySource/Dirgo"
DOWNLOAD_BASE="${DIRGO_DOWNLOAD_BASE:-https://github.com/$REPOSITORY/releases/latest/download}"
INSTALL_DIR="${DIRGO_INSTALL_DIR:-$HOME/.local/bin}"
SETUP_SHELL=1
ASSUME_YES=0

for argument in "$@"; do
  case "$argument" in
    --no-setup) SETUP_SHELL=0 ;;
    --yes|-y) ASSUME_YES=1 ;;
    --help|-h)
      printf '%s\n' 'Dirgo installer' '' 'Usage: dirgo-installer.sh [--yes] [--no-setup]' '' 'Environment:' '  DIRGO_INSTALL_DIR    Binary destination (default: ~/.local/bin)' '  DIRGO_DOWNLOAD_BASE  Release asset base URL'
      exit 0
      ;;
    *) printf 'Unknown option: %s\n' "$argument" >&2; exit 2 ;;
  esac
done

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != "dumb" ]; then
  BLUE='\033[1;34m'
  GREEN='\033[32m'
  RESET='\033[0m'
else
  BLUE=''
  GREEN=''
  RESET=''
fi
if [ -t 1 ] && [ "${TERM:-}" != "dumb" ]; then
  CHECK='✓'
else
  CHECK='OK'
fi

heading() {
  printf '%bDIRGO%b\nGo anywhere. Instantly.\n\n' "$BLUE" "$RESET"
}

success() {
  printf '%b%s%b %s\n' "$GREEN" "$CHECK" "$RESET" "$1"
}

fail() {
  printf 'Dirgo installer: %s\n' "$1" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || fail 'curl is required'
command -v tar >/dev/null 2>&1 || fail 'tar is required'
command -v awk >/dev/null 2>&1 || fail 'awk is required'
command -v install >/dev/null 2>&1 || fail 'install is required'

case "$(uname -s):$(uname -m)" in
  Darwin:arm64|Darwin:aarch64) TARGET='aarch64-apple-darwin' ;;
  Darwin:x86_64) TARGET='x86_64-apple-darwin' ;;
  Linux:x86_64|Linux:amd64) TARGET='x86_64-unknown-linux-gnu' ;;
  *) fail "unsupported platform $(uname -s) $(uname -m); use a release archive instead" ;;
esac

if [ "$TARGET" = 'x86_64-unknown-linux-gnu' ] && command -v ldd >/dev/null 2>&1; then
  if ldd --version 2>&1 | grep -qi musl; then
    fail 'this release requires glibc 2.35 or newer; musl is not supported yet'
  fi
  if command -v getconf >/dev/null 2>&1; then
    GLIBC_VERSION=$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{ print $2 }')
    if [ -n "$GLIBC_VERSION" ] && [ "$(printf '%s\n' 2.35 "$GLIBC_VERSION" | sort -V | head -n 1)" != '2.35' ]; then
      fail "glibc $GLIBC_VERSION is too old; Dirgo requires glibc 2.35 or newer"
    fi
  fi
fi

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/dirgo-install.XXXXXX") || fail 'could not create a temporary directory'
STAGED_BINARY=''
cleanup() {
  case "$TMP_DIR" in
    "${TMPDIR:-/tmp}"/dirgo-install.*) rm -rf -- "$TMP_DIR" ;;
    *) printf 'Refusing to clean unexpected temporary path: %s\n' "$TMP_DIR" >&2 ;;
  esac
  if [ -n "$STAGED_BINARY" ]; then
    case "$STAGED_BINARY" in
      "$INSTALL_DIR"/.dgo-install.*) rm -f -- "$STAGED_BINARY" ;;
      *) printf 'Refusing to clean unexpected staged path: %s\n' "$STAGED_BINARY" >&2 ;;
    esac
  fi
}
trap cleanup EXIT HUP INT TERM

ASSET="dirgo-$TARGET.tar.gz"
ARCHIVE="$TMP_DIR/$ASSET"
CHECKSUMS="$TMP_DIR/SHA256SUMS"

heading
printf 'Downloading the verified release for %s...\n' "$TARGET"
download() {
  case "$1" in
    https://*) curl --proto '=https' --tlsv1.2 -fLsS "$1" -o "$2" ;;
    *)
      [ -n "${DIRGO_DOWNLOAD_BASE:-}" ] || fail 'refusing a non-HTTPS release URL'
      curl -fLsS "$1" -o "$2"
      ;;
  esac
}
download "$DOWNLOAD_BASE/$ASSET" "$ARCHIVE" || fail 'release download failed'
download "$DOWNLOAD_BASE/SHA256SUMS" "$CHECKSUMS" || fail 'checksum download failed'

EXPECTED=$(awk -v asset="$ASSET" '$2 == asset || $2 == "*" asset { print $1; exit }' "$CHECKSUMS")
[ -n "$EXPECTED" ] || fail "SHA256SUMS does not contain $ASSET"
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL=$(sha256sum "$ARCHIVE" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL=$(shasum -a 256 "$ARCHIVE" | awk '{ print $1 }')
else
  fail 'sha256sum or shasum is required to verify the download'
fi
[ "$ACTUAL" = "$EXPECTED" ] || fail 'checksum verification failed; nothing was installed'
success 'Download verified'

MEMBER=$(tar -tzf "$ARCHIVE" | awk -F/ 'NF == 2 && $2 == "dgo" { print; exit }')
[ -n "$MEMBER" ] || fail 'release archive does not contain dgo in the expected location'
case "$MEMBER" in
  /*|../*|*/../*) fail 'unsafe path in release archive' ;;
esac
tar -xzf "$ARCHIVE" -C "$TMP_DIR" "$MEMBER" || fail 'could not extract dgo'
[ -f "$TMP_DIR/$MEMBER" ] && [ ! -L "$TMP_DIR/$MEMBER" ] || fail 'dgo is not a regular file in the release archive'

mkdir -p "$INSTALL_DIR" || fail "could not create $INSTALL_DIR"
STAGED_BINARY=$(mktemp "$INSTALL_DIR/.dgo-install.XXXXXX") || fail "could not stage the binary in $INSTALL_DIR"
install -m 755 "$TMP_DIR/$MEMBER" "$STAGED_BINARY" || fail "could not stage the binary in $INSTALL_DIR"
"$STAGED_BINARY" --version >/dev/null || fail 'the downloaded binary did not start'
mv -f -- "$STAGED_BINARY" "$INSTALL_DIR/dgo" || fail "could not install to $INSTALL_DIR"
STAGED_BINARY=''
success "Installed to $INSTALL_DIR/dgo"

if [ "$SETUP_SHELL" -eq 1 ]; then
  case "${SHELL:-}" in
    */zsh|*/bash|*/fish)
      if [ "$ASSUME_YES" -eq 1 ]; then
        "$INSTALL_DIR/dgo" setup --yes
      elif [ -r /dev/tty ]; then
        "$INSTALL_DIR/dgo" setup </dev/tty
      else
        printf '\nShell setup needs an interactive confirmation. Run:\n  %s setup\n' "$INSTALL_DIR/dgo"
      fi
      ;;
    *)
      printf '\nBinary installed. Connect a supported shell with:\n  %s setup --shell zsh|bash|fish\n' "$INSTALL_DIR/dgo"
      ;;
  esac
else
  printf '\nShell setup skipped. Run `%s setup` when ready.\n' "$INSTALL_DIR/dgo"
fi
