#!/usr/bin/env bash
set -euo pipefail

(("$#" > 0)) || {
  echo "usage: notarize.sh <app-dmg-or-zip> [...]" >&2
  exit 1
}
: "${APP_STORE_CONNECT_KEY_ID:?}"
: "${APP_STORE_CONNECT_ISSUER_ID:?}"
: "${APP_STORE_CONNECT_KEY_BASE64:?}"
key="$RUNNER_TEMP/AuthKey.p8"
echo "$APP_STORE_CONNECT_KEY_BASE64" | base64 --decode > "$key"

notarize() {
  xcrun notarytool submit "$1" --key "$key" --key-id "$APP_STORE_CONNECT_KEY_ID" \
    --issuer "$APP_STORE_CONNECT_ISSUER_ID" --wait
}

for artifact in "$@"; do
  case "$artifact" in
    *.app)
      [[ -d "$artifact" ]] || {
        echo "missing application: $artifact" >&2
        exit 1
      }
      archive="$RUNNER_TEMP/$(basename "$artifact" .app).zip"
      ditto -c -k --keepParent "$artifact" "$archive"
      notarize "$archive"
      xcrun stapler staple "$artifact"
      spctl --assess --type exec --verbose=2 "$artifact"
      ;;
    *.dmg)
      [[ -f "$artifact" ]] || {
        echo "missing disk image: $artifact" >&2
        exit 1
      }
      notarize "$artifact"
      xcrun stapler staple "$artifact"
      ;;
    *.zip)
      [[ -f "$artifact" ]] || {
        echo "missing archive: $artifact" >&2
        exit 1
      }
      # A ticket cannot be stapled to a zip or a bare command-line tool; Gatekeeper fetches it online instead.
      notarize "$artifact"
      ;;
    *)
      echo "unsupported notarization artifact: $artifact" >&2
      exit 1
      ;;
  esac
done
rm -f "$key"
