#!/usr/bin/env bash

set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd -P)"
readonly BASE_CONFIG="${REPO_ROOT}/src-tauri/tauri.conf.json"
readonly MAC_CONFIG="${REPO_ROOT}/src-tauri/tauri.macos.conf.json"

readonly BUILD_MODE="${MINETRACE_MAC_BUILD_MODE:-local}"
readonly BUILD_TARGET="${MINETRACE_MAC_TARGET:-universal-apple-darwin}"
readonly SKIP_STAPLING="${MINETRACE_SKIP_STAPLING:-0}"

BUILD_MARKER=""

log() {
  printf '[MineTrace macOS] %s\n' "$*"
}

die() {
  printf '[MineTrace macOS] Error: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [[ -n "${BUILD_MARKER}" && -f "${BUILD_MARKER}" ]]; then
    rm -f -- "${BUILD_MARKER}"
  fi
}

trap cleanup EXIT

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "Required command '$1' was not found."
}

require_env() {
  [[ -n "${!1:-}" ]] || die "Required environment variable '$1' is not set."
}

rust_version_is_supported() {
  local version
  local major
  local minor

  version="$(rustc --version | awk '{print $2}')"
  major="${version%%.*}"
  version="${version#*.}"
  minor="${version%%.*}"

  [[ "${major}" -gt 1 || ( "${major}" -eq 1 && "${minor}" -ge 92 ) ]]
}

target_is_available() {
  local target_libdir

  target_libdir="$(rustc --print target-libdir --target "$1" 2>/dev/null || true)"
  [[ -n "${target_libdir}" && -d "${target_libdir}" ]]
}

validate_target() {
  case "${BUILD_TARGET}" in
    universal-apple-darwin)
      target_is_available aarch64-apple-darwin || die "The active Rust toolchain is missing aarch64-apple-darwin. Install it with rustup or activate a toolchain that provides it."
      target_is_available x86_64-apple-darwin || die "The active Rust toolchain is missing x86_64-apple-darwin. Install it with rustup or activate a toolchain that provides it."
      ;;
    aarch64-apple-darwin | x86_64-apple-darwin)
      target_is_available "${BUILD_TARGET}" || die "The active Rust toolchain is missing ${BUILD_TARGET}. Install it with rustup or activate a toolchain that provides it."
      ;;
    *)
      die "MINETRACE_MAC_TARGET must be universal-apple-darwin, aarch64-apple-darwin, or x86_64-apple-darwin."
      ;;
  esac
}

validate_notarization_environment() {
  local has_api_credentials=0
  local has_apple_id_credentials=0

  if [[ -n "${APPLE_API_ISSUER:-}" || -n "${APPLE_API_KEY:-}" || -n "${APPLE_API_KEY_PATH:-}" ]]; then
    require_env APPLE_API_ISSUER
    require_env APPLE_API_KEY
    require_env APPLE_API_KEY_PATH
    [[ -r "${APPLE_API_KEY_PATH}" ]] || die "APPLE_API_KEY_PATH must reference a readable key file."
    has_api_credentials=1
  fi

  if [[ -n "${APPLE_ID:-}" || -n "${APPLE_PASSWORD:-}" || -n "${APPLE_TEAM_ID:-}" ]]; then
    require_env APPLE_ID
    require_env APPLE_PASSWORD
    require_env APPLE_TEAM_ID
    has_apple_id_credentials=1
  fi

  if [[ "${has_api_credentials}" -eq 0 && "${has_apple_id_credentials}" -eq 0 ]]; then
    die "Release mode requires either App Store Connect API credentials or Apple ID notarization credentials."
  fi

  if [[ "${has_api_credentials}" -eq 1 && "${has_apple_id_credentials}" -eq 1 ]]; then
    die "Provide only one notarization credential set, not both."
  fi
}

validate_release_identity() {
  local identity_match

  require_env APPLE_SIGNING_IDENTITY
  [[ "${APPLE_SIGNING_IDENTITY}" != "-" ]] || die "Release mode cannot use ad-hoc signing."

  identity_match="$(security find-identity -v -p codesigning 2>/dev/null | grep -F -m 1 -- "${APPLE_SIGNING_IDENTITY}" || true)"
  [[ -n "${identity_match}" ]] || die "APPLE_SIGNING_IDENTITY was not found in the available keychains."

  case "${identity_match}" in
    *"Developer ID Application:"*) ;;
    *) die "Direct DMG releases require a Developer ID Application identity." ;;
  esac
}

validate_prerequisites() {
  [[ "$(uname -s)" == "Darwin" ]] || die "This script must run on macOS."

  case "${BUILD_MODE}" in
    local | release) ;;
    *) die "MINETRACE_MAC_BUILD_MODE must be 'local' or 'release'." ;;
  esac

  case "${SKIP_STAPLING}" in
    0 | 1) ;;
    *) die "MINETRACE_SKIP_STAPLING must be 0 or 1." ;;
  esac

  require_command pnpm
  require_command cargo
  require_command rustc
  require_command xcode-select
  require_command xcrun
  require_command codesign
  require_command security
  require_command lipo
  require_command hdiutil
  require_command plutil
  require_command shasum
  require_command /usr/libexec/PlistBuddy

  xcode-select -p >/dev/null 2>&1 || die "Xcode command-line tools are not configured."

  [[ -f "${REPO_ROOT}/package.json" ]] || die "package.json was not found at the repository root."
  [[ -f "${BASE_CONFIG}" ]] || die "The shared Tauri config does not exist yet: src-tauri/tauri.conf.json"
  [[ -f "${MAC_CONFIG}" ]] || die "The macOS Tauri overlay does not exist yet: src-tauri/tauri.macos.conf.json"
  [[ -f "${REPO_ROOT}/src-tauri/Cargo.toml" ]] || die "src-tauri/Cargo.toml was not found."
  [[ -x "${REPO_ROOT}/node_modules/.bin/tauri" ]] || die "The project-local Tauri CLI is missing. Run pnpm install first."

  rust_version_is_supported || die "MineTrace requires Rust 1.92 or newer; update or activate a compatible toolchain."
  validate_target

  if [[ "${BUILD_MODE}" == "release" ]]; then
    require_command spctl
    xcrun --find notarytool >/dev/null 2>&1 || die "Apple notarytool was not found."
    xcrun --find stapler >/dev/null 2>&1 || die "Apple stapler was not found."
    validate_release_identity
    validate_notarization_environment
  fi
}

validate_architectures() {
  local executable_path="$1"
  local architectures

  architectures="$(lipo -archs "${executable_path}")"

  case "${BUILD_TARGET}" in
    universal-apple-darwin)
      [[ " ${architectures} " == *" arm64 "* ]] || die "Universal app is missing the arm64 slice."
      [[ " ${architectures} " == *" x86_64 "* ]] || die "Universal app is missing the x86_64 slice."
      ;;
    aarch64-apple-darwin)
      [[ " ${architectures} " == *" arm64 "* ]] || die "App is missing the arm64 slice."
      ;;
    x86_64-apple-darwin)
      [[ " ${architectures} " == *" x86_64 "* ]] || die "App is missing the x86_64 slice."
      ;;
  esac

  log "Architectures verified: ${architectures}"
}

validate_app_bundle() {
  local app_path="$1"
  local executable_name
  local executable_path
  local signing_details
  local sandbox_value

  executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "${app_path}/Contents/Info.plist" 2>/dev/null)"
  [[ -n "${executable_name}" ]] || die "CFBundleExecutable is missing from the app bundle."
  executable_path="${app_path}/Contents/MacOS/${executable_name}"
  [[ -x "${executable_path}" ]] || die "The app executable is missing or is not executable."

  validate_architectures "${executable_path}"

  codesign --verify --deep --strict "${app_path}" >/dev/null 2>&1 || die "The app bundle failed strict code-signature verification."

  signing_details="$(codesign -dvv "${app_path}" 2>&1 || true)"
  [[ "${signing_details}" == *"runtime"* ]] || die "The app signature does not contain the Hardened Runtime flag."

  sandbox_value="$(codesign -d --entitlements :- "${app_path}" 2>/dev/null | plutil -extract com.apple.security.app-sandbox raw -o - - 2>/dev/null || true)"
  [[ "${sandbox_value}" != "true" ]] || die "The direct-distribution app unexpectedly enables App Sandbox."

  if [[ "${BUILD_MODE}" == "release" ]]; then
    spctl --assess --type execute "${app_path}" >/dev/null 2>&1 || die "Gatekeeper assessment failed for the release app."

    if [[ "${SKIP_STAPLING}" == "0" ]]; then
      xcrun stapler validate "${app_path}" >/dev/null 2>&1 || die "The release app does not have a valid stapled notarization ticket."
    fi
  fi

  log "App bundle checks passed: ${app_path}"
}

validate_dmg() {
  local dmg_path="$1"

  hdiutil verify "${dmg_path}" >/dev/null 2>&1 || die "The DMG failed hdiutil verification."

  if [[ "${BUILD_MODE}" == "release" && "${SKIP_STAPLING}" == "0" ]]; then
    xcrun stapler validate "${dmg_path}" >/dev/null 2>&1 || die "The release DMG does not have a valid stapled notarization ticket."
  fi

  log "DMG checks passed: ${dmg_path}"
  shasum -a 256 "${dmg_path}"
}

build() {
  local build_args

  build_args=(
    tauri build
    --config "${MAC_CONFIG}"
    --target "${BUILD_TARGET}"
    --bundles app,dmg
  )

  if [[ "${SKIP_STAPLING}" == "1" ]]; then
    build_args+=(--skip-stapling)
  fi

  BUILD_MARKER="$(mktemp -t minetrace-macos-build.XXXXXX)"

  log "Building ${BUILD_TARGET} in ${BUILD_MODE} mode."

  if [[ "${BUILD_MODE}" == "local" ]]; then
    (
      cd -- "${REPO_ROOT}"
      APPLE_SIGNING_IDENTITY=- \
        pnpm "${build_args[@]}"
    )
  else
    (
      cd -- "${REPO_ROOT}"
      pnpm "${build_args[@]}"
    )
  fi
}

verify_artifacts() {
  local bundle_root="${REPO_ROOT}/src-tauri/target/${BUILD_TARGET}/release/bundle"
  local app_count=0
  local dmg_count=0
  local candidate
  local app_candidates
  local dmg_candidates

  shopt -s nullglob
  app_candidates=("${bundle_root}/macos/"*.app)
  dmg_candidates=("${bundle_root}/dmg/"*.dmg)
  shopt -u nullglob

  for candidate in "${app_candidates[@]}"; do
    if [[ "${candidate}" -nt "${BUILD_MARKER}" || "${candidate}/Contents/Info.plist" -nt "${BUILD_MARKER}" ]]; then
      validate_app_bundle "${candidate}"
      app_count=$((app_count + 1))
    fi
  done

  for candidate in "${dmg_candidates[@]}"; do
    if [[ "${candidate}" -nt "${BUILD_MARKER}" ]]; then
      validate_dmg "${candidate}"
      dmg_count=$((dmg_count + 1))
    fi
  done

  [[ "${app_count}" -gt 0 ]] || die "No app bundle created by this invocation was found."
  [[ "${dmg_count}" -gt 0 ]] || die "No DMG created by this invocation was found."

  if [[ "${BUILD_MODE}" == "local" ]]; then
    log "Local artifacts are ad-hoc signed and are not ready for public distribution."
  elif [[ "${SKIP_STAPLING}" == "1" ]]; then
    log "Release artifacts were notarized without stapling; staple and validate them before distribution."
  else
    log "Release artifact verification completed. Perform a quarantined-download smoke test before publishing."
  fi
}

main() {
  validate_prerequisites
  build
  verify_artifacts
}

main "$@"
