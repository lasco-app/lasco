#!/usr/bin/env bash
# Records the complete iOS Lasco Cloud demo on an isolated Simulator.
set -euo pipefail

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
flow_path="$root_dir/maestro/ios-demo/ios-demo.yaml"
fixture_dir="$root_dir/maestro/ios-demo/assets"
fixture_path="$fixture_dir/holidays.jpg"
artifact_dir="$root_dir/maestro/ios-demo/artifacts"
derived_data_path="${DERIVED_DATA_PATH:-/tmp/lasco-maestro-release}"

simulator_name="${SIMULATOR_NAME:-Lasco Maestro iOS}"
device_type="${SIMULATOR_DEVICE_TYPE:-com.apple.CoreSimulator.SimDeviceType.iPhone-17}"
runtime="${SIMULATOR_RUNTIME:-com.apple.CoreSimulator.SimRuntime.iOS-26-0}"
fixture_url="https://picsum.photos/seed/lasco-holidays/1600/1200.jpg"

export MAESTRO_LIBRARY_NAME="Lasco play store review"
export MAESTRO_LIBRARY_USERNAME="username"
export MAESTRO_LIBRARY_PASSWORD="1234567"

if { [ -z "${MAESTRO_CLOUD_EMAIL:-}" ] || [ -z "${MAESTRO_CLOUD_PASSWORD:-}" ]; } \
  && { [ -z "${MAESTRO_REVIEW_LASCO_CLOUD_EMAIL:-}" ] || [ -z "${MAESTRO_REVIEW_LASCO_CLOUD_PASSWORD:-}" ]; } \
  && command -v doppler >/dev/null; then
  exec doppler run \
    --scope "${DOPPLER_SCOPE:-$root_dir/../lasco-cloud}" \
    --config "${DOPPLER_CONFIG:-dev}" \
    -- "$0" "$@"
fi

export MAESTRO_CLOUD_EMAIL="${MAESTRO_CLOUD_EMAIL:-${MAESTRO_REVIEW_LASCO_CLOUD_EMAIL:-}}"
export MAESTRO_CLOUD_PASSWORD="${MAESTRO_CLOUD_PASSWORD:-${MAESTRO_REVIEW_LASCO_CLOUD_PASSWORD:-}}"

if [ -z "$MAESTRO_CLOUD_EMAIL" ] || [ -z "$MAESTRO_CLOUD_PASSWORD" ]; then
  echo "Lasco Cloud credentials are required. Run via Doppler or provide MAESTRO_CLOUD_EMAIL and MAESTRO_CLOUD_PASSWORD." >&2
  exit 1
fi

if ! command -v xcodebuild >/dev/null || ! command -v xcrun >/dev/null || ! command -v curl >/dev/null; then
  echo "This script requires Xcode command-line tools and curl." >&2
  exit 1
fi

maestro_bin="${MAESTRO_BIN:-}"
if [ -z "$maestro_bin" ]; then
  maestro_bin="$(command -v maestro || true)"
fi
if [ -z "$maestro_bin" ]; then
  echo "Maestro is not on PATH. Set MAESTRO_BIN to its executable path." >&2
  exit 1
fi

mkdir -p "$fixture_dir" "$artifact_dir"

if [ ! -s "$fixture_path" ] || [ "${1:-}" = "--refresh-fixture" ]; then
  echo "Downloading the deterministic demo photo…"
  curl --fail --location --retry 3 --output "$fixture_path.tmp" "$fixture_url"
  mv -f "$fixture_path.tmp" "$fixture_path"
fi

simulator_udid="$({
  xcrun simctl list devices available -j |
    SIMULATOR_NAME="$simulator_name" ruby -rjson -e '
      devices = JSON.parse(STDIN.read).fetch("devices").values.flatten
      device = devices.find { |entry| entry["name"] == ENV.fetch("SIMULATOR_NAME") && entry["isAvailable"] }
      puts device["udid"] if device
    '
} )"

created_simulator=false
if [ -z "$simulator_udid" ]; then
  echo "Creating dedicated Simulator: $simulator_name"
  simulator_udid="$(xcrun simctl create "$simulator_name" "$device_type" "$runtime")"
  created_simulator=true
fi

echo "Booting dedicated Simulator: $simulator_name ($simulator_udid)"
xcrun simctl boot "$simulator_udid" >/dev/null 2>&1 || true
xcrun simctl bootstatus "$simulator_udid" -b

if [ "$created_simulator" = true ]; then
  # Configure a newly-created device once. Existing demo devices retain this
  # state so reruns do not reset the Simulator or re-trigger system onboarding.
  echo "Setting new Simulator language to English (United States)…"
  xcrun simctl spawn "$simulator_udid" defaults write NSGlobalDomain AppleLanguages -array en-US en
  xcrun simctl spawn "$simulator_udid" defaults write NSGlobalDomain AppleLocale -string en_US

  settings_plist="$HOME/Library/Developer/CoreSimulator/Devices/$simulator_udid/data/Containers/Shared/SystemGroup/systemgroup.com.apple.configurationprofiles/Library/ConfigurationProfiles/UserSettings.plist"
  echo "Disabling Password AutoFill on the new Simulator…"
  /usr/bin/plutil -replace restrictedBool.allowPasswordAutoFill.value -bool NO "$settings_plist"
fi

echo "Building the Release configuration…"
xcodebuild \
  -project "$root_dir/lasco-swift/Lasco.xcodeproj" \
  -scheme Lasco \
  -configuration Release \
  -destination "platform=iOS Simulator,id=$simulator_udid" \
  -derivedDataPath "$derived_data_path" \
  build \
  CODE_SIGNING_ALLOWED=NO \
  SWIFT_OPTIMIZATION_LEVEL=-Onone

app_path="$derived_data_path/Build/Products/Release-iphonesimulator/Lasco.app"
xcrun simctl uninstall "$simulator_udid" com.lasco.lasco >/dev/null 2>&1 || true
xcrun simctl install "$simulator_udid" "$app_path"

video_path="$artifact_dir/ios-lasco-cloud-demo.mp4"
timestamp="$(date +%Y%m%d-%H%M%S)"
test_output_dir="$artifact_dir/test-output-$timestamp"

echo "Recording demo…"
"$maestro_bin" --udid "$simulator_udid" test --test-output-dir "$test_output_dir" "$flow_path"

recorded_video="$(find "$test_output_dir" -type f -name '*.mp4' -print -quit)"
if [ -z "$recorded_video" ]; then
  echo "Maestro completed without producing a recording." >&2
  exit 1
fi

mv -f "$recorded_video" "$video_path"

echo "Video ready: $video_path"
