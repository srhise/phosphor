#!/usr/bin/env bash
# Cut a public release: a signed, notarized, universal DMG attached to a
# draft GitHub release, plus a Homebrew cask file ready for the tap.
#
# One-time setup this script checks for and explains if missing:
#   1. A "Developer ID Application" certificate in the keychain
#      (developer.apple.com -> Certificates, or Xcode -> Settings ->
#      Accounts -> Manage Certificates -> "+").
#   2. Notary credentials stored as a keychain profile:
#      xcrun notarytool store-credentials phosphor-notary \
#          --apple-id <apple id> --team-id <team id> \
#          --password <app-specific password from account.apple.com>
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
TAG="v$VERSION"
NOTARY_PROFILE=${NOTARY_PROFILE:-phosphor-notary}
DMG="target/Phosphor-$VERSION.dmg"

# --- preflight -------------------------------------------------------------

fail() { echo "error: $*" >&2; exit 1; }

[ -z "$(git status --porcelain)" ] || fail "working tree is not clean"
[ "$(git branch --show-current)" = "main" ] || fail "not on main"
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null \
    && fail "tag $TAG already exists; bump the version in Cargo.toml"

IDENTITY=${SIGN_IDENTITY:-$(security find-identity -v -p codesigning \
    | awk -F'"' '/Developer ID Application/ {print $2; exit}')}
[ -n "$IDENTITY" ] || fail 'no "Developer ID Application" certificate in the
keychain. Create one at developer.apple.com -> Certificates (type:
Developer ID Application) or in Xcode -> Settings -> Accounts -> Manage
Certificates, then run this again.'

xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1 \
    || fail "no notary credentials under profile '$NOTARY_PROFILE'. Run:
  xcrun notarytool store-credentials $NOTARY_PROFILE \\
      --apple-id <apple id> --team-id <team id> \\
      --password <app-specific password from account.apple.com>"

gh auth status >/dev/null || fail "gh is not authenticated"

# --- build -----------------------------------------------------------------

echo "==> tests"
cargo test --release --quiet

echo "==> building universal binary"
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
mkdir -p target/universal
lipo -create \
    target/aarch64-apple-darwin/release/phosphor \
    target/x86_64-apple-darwin/release/phosphor \
    -output target/universal/phosphor

PHOSPHOR_BIN=target/universal/phosphor SIGN_IDENTITY="$IDENTITY" ./tools/package.sh

# --- notarize the app, then the DMG ----------------------------------------
# Notarizing and stapling the .app itself means it passes Gatekeeper even
# offline once copied out of the DMG; notarizing the DMG covers the download.

echo "==> notarizing app"
ditto -c -k --keepParent target/Phosphor.app target/phosphor-app.zip
xcrun notarytool submit target/phosphor-app.zip \
    --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple target/Phosphor.app

echo "==> building $DMG"
STAGE=target/dmg-stage
rm -rf "$STAGE"; mkdir -p "$STAGE"
cp -R target/Phosphor.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "Phosphor" -srcfolder "$STAGE" -format UDZO "$DMG"
codesign --force --timestamp --sign "$IDENTITY" "$DMG"

echo "==> notarizing DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$DMG"

echo "==> verifying"
spctl --assess --type execute --verbose target/Phosphor.app
xcrun stapler validate "$DMG"

# --- Homebrew cask ----------------------------------------------------------

SHA=$(shasum -a 256 "$DMG" | awk '{print $1}')
cat > target/phosphor.rb <<CASK
cask "phosphor" do
  version "$VERSION"
  sha256 "$SHA"

  url "https://github.com/srhise/phosphor/releases/download/v#{version}/Phosphor-#{version}.dmg"
  name "Phosphor"
  desc "Distraction-free writer that emulates VGA text mode"
  homepage "https://github.com/srhise/phosphor"

  depends_on macos: ">= :big_sur"

  app "Phosphor.app"

  zap trash: [
    "~/Library/Application Support/phosphor",
  ]
end
CASK

# --- publish (draft) ---------------------------------------------------------

echo "==> tagging $TAG and creating draft release"
git tag -a "$TAG" -m "phosphor $TAG"
git push origin "$TAG"
gh release create "$TAG" "$DMG" --draft \
    --title "phosphor $TAG" \
    --generate-notes

echo
echo "Draft release created. Next steps:"
echo "  1. Review and publish: gh release edit $TAG --draft=false"
echo "  2. Copy target/phosphor.rb into the srhise/homebrew-tap repo (Casks/)"
