#!/usr/bin/env bash
# Records the complete iOS Lasco Cloud demo on an isolated Simulator.
# To inject secrets explicitly without HTTP retries, invoke this script with:
# doppler run --attempts 1 --scope ../lasco-cloud --config dev_personal -- maestro/ios-demo/record-ios-demo.sh
# This script never invokes Doppler itself.
set -euo pipefail

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
flow_path="$root_dir/maestro/ios-demo/ios-demo.yaml"
artifact_dir="$root_dir/maestro/ios-demo/artifacts"
derived_data_path="${DERIVED_DATA_PATH:-/tmp/lasco-maestro-release}"

simulator_name="${SIMULATOR_NAME:-Lasco Maestro iOS}"
device_type="${SIMULATOR_DEVICE_TYPE:-com.apple.CoreSimulator.SimDeviceType.iPhone-17}"
runtime="${SIMULATOR_RUNTIME:-com.apple.CoreSimulator.SimRuntime.iOS-26-0}"

export MAESTRO_LIBRARY_NAME="Lasco play store review"
export MAESTRO_LIBRARY_USERNAME="username"
export MAESTRO_LIBRARY_PASSWORD="1234567"

required_s3_variables=(
  MAESTRO_S3_REMOTE_NAME
  MAESTRO_S3_REMOTE_ENDPOINT
  MAESTRO_S3_REMOTE_BUCKET
  MAESTRO_S3_REMOTE_REGION
  MAESTRO_S3_REMOTE_PATH_PREFIX
  MAESTRO_S3_REMOTE_ACCESS_KEY
  MAESTRO_S3_REMOTE_SECRET_KEY
)

export MAESTRO_CLOUD_EMAIL="${MAESTRO_CLOUD_EMAIL:-${MAESTRO_REVIEW_LASCO_CLOUD_EMAIL:-}}"
export MAESTRO_CLOUD_PASSWORD="${MAESTRO_CLOUD_PASSWORD:-${MAESTRO_REVIEW_LASCO_CLOUD_PASSWORD:-}}"

if [ -z "$MAESTRO_CLOUD_EMAIL" ] || [ -z "$MAESTRO_CLOUD_PASSWORD" ]; then
  echo "Lasco Cloud credentials are required. Provide MAESTRO_CLOUD_EMAIL and MAESTRO_CLOUD_PASSWORD before running this script." >&2
  exit 1
fi

for variable_name in "${required_s3_variables[@]}"; do
  if [ -z "${!variable_name:-}" ]; then
    echo "$variable_name is required. Provide all MAESTRO_S3_REMOTE_* variables before running this script." >&2
    exit 1
  fi
done

if ! command -v xcodebuild >/dev/null || ! command -v xcrun >/dev/null; then
  echo "This script requires Xcode command-line tools." >&2
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

mkdir -p "$artifact_dir"

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
  echo "Setting new Simulator language to English (United States)…"
  xcrun simctl spawn "$simulator_udid" defaults write NSGlobalDomain AppleLanguages -array en-US en
  xcrun simctl spawn "$simulator_udid" defaults write NSGlobalDomain AppleLocale -string en_US
fi

echo "Building the Release configuration…"
xcodebuild \
  -project "$root_dir/lasco-swift/Lasco.xcodeproj" \
  -scheme Lasco \
  -configuration Release \
  -destination "platform=iOS Simulator,id=$simulator_udid" \
  -derivedDataPath "$derived_data_path" \
  build \
  SWIFT_OPTIMIZATION_LEVEL=-Onone

app_path="$derived_data_path/Build/Products/Release-iphonesimulator/Lasco.app"
xcrun simctl uninstall "$simulator_udid" com.lasco.lasco >/dev/null 2>&1 || true
xcrun simctl install "$simulator_udid" "$app_path"

video_path="$artifact_dir/ios-lasco-demo.mp4"
timestamp="$(date +%Y%m%d-%H%M%S)"
test_output_dir="$artifact_dir/test-output-$timestamp"
mkdir -p "$test_output_dir"

echo "Recording demo…"
maestro_console_log="$test_output_dir/maestro-console.log"
if ! "$maestro_bin" --udid "$simulator_udid" test --test-output-dir "$test_output_dir" "$flow_path" >"$maestro_console_log" 2>&1; then
  echo "Maestro recording failed. Debug artifacts: $test_output_dir" >&2
  exit 1
fi

recorded_video="$(find "$test_output_dir" -type f -name '*.mp4' -print -quit)"
if [ -z "$recorded_video" ]; then
  echo "Maestro completed without producing a recording." >&2
  exit 1
fi

mv -f "$recorded_video" "$video_path"

echo "Video ready: $video_path"
