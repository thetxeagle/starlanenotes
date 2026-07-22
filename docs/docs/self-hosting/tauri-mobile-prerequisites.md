# Tauri Mobile Prerequisites (iOS + Android)

Use this checklist before running mobile dev commands in Starlane Notes.

## Android

1. Install Android command-line tooling:

```bash
brew install --cask android-commandlinetools
brew install openjdk@17
```

2. Install SDK platform + build tools + NDK via `sdkmanager`:

```bash
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"
export PATH="/opt/homebrew/opt/openjdk@17/bin:$PATH"
mkdir -p "$HOME/Library/Android/sdk"
yes | sdkmanager --sdk_root="$HOME/Library/Android/sdk" --licenses
sdkmanager --sdk_root="$HOME/Library/Android/sdk" \
  "platform-tools" \
  "platforms;android-34" \
  "build-tools;34.0.0" \
  "ndk;26.3.11579264" \
  "cmake;3.22.1"
```

3. Export SDK path:

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"
export PATH="$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools:$PATH"
```

4. Verify tooling:

```bash
echo "$ANDROID_HOME"
adb version
```

5. Initialize and run:

```bash
npm run mobile:android:init
npm run mobile:android:studio
npm run mobile:android:dev
npm run mobile:android:build
```

The Android Studio project root is:

```text
apps/desktop/src-tauri/gen/android
```

Use Android Studio for SDK management, emulator/device tools, Gradle diagnostics, and Image Asset Studio experiments. Use the Tauri wrapper commands for authoritative app build/run validation.

Common blocker:

- `Android SDK not found` means `ANDROID_HOME` is unset or points to the wrong path.
- `Unable to locate a Java Runtime` means `JAVA_HOME`/`PATH` do not include the installed JDK.
- `Unsupported class file major version` means JDK is too new for Gradle/Groovy; use JDK 17.
- Local emulator/device builds do not require a Google account. Play Store distribution requires a Google Play Developer account and release signing setup.

## iOS

1. Install full Xcode and command line tools.
2. Accept Xcode license (admin action) and open Xcode at least once:

```bash
sudo xcodebuild -license
```

3. Optionally switch active developer directory (admin action):

```bash
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
```

If you cannot switch globally, set `DEVELOPER_DIR` per command (the project scripts now do this automatically).
4. Verify simulator visibility:

```bash
xcrun simctl list --json devices available
```

5. Initialize iOS project files:

```bash
npm run mobile:ios:init
```

6. Set signing team for device builds:

- Recommended: `APPLE_DEVELOPMENT_TEAM` environment variable (picked up by `mobile:ios:*` wrapper scripts)
- Optional: `APPLE_BUNDLE_IDENTIFIER` environment variable for local bundle-id override
- Alternative: Tauri iOS config `bundle > iOS > developmentTeam`

7. Run readiness check:

```bash
APPLE_DEVELOPMENT_TEAM=<team-id> npm run mobile:ios:doctor
```

8. Build through the Tauri wrapper, not by pressing Build directly in Xcode:

```bash
APPLE_DEVELOPMENT_TEAM=<team-id> ./scripts/mobile/ios-tauri.sh build --debug -t aarch64-sim --ci
APPLE_DEVELOPMENT_TEAM=<team-id> ./scripts/mobile/ios-tauri.sh build --debug -t aarch64 --ci
```

For the current Starlane Notes development signing lane, use:

```bash
APPLE_DEVELOPMENT_TEAM=H9FVZ6838A npm run mobile:ios:doctor
APPLE_DEVELOPMENT_TEAM=H9FVZ6838A npm run mobile:ios:build
```

The physical-device build exports:

```text
apps/desktop/src-tauri/gen/apple/build/arm64/Starlane Notes.ipa
```

To install the exported app on a paired iPhone without using the Xcode Run button:

```bash
xcrun devicectl list devices
mkdir -p /tmp/starlane-ios-install
unzip -q "apps/desktop/src-tauri/gen/apple/build/arm64/Starlane Notes.ipa" -d /tmp/starlane-ios-install
xcrun devicectl device install app --device <device-identifier> "/tmp/starlane-ios-install/Payload/Starlane Notes.app"
xcrun devicectl device process launch --device <device-identifier> --terminate-existing dev.starlane.notes
```

Xcode/CoreDevice can install and launch on a paired iPhone over the local wireless connection after the phone has trusted the Mac and been enabled for development. A cable is usually required for the initial trust/developer setup, but not for every later install.

Common blockers:

- Missing generated project (`gen/apple` absent): run `mobile:ios:init`.
- `xcrun simctl ... exited with code 72` often means Xcode license is not accepted or active developer directory is not full Xcode.
- Simulator discovery/signing errors: fix Xcode + team configuration first.
- `mobile:ios:doctor` warning about identities/profiles means device signing is not provisioned yet.
- Xcode signing errors like `No Account for Team` usually mean `APPLE_DEVELOPMENT_TEAM` does not match the installed provisioning profile's team identifier. Decode the active `.mobileprovision` profile or check Xcode Accounts, then use that team ID in the Tauri command.
- For the current local setup, the Apple Development certificate display name includes `2U7A4G64TU`, but the installed wildcard provisioning profile belongs to team `H9FVZ6838A`. Use `H9FVZ6838A` in `APPLE_DEVELOPMENT_TEAM`.
- Xcode GUI build failures mentioning `*-server-addr` mean Xcode was launched directly without Tauri's temporary build context. Run through `mobile:ios:*` scripts.
- Xcode GUI build failures mentioning `failed to build WebSocket client` or `Connection refused` mean Tauri's xcode-script cannot reach the helper process started by `cargo tauri ios dev`. Run from `npm run mobile:ios:dev` or `./scripts/mobile/ios-tauri.sh dev` instead of pressing Build directly in Xcode.
- If a physical iPhone cannot find the frontend dev server, run through `npm run mobile:ios:dev -- --host <mac-lan-ip>`. The web Vite config listens on `0.0.0.0` and sets HMR to `TAURI_DEV_HOST` when Tauri provides that mobile host.
- Xcode GUI build failures mentioning `cargo: command not found` mean the generated script cannot find Rust. Regenerate or keep the Xcode build script path export.
- Xcode GUI build failures mentioning `Operation not permitted` for `project.pbxproj` mean user script sandboxing is blocking Tauri's build helper.

## Related Commands

```bash
npm run mobile:android:init
npm run mobile:android:studio
npm run mobile:android:dev
npm run mobile:android:build
npm run mobile:ios:init
npm run mobile:ios:dev
npm run mobile:ios:doctor
```
