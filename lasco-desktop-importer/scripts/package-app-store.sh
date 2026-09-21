#!/bin/zsh
set -euo pipefail

project_root=${0:A:h:h}
cd "$project_root"

required_profiles=(
  "AppStore/embedded.provisionprofile"
  "AppStore/runtime.provisionprofile"
)

for profile in "${required_profiles[@]}"; do
  if [[ ! -f "$profile" ]]; then
    print -u2 "Missing $profile. See AppStore/README.md."
    exit 1
  fi
done

if ! /usr/bin/security find-identity -v -p codesigning | /usr/bin/grep -q "Mac App Distribution"; then
  print -u2 "No Mac App Distribution certificate is installed in a keychain."
  exit 1
fi

gradle_args=(packageReleasePkg -Plasco.appStore=true -Pcompose.desktop.mac.sign=true)
if [[ -n "${LASCO_MAC_SIGNING_IDENTITY:-}" ]]; then
  gradle_args+=("-Pcompose.desktop.mac.signing.identity=${LASCO_MAC_SIGNING_IDENTITY}")
fi
if [[ -n "${LASCO_MAC_SIGNING_KEYCHAIN:-}" ]]; then
  gradle_args+=("-Pcompose.desktop.mac.signing.keychain=${LASCO_MAC_SIGNING_KEYCHAIN}")
fi

exec "${GRADLE_COMMAND:-gradle}" "${gradle_args[@]}"
