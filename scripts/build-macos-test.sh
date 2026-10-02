#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "[build] macOS is required for the CLAP test bundle" >&2
  exit 2
fi

target="${CARGO_BUILD_TARGET:-aarch64-apple-darwin}"
if [[ "${target}" != "aarch64-apple-darwin" ]]; then
  echo "[build] this test bundle is arm64-only; use CARGO_BUILD_TARGET=aarch64-apple-darwin" >&2
  exit 2
fi

dist_dir="${DIST_DIR:-${repo_root}/dist}"
bundle_name="tension_field-v0.1.0-macos.clap"
binary_name="tension_field"
cargo_target_dir="${CARGO_TARGET_DIR:-${repo_root}/target}"
if [[ "${cargo_target_dir}" != /* ]]; then
  cargo_target_dir="${repo_root}/${cargo_target_dir}"
fi
source_binary="${cargo_target_dir}/${target}/release/libtension_field.dylib"
staging_root="$(mktemp -d "${TMPDIR:-/tmp}/tension-field-clap.XXXXXX")"
staged_bundle="${staging_root}/${bundle_name}"
destination="${dist_dir}/${bundle_name}"
trap 'rm -rf "${staging_root}"' EXIT

echo "[build] cargo build --locked --release --target ${target}"
cargo build --locked --release --target "${target}"
if [[ ! -f "${source_binary}" ]]; then
  echo "[build] expected cdylib not found: ${source_binary}" >&2
  exit 1
fi

mkdir -p "${staged_bundle}/Contents/MacOS"
cp "${source_binary}" "${staged_bundle}/Contents/MacOS/${binary_name}"
chmod 755 "${staged_bundle}/Contents/MacOS/${binary_name}"

cat > "${staged_bundle}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>tension_field</string>
  <key>CFBundleIdentifier</key>
  <string>com.portalsurfer.tensionfield.clap</string>
  <key>CFBundleName</key>
  <string>Tension Field</string>
  <key>CFBundlePackageType</key>
  <string>BNDL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
</dict>
</plist>
PLIST
printf 'BNDL????' > "${staged_bundle}/Contents/PkgInfo"

codesign_args=(--force --deep --sign - --timestamp=none)
/usr/bin/codesign "${codesign_args[@]}" "${staged_bundle}"
/usr/bin/plutil -lint "${staged_bundle}/Contents/Info.plist"
/usr/bin/codesign --verify --deep --strict "${staged_bundle}"
/usr/bin/file "${staged_bundle}/Contents/MacOS/${binary_name}"
architectures="$(/usr/bin/lipo -archs "${staged_bundle}/Contents/MacOS/${binary_name}")"
if [[ "${architectures}" != *arm64* ]]; then
  echo "[build] expected arm64 binary, got: ${architectures}" >&2
  exit 1
fi
/usr/bin/nm -gU "${staged_bundle}/Contents/MacOS/${binary_name}" | grep -q '_clap_entry'

mkdir -p "${dist_dir}"
if [[ -e "${destination}" ]]; then
  rm -rf "${destination}"
fi
mv "${staged_bundle}" "${destination}"

echo "[build] bundle: ${destination}"
echo "[build] architecture: ${architectures}"
echo "[build] sha256: $(/usr/bin/shasum -a 256 "${destination}/Contents/MacOS/${binary_name}" | awk '{print $1}')"
echo "[build] signing: ad hoc (not notarized)"
