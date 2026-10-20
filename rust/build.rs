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
        root.clone(),
    ] {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    for path in [
        "portable/core/libclover_ares.a",
        "portable/core/clover_ares.lib",
        "libclover_native_menu.a",
    ] {
        let path = root.join(path);
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    // Pinned native static libraries used by the Rust desktop package.
    println!("cargo:rustc-link-lib=static=clover_ares");
    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if windows {
        // SDL's archive name differs from its DLL import library on MSVC.
        println!("cargo:rustc-link-lib=static=SDL3-static");
    } else {
        println!("cargo:rustc-link-lib=static=SDL3");
    }
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
    } else if windows {
        for library in [
            "kernel32", "user32", "gdi32", "winmm", "imm32", "ole32", "oleaut32", "version",
            "uuid", "advapi32", "setupapi", "shell32", "shlwapi", "dinput8", "ws2_32",
        ] {
            println!("cargo:rustc-link-lib={library}");
        }
    } else {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=dl");
    }
}
