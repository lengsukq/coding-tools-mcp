#!/usr/bin/env bash
# Shared macOS Universal bundle gate used by both release.yml and macos-release.yml.
# Usage:
#   bash scripts/verify-macos-bundle.sh --dmg path/to/App.dmg
#   bash scripts/verify-macos-bundle.sh --app path/to/App.app
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TAURI_CONF="${REPO_ROOT}/src-tauri/tauri.conf.json"

DMG_PATH=""
APP_PATH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg)
      [[ $# -ge 2 ]] || { echo "Missing value for --dmg" >&2; exit 2; }
      DMG_PATH="$2"
      shift 2
      ;;
    --app)
      [[ $# -ge 2 ]] || { echo "Missing value for --app" >&2; exit 2; }
      APP_PATH="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "${DMG_PATH}" && -z "${APP_PATH}" ]]; then
  echo "Usage: $0 --dmg <path> | --app <path>" >&2
  exit 2
fi

EXPECTED_IDENTIFIER="$(node -p "JSON.parse(require('node:fs').readFileSync('${TAURI_CONF}', 'utf8')).identifier")"
if [[ -z "${EXPECTED_IDENTIFIER}" ]]; then
  echo "Failed to read bundle identifier from ${TAURI_CONF}" >&2
  exit 1
fi

MOUNT_DIR=""
cleanup() {
  if [[ -n "${MOUNT_DIR}" ]]; then
    hdiutil detach "${MOUNT_DIR}" -force >/dev/null 2>&1 || true
    rmdir "${MOUNT_DIR}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ -n "${DMG_PATH}" ]]; then
  MOUNT_DIR="$(mktemp -d)"
  hdiutil attach "${DMG_PATH}" -nobrowse -readonly -mountpoint "${MOUNT_DIR}" >/dev/null
  APP_PATH="$(find "${MOUNT_DIR}" -maxdepth 2 -name '*.app' | head -n 1)"
fi

if [[ -z "${APP_PATH}" || ! -d "${APP_PATH}" ]]; then
  echo "Could not locate .app bundle to verify" >&2
  exit 1
fi

INFO_PLIST="${APP_PATH}/Contents/Info.plist"
BIN_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "${INFO_PLIST}")"
ARCHS="$(lipo -archs "${APP_PATH}/Contents/MacOS/${BIN_NAME}")"
echo "Detected macOS architectures: ${ARCHS}"
echo "${ARCHS}" | grep -qw x86_64
echo "${ARCHS}" | grep -qw arm64

codesign --verify --deep --strict --verbose=2 "${APP_PATH}"
SIGN_INFO="$(codesign -dvv "${APP_PATH}" 2>&1)"
echo "${SIGN_INFO}"
echo "${SIGN_INFO}" | grep -q "Identifier=${EXPECTED_IDENTIFIER}"

# Keep privacy keys synchronized with src-tauri/tests/macos_info_plist.rs.
for key in \
  NSScreenCaptureUsageDescription \
  NSDocumentsFolderUsageDescription \
  NSDesktopFolderUsageDescription \
  NSDownloadsFolderUsageDescription \
  NSRemovableVolumesUsageDescription \
  NSNetworkVolumesUsageDescription; do
  /usr/libexec/PlistBuddy -c "Print :${key}" "${INFO_PLIST}" >/dev/null
done

echo "Verified macOS bundle (${EXPECTED_IDENTIFIER}) successfully."
