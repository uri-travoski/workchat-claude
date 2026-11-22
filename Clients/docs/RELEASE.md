# Releasing WorkChat

This folder lives in the WorkChat monorepo; the workflows are in the root `.github/workflows/` (`apps-ci.yml`, `release.yml`). One tag `vX.Y.Z` releases the server image, the server install files and every app together. Paths below are relative to `Clients/` unless they start with `workchat-server/` or `CHANGELOG.md`.

Repository: https://github.com/uri-travoski/workchat-claude (public). The first public release was **v1.1.0**, titled "WorkChat v1.1"; the current one is **v1.2.0**, "WorkChat v1.2"
(title rule: `WorkChat v<version without a trailing .0>`).

## Steps

1. Update the version in `src-tauri/tauri.conf.json`, `Cargo.toml` (`[workspace.package]`) and `package.json`
   (all three must match; the workflow checks the tag against `tauri.conf.json`, `workchat-server/Cargo.toml` and the image pinned in `workchat-server/deploy/docker-compose.yml`, so bump those too), and the Android `versionCode` in
   `src-tauri/tauri.android.conf.json` (`major*1000000 + minor*1000 + patch`, so 1.2.0 is 1002000; the workflow checks it too).
   Also add a `<release>` entry to `packaging/flatpak/app.workchat.WorkChat.metainfo.xml`.
2. Add a `## [x.y.z] - date` section (with `### Server` and `### Apps`) to the root `CHANGELOG.md`. It becomes the release notes.
3. Run locally: `npm test && npm run check && cargo test --workspace`.
4. Commit, then tag and push:
   ```sh
   git tag v1.2.0 && git push origin main v1.2.0
   ```
5. The **Release** workflow builds everything and creates the GitHub release with all assets and `SHA256SUMS`.
   To rebuild an existing tag, run the workflow manually (Actions, Release, Run workflow) and enter the tag.

Workflow `CI` runs on every push and pull request (TypeScript check and tests, `cargo test`, a Linux deb build).

## Asset names (a contract: do not rename)

The in-app update checker reads `https://api.github.com/repos/uri-travoski/workchat-claude/releases/latest` and picks the asset for the
current platform by these exact names. `<ver>` is the version without the `v` (tag `v1.2.0` gives `1.2.0`). Changing a name breaks
update detection in installed apps. Android `-unsigned` files intentionally do not match the update checker's names.

| Asset | Notes |
|---|---|
| `WorkChat-<ver>-windows-x64-setup.exe`, `-windows-arm64-setup.exe` | NSIS installers (per user or per machine) |
| `WorkChat-<ver>-windows-x64-portable.exe`, `-windows-arm64-portable.exe` | raw exe, needs WebView2 (built into Windows 10/11) |
| `WorkChat-<ver>-linux-amd64.deb`, `-linux-x86_64.rpm`, `-linux-x86_64.AppImage`, `-linux-x86_64.flatpak` | Linux x86_64, built on Ubuntu 22.04 (glibc 2.35) |
| `WorkChat-<ver>-linux-arm64.deb`, `-linux-aarch64.rpm`, `-linux-aarch64.AppImage`, `-linux-aarch64.flatpak` | Linux arm64, built natively on the `ubuntu-24.04-arm` runner (since 1.2.0) |
| `WorkChat-<ver>-android-arm64.apk`, `-android-x86_64.apk` | signed with the release key; when CI has no key the files are named `WorkChat-<ver>-android-<abi>-unsigned.apk` instead |
| `WorkChat-<ver>-ios-unsigned.ipa`, `-ios-simulator.zip` | no code signature |
| `SHA256SUMS` | `sha256sum -c SHA256SUMS` |

Naming rule for Linux: Debian packages use the Debian arch (`amd64`, `arm64`), everything else the kernel arch (`x86_64`, `aarch64`).
The update checker detects the CPU (`std::env::consts::ARCH`) and picks the matching set; an unsupported CPU opens the release page.
The Linux job is a matrix (`x86_64`, `aarch64`) producing the artifacts `assets-linux-x86_64` and `assets-linux-aarch64`. arm64 binaries
need glibc 2.39 or newer (Ubuntu 24.04 build), so they do not run on older distros such as Ubuntu 22.04 or Debian 12.

The iOS job is `continue-on-error`: if unsigned packaging breaks on a new Xcode, the rest of the release still goes out
(the release is only skipped when Linux (x86_64 or arm64), Windows or Android fails).

## Android APKs without CI secrets

If `ANDROID_KEYSTORE_B64` is not set the Android job still succeeds: it builds the APKs unsigned, names them
`WorkChat-<ver>-android-arm64-unsigned.apk` / `-x86_64-unsigned.apk`, adds a warning plus a job summary note, and the
release notes say so. Replace them with locally signed ones (uses `~/keys/workchat/signing.env`):

```sh
gh release download v1.2.0 -R uri-travoski/workchat-claude -p '*android*unsigned.apk' -D apks
./scripts/android-sign-local.sh 1.2.0 apks     # zipalign + apksigner, writes WorkChat-1.2.0-android-{arm64,x86_64}.apk
# then run the gh commands it prints (delete the -unsigned assets, upload the signed ones, refresh SHA256SUMS)
```

## Secrets (repository Settings, Secrets and variables, Actions)

| Secret | Required | What |
|---|---|---|
| `ANDROID_KEYSTORE_B64` | for a properly signed APK | `base64 -w0 ~/keys/workchat/workchat-release.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | with the keystore | value of `ANDROID_KEYSTORE_PASSWORD` in `~/keys/workchat/signing.env` |
| `ANDROID_KEY_ALIAS` | with the keystore | `workchat` |
| `ANDROID_KEY_PASSWORD` | with the keystore | value in `signing.env` |
| `GOOGLE_SERVICES_JSON_B64` | optional (FCM) | `base64 -w0 google-services.json` |

Create the key once with `./scripts/android-keystore.sh` (it lives outside the repo in `~/keys/workchat/`, mode 600; back it up,
losing it means existing installs cannot be updated). Set the secrets with, for example,
`gh secret set ANDROID_KEYSTORE_B64 < <(base64 -w0 ~/keys/workchat/workchat-release.jks)`.
No other secrets are needed: `GITHUB_TOKEN` publishes the release.

## Adding Firebase (FCM) later

1. Create a Firebase project, add an Android app with package `app.workchat`, download `google-services.json`.
2. Store it as the `GOOGLE_SERVICES_JSON_B64` secret. The Android job writes it to `src-tauri/gen/android/app/google-services.json`.
3. Nothing else to change: `scripts/android-post-init.py` applies the Google services Gradle plugin only when
   `app/google-services.json` exists, and the push plugin then compiles its real `FirebaseMessagingService` (source set
   `src/fcm/kotlin`) instead of a stub (`src/nofcm/kotlin`), so builds without the file keep working and ship no Firebase code.
4. Put the Firebase service-account credentials into the server's push settings (admin panel).

## Adding Apple (APNs, signing) later

Today the iOS build is unsigned (sideload or simulator), so no Apple secret is needed; APNs registration needs a signed build
with the Push Notifications capability. When you have an Apple Developer account: create the App ID `app.workchat` with Push
Notifications, an APNs key (.p8) for the server's push settings, and for signed IPAs add secrets for the distribution certificate
(p12 + password), provisioning profile and `APPLE_DEVELOPMENT_TEAM`, then replace `scripts/ios-unsigned.sh` in the iOS job
with `npx tauri ios build --export-method app-store-connect` (or `release-testing`).

## Sideloading the unsigned IPA

The `.ipa` has no signature, so iOS will not run it until it is re-signed with your own Apple ID:

- **AltStore / SideStore** (free Apple ID, 7-day certificate, auto refresh): open the IPA in AltStore on the device.
- **Sideloadly** (Windows/macOS): drag the IPA in, enter your Apple ID, install; re-install every 7 days with a free ID.
- **TrollStore** (some iOS versions only) installs unsigned IPAs permanently.
- Then trust the profile under Settings, General, VPN and Device Management. iOS 16+ also needs Developer Mode on.

The simulator zip (`-ios-simulator.zip`) runs on a Mac without any signing: unzip, then
`xcrun simctl install booted WorkChat.app && xcrun simctl launch booted app.workchat`.

## Smoke test after a release

Download the assets and `sha256sum -c SHA256SUMS`; install the deb in a throwaway container or the AppImage directly (on arm64 hardware or a VM for the `arm64`/`aarch64` files)
(`chmod +x`, run); on Android `apksigner verify --print-certs` must show the fingerprint printed by
`keytool -list -v -keystore ~/keys/workchat/workchat-release.jks`.
