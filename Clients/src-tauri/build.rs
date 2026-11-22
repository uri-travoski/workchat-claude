fn main() {
    // The audio library (Oboe) is C++: on Android link the static C++ runtime after everything else so symbols such as
    // __gxx_personality_v0 and the libc++abi helpers resolve (libc++_shared.so is then not needed in the APK).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-cdylib-link-arg=-lc++_static");
        println!("cargo:rustc-cdylib-link-arg=-lc++abi");
    }
    tauri_build::build()
}
