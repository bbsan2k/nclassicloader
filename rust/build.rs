use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CLOVER_NATIVE_BUILD_DIR");
    let native_app = env::var_os("CARGO_FEATURE_NATIVE_APP").is_some();
    let native_ares = env::var_os("CARGO_FEATURE_NATIVE_ARES").is_some();
    if !native_app && !native_ares {
        return;
    }
    let root = env::var_os("CLOVER_NATIVE_BUILD_DIR")
        .map(PathBuf::from)
        .expect("native-ares/native-app requires CLOVER_NATIVE_BUILD_DIR from the CMake build");
    let mut search_paths = vec![root.join("portable/core"), root.clone()];
    if native_app {
        search_paths.push(root.join("third_party/SDL"));
    }
    for path in search_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    for path in [
        "portable/core/libclover_ares.a",
        "portable/core/clover_ares.lib",
        "libclover_ares.a",
        "clover_ares.lib",
    ] {
        let path = root.join(path);
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    if native_app {
        let path = root.join("libclover_native_menu.a");
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    // The standalone core archive is the only native dependency for native-ares.
    println!("cargo:rustc-link-lib=static=clover_ares");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let apple = matches!(target_os.as_str(), "macos" | "ios" | "tvos");
    if !native_app {
        if apple {
            println!("cargo:rustc-link-lib=c++");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
        } else if target_os == "windows" {
            for library in [
                "ws2_32", "winmm", "ole32", "shell32", "shlwapi", "advapi32", "user32", "uuid",
            ] {
                println!("cargo:rustc-link-lib={library}");
            }
        } else {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=dl");
        }
        return;
    }
    // Pinned native static libraries used by the desktop application.
    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if windows {
        // SDL's archive name differs from its DLL import library on MSVC.
        println!("cargo:rustc-link-lib=static=SDL3-static");
    } else {
        println!("cargo:rustc-link-lib=static=SDL3");
    }
    if target_os == "macos" {
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
    } else if apple {
        println!("cargo:rustc-link-lib=c++");
        for framework in [
            "CoreMedia",
            "CoreVideo",
            "UIKit",
            "CoreAudio",
            "AudioToolbox",
            "AVFoundation",
            "Foundation",
            "GameController",
            "Metal",
            "QuartzCore",
            "CoreFoundation",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
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
