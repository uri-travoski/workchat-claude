# Building WorkChat locally

Tauri 2 app: Rust core (`src-tauri`, workspace crate `workchat-proto`) plus a Svelte 5 UI built with Vite.
Common to every platform: Rust (stable, 1.85 or newer), Node 20+ and `npm ci`.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
# only if the default linker misbehaves (some distrobox setups):
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/cc CC=/usr/bin/cc CXX=/usr/bin/c++
npm ci
npm test && npm run check && cargo test --workspace
```

All output lands in `target/<triple>/release/bundle/` (Linux native build: `target/release/bundle/`).
Settings per platform live in `src-tauri/tauri.conf.json` plus `tauri.{linux,windows,android,ios}.conf.json`
(Tauri merges the matching one automatically).

## Icons

`./scripts/make-icons.sh` regenerates every icon from `../images/app-icon.svg`:
desktop png/ico/icns and Windows Square logos (`npx tauri icon`), opaque iOS icons, Android legacy + adaptive icons
(navy background, white bubble, monochrome layer for themed icons) and the Linux/flatpak set in `packaging/linux/`.
Needs `rsvg-convert` and ImageMagick.

## Linux (deb, rpm, AppImage, flatpak; x86_64 and arm64)

Debian/Ubuntu build dependencies:

```sh
sudo apt install build-essential cmake pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev libasound2-dev libopus-dev patchelf rpm file desktop-file-utils \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
```

```sh
npm run tauri build -- --bundles deb,rpm        # fast, about 4 MiB each
NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1 npm run tauri build -- --bundles appimage
```

- `NO_STRIP=true` stops linuxdeploy's bundled `strip` from choking on newer toolchains (relocation-section errors);
  `APPIMAGE_EXTRACT_AND_RUN=1` lets linuxdeploy run without FUSE (containers, distrobox).
- The AppImage bundles WebKitGTK but not the GStreamer plugins (`bundleMediaFramework` is false): bundling them took the AppImage from 105 MiB to 180 MiB in a local test. Video playback uses the system's GStreamer plugins (same as the deb/rpm
  dependencies); set it to `true` in `tauri.linux.conf.json` for a fully self-contained, much larger image (needs `patchelf`).
- **arm64 (aarch64)**: build natively on an arm64 machine with the same commands and packages (cross-compiling WebKitGTK is not set up);
  CI uses the `ubuntu-24.04-arm` runner. The outputs are `...arm64.deb`, `...aarch64.rpm`, `...aarch64.AppImage` (tauri bundles AppImages on aarch64 Linux too).
- Build on the oldest distro you want to support (CI uses Ubuntu 22.04, glibc 2.35, for x86_64; the arm64 build runs on Ubuntu 24.04, glibc 2.39); binaries do not run on older glibc.
- The `.deb`/`.rpm` install the `workchat://` URL handler through their `.desktop` file
  (`packaging/linux/workchat.desktop`, category Network;Chat;InstantMessaging).

Flatpak (wraps the prebuilt binary, GNOME runtime 48):

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
npm run tauri build -- --bundles deb            # produces target/release/workchat
./scripts/build-flatpak.sh                      # -> target/flatpak/WorkChat-<ver>-linux-x86_64.flatpak (aarch64 on an arm64 machine)
flatpak install --user target/flatpak/WorkChat-*.flatpak
```

The script picks the flatpak arch from `uname -m` (`x86_64` or `aarch64`, override with `FLATPAK_ARCH`), builds the manifest for that arch
and names the bundle `WorkChat-<ver>-linux-<arch>.flatpak`. The binary must have been built on the same CPU. The arm64 GNOME runtime
comes from flathub as well.

Validate the metadata with `appstreamcli validate --no-net packaging/flatpak/app.workchat.WorkChat.metainfo.xml`
and `desktop-file-validate packaging/flatpak/app.workchat.WorkChat.desktop`. The binary must be built against
glibc no newer than the runtime's, so use the CI binary (Ubuntu 22.04) rather than one from a very new distro.

## Windows (x64 and arm64)

Prerequisites: Visual Studio Build Tools with "Desktop development with C++" (plus the "MSVC ARM64 build tools"
component for arm64), CMake, Rust with the target, Node.

```powershell
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
npm run tauri build -- --target x86_64-pc-windows-msvc --bundles nsis
npm run tauri build -- --target aarch64-pc-windows-msvc --bundles nsis    # cross-compiles from an x64 machine
```

- Installer: `target/<triple>/release/bundle/nsis/WorkChat_<ver>_<arch>-setup.exe`. It offers per-user or
  per-machine install, downloads the WebView2 runtime if missing (`downloadBootstrapper`; use `embedBootstrapper` in
  `tauri.windows.conf.json` for an offline-capable installer, about 1.8 MB larger) and registers `workchat://`
  (`packaging/windows/hooks.nsh`).
- Portable: the raw `target/<triple>/release/workchat.exe` (UI and resources are embedded, no other files needed).
  It needs the WebView2 runtime, which ships with Windows 10 (since 2021 updates) and Windows 11. The portable exe
  does not register `workchat://`. Data is stored in the user's app-data folder unless `WORKCHAT_DATA_DIR` is set.
- Windows builds are not possible from Linux in this setup; use the CI job.

## Android (arm64-v8a and x86_64 APKs)

Prerequisites: JDK 17, Android SDK (set `ANDROID_HOME`), NDK 27.1.12297006 (`NDK_HOME`), Rust targets
`aarch64-linux-android x86_64-linux-android`, cmake and libopus headers on the host.

```sh
export ANDROID_HOME=/opt/android-sdk NDK_HOME=/opt/android-sdk/ndk/27.1.12297006
rustup target add aarch64-linux-android x86_64-linux-android
npx tauri android init --ci                 # once; creates src-tauri/gen/android
python3 scripts/android-post-init.py        # adds signing + adaptive icon; safe to re-run after every init
./scripts/android-keystore.sh               # once, creates the permanent release key outside the repo
source ~/keys/workchat/signing.env          # ANDROID_KEYSTORE_PATH / _PASSWORD / ANDROID_KEY_ALIAS / _PASSWORD
npx tauri android build --apk --split-per-abi --target aarch64 x86_64
```

APKs: `src-tauri/gen/android/app/build/outputs/apk/{arm64,x86_64}/release/`. Without the env vars the release APK is left unsigned
(`app-<abi>-release-unsigned.apk`, not installable until signed: `scripts/android-sign-local.sh`). The versionCode is set in
`tauri.android.conf.json` (`major*1000000 + minor*1000 + patch`, 1.2.0 is 1002000). minSdk is 24.

Check a signature: `$ANDROID_HOME/build-tools/<ver>/apksigner verify --print-certs app-arm64-release.apk`.

What `scripts/android-post-init.py` does after `tauri android init` (safe to re-run; `src-tauri/gen/android` is committed):
release signing from the env vars above, the adaptive icon, `MainActivity.kt` from `packaging/android/` (edge-to-edge insets,
Back button routed through the page via `window.__wcBack`, then `moveTaskToBack`), `usesCleartextTraffic="true"` (the
app itself enforces the https-or-LAN rule), `windowSoftInputMode=adjustResize`, `workchat://` and `https://*/l/<token>`
deep-link intent filters, the `fcmEnabled` manifest placeholder and, only if `app/google-services.json` exists, the Google
services Gradle plugin. The Android side of the app lives in `crates/tauri-plugin-workchat/android` (Kotlin): UnifiedPush
receiver, notification channels, the foreground "keep connection alive" service, boot receiver, FileProvider
(`WcFileProvider`, own class so it does not clash with the one Tauri generates), APK installer, save to Downloads, system
file picker, system bar colours. The Rust core is linked with the static C++ runtime (`src-tauri/build.rs`) because the audio
library (Oboe) is C++.

Test on the emulator: `adb -s emulator-5554 install -r .../app-universal-debug.apk` (from
`npx tauri android build --debug --apk --target x86_64`); the host server is `http://10.0.2.2:<port>` (plain http is allowed
for that address). `adb reverse tcp:8099 tcp:8099` lets the app reach a fake release server on the host for update tests.

## iOS (unsigned)

Needs a Mac with Xcode, `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, cmake, and CocoaPods is not needed.
No Apple account is needed for an unsigned build:

```sh
./scripts/ios-unsigned.sh 1.2.0 out     # -> out/WorkChat-1.2.0-ios-unsigned.ipa, out/WorkChat-1.2.0-ios-simulator.zip
```

The script runs `tauri ios init` if needed, copies `packaging/ios/Info.ios.plist` (usage strings, ATS local
networking, `workchat` URL scheme, remote-notification background mode) and builds with code signing disabled.
To install the simulator build: `xcrun simctl install booted WorkChat.app`.
