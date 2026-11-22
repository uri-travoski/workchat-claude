#!/usr/bin/env python3
"""Run after `tauri android init` (idempotent). Patches the generated Android project:

1. Signing: release builds are signed with the keystore from the env vars
   ANDROID_KEYSTORE_PATH, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD
   (locally: `source ~/keys/workchat/signing.env`; CI: written from the ANDROID_* secrets).
   If ANDROID_KEYSTORE_PATH is not set the release APK is left unsigned (app-*-release-unsigned.apk);
   CI names such assets "-unsigned" and scripts/android-sign-local.sh signs them with the local key.
2. Adaptive launcher icon: XML plus the background/foreground/monochrome PNGs from packaging/android-icons
   (the Tauri init only copies ic_launcher*.png and the foreground, not the anydpi-v26 XML or the rest).
3. WorkChat integration: MainActivity (insets, Back button), manifest (cleartext for private hosts, adjustResize,
   deep links workchat:// and https://<any host>/l/<token>), Google push only when app/google-services.json exists.
"""
import pathlib, re, shutil, sys

root = pathlib.Path(__file__).resolve().parent.parent
app = root / "src-tauri/gen/android/app"
gradle = app / "build.gradle.kts"
if not gradle.exists():
    sys.exit(f"{gradle} not found: run `npx tauri android init` first")

MARK = "// workchat-signing"
text = gradle.read_text()
if MARK not in text:
    signing = f'''    {MARK}
    signingConfigs {{
        create("workchatRelease") {{
            val ks = System.getenv("ANDROID_KEYSTORE_PATH")
            if (!ks.isNullOrEmpty()) {{
                storeFile = file(ks)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }}
        }}
    }}
'''
    new, n = re.subn(r"(\n    buildTypes \{)", "\n" + signing.rstrip("\n") + r"\1", text, count=1)
    if n != 1:
        sys.exit("could not find `buildTypes {` in build.gradle.kts")
    use = '''        getByName("release") {
            if (!System.getenv("ANDROID_KEYSTORE_PATH").isNullOrEmpty()) {
                signingConfig = signingConfigs.getByName("workchatRelease")
            }'''
    new, n = re.subn(r'        getByName\("release"\) \{', use, new, count=1)
    if n != 1:
        sys.exit('could not find getByName("release") in build.gradle.kts')
    gradle.write_text(new)
    print("patched signing into", gradle)
else:
    print("signing already patched")

res_src = root / "packaging/android-icons/res"
res_dst = app / "src/main/res"
for png in res_src.glob("mipmap-*/ic_launcher_*.png"):
    d = res_dst / png.parent.name
    d.mkdir(parents=True, exist_ok=True)
    shutil.copy(png, d / png.name)
src = res_src / "mipmap-anydpi-v26"
dst = res_dst / "mipmap-anydpi-v26"
dst.mkdir(parents=True, exist_ok=True)
for f in src.glob("*.xml"):
    shutil.copy(f, dst / f.name)
for stale in ("drawable/ic_launcher_background.xml", "drawable-v24/ic_launcher_foreground.xml"):
    p = app / "src/main/res" / stale
    if p.exists():
        p.unlink()
print("adaptive icon xml installed")


# ---------------------------------------------------------------- 3. WorkChat integration
pkg = app / "src/main/java/app/workchat"
shutil.copy(root / "packaging/android/MainActivity.kt", pkg / "MainActivity.kt")
print("MainActivity installed")

text = gradle.read_text()
# Plain http is allowed by the manifest; the app itself only connects over http to private/loopback/.local hosts.
text = text.replace('manifestPlaceholders["usesCleartextTraffic"] = "false"', 'manifestPlaceholders["usesCleartextTraffic"] = "true"')
if 'manifestPlaceholders["fcmEnabled"]' not in text:
    has_fcm = (app / "google-services.json").exists()
    text = text.replace('        applicationId = "app.workchat"', f'        manifestPlaceholders["fcmEnabled"] = "{str(has_fcm).lower()}"\n        applicationId = "app.workchat"', 1)
if (app / "google-services.json").exists() and "com.google.gms.google-services" not in text:
    text = text.replace('    id("rust")', '    id("rust")\n    id("com.google.gms.google-services")', 1)
    top = app.parent / "build.gradle.kts"
    t = top.read_text()
    if "com.google.gms:google-services" not in t:
        top.write_text(t.replace('classpath("org.jetbrains.kotlin:kotlin-gradle-plugin', 'classpath("com.google.gms:google-services:4.4.2")\n        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin', 1))
    print("Google push enabled (google-services.json found)")
gradle.write_text(text)

mf = app / "src/main/AndroidManifest.xml"
m = mf.read_text()
if "workchat-links" not in m:
    m = m.replace('android:launchMode="singleTask"', 'android:launchMode="singleTask"\n            android:windowSoftInputMode="adjustResize"', 1)
    links = """
            <!-- workchat-links: workchat://login, workchat://chat/<id>, https://<server>/l/<token> -->
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="workchat" />
            </intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="https" android:host="*" android:pathPrefix="/l/" />
                <data android:scheme="http" android:host="*" android:pathPrefix="/l/" />
            </intent-filter>
"""
    m = m.replace("        </activity>", links + "        </activity>", 1)
    mf.write_text(m)
    print("manifest: adjustResize and deep links")
