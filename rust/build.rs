use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CLOVER_NATIVE_BUILD_DIR");
    if env::var_os("CARGO_FEATURE_NATIVE_APP").is_none() {
        return;
    }
    let root = env::var_os("CLOVER_NATIVE_BUILD_DIR")
        .map(PathBuf::from)
        .expect("native-app requires CLOVER_NATIVE_BUILD_DIR from the CMake build");
    for path in [
        root.join("portable/core"),
        root.join("third_party/SDL"),
        root.join("third_party/zlib"),
        root.clone(),
    ] {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    // The same pinned static libraries and platform dependencies as the CMake app.
    println!("cargo:rustc-link-lib=static=clover_ares");
    println!("cargo:rustc-link-lib=static=SDL3");
    println!("cargo:rustc-link-lib=static=z");
    println!("cargo:rustc-link-lib=m");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=static=clover_native_menu");
        println!("cargo:rustc-link-lib=c++");
        for framework in [
            "CoreMedia",
            "CoreVideo",
            "Cocoa",
            "IOKit",
            "ForceFeedback",
            "Carbon",
            "CoreAudio",
            "AudioToolbox",
            "AVFoundation",
            "Foundation",
            "GameController",
            "Metal",
            "QuartzCore",
            "AppKit",
            "CoreFoundation",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,UniformTypeIdentifiers");
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,CoreHaptics");
    } else if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for library in [
            "ws2_32", "winmm", "ole32", "shell32", "shlwapi", "advapi32", "user32", "uuid",
        ] {
            println!("cargo:rustc-link-lib={library}");
        }
    } else {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=dl");
    }
}
