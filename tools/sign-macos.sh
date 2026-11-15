#!/usr/bin/env bash
# Signs the Pulley bridge app with a Developer ID certificate (hardened runtime), then notarizes
# and staples the disk image, if credentials are configured. Without them it says why it skipped
# and exits 0, so packaging works without certificates.
#
#   tools/sign-macos.sh dist/installers/Pulley.app dist/installers/Pulley_0.1.0_aarch64.dmg
#
# Signing:      PULLEY_MAC_SIGN_IDENTITY  e.g. "Developer ID Application: Your Name (TEAMID)"
#               (the certificate must be in the login keychain)
# Notarizing:   PULLEY_MAC_NOTARY_PROFILE a profile stored with
#               `xcrun notarytool store-credentials <profile> --apple-id … --team-id …`
# UNTESTED: written without access to a Mac. Check it on the first macOS release.
set -euo pipefail
app="${1:?usage: sign-macos.sh <Pulley.app> [<image.dmg>]}"
dmg="${2:-}"

if [ -z "${PULLEY_MAC_SIGN_IDENTITY:-}" ]; then
  echo "macOS signing skipped: set PULLEY_MAC_SIGN_IDENTITY (and PULLEY_MAC_NOTARY_PROFILE to notarize)."
  exit 0
fi

# Sign the executable first, then the bundle (no --deep: it signs nested code in the wrong order).
codesign --force --options runtime --timestamp --sign "$PULLEY_MAC_SIGN_IDENTITY" "$app/Contents/MacOS/pulley-bridge"
codesign --force --options runtime --timestamp --sign "$PULLEY_MAC_SIGN_IDENTITY" "$app"
codesign --verify --strict --verbose=2 "$app"
echo "signed: $app"

if [ -n "$dmg" ]; then
  codesign --force --timestamp --sign "$PULLEY_MAC_SIGN_IDENTITY" "$dmg"
  if [ -z "${PULLEY_MAC_NOTARY_PROFILE:-}" ]; then
    echo "notarization skipped: set PULLEY_MAC_NOTARY_PROFILE."
    exit 0
  fi
  xcrun notarytool submit "$dmg" --keychain-profile "$PULLEY_MAC_NOTARY_PROFILE" --wait
  xcrun stapler staple "$dmg"
  spctl --assess --type open --context context:primary-signature --verbose "$dmg"
  echo "notarized and stapled: $dmg"
fi
