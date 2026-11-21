//! Compiles the UI and links libmpv: on iOS statically, from the MPVKit xcframeworks that ios/package.sh unpacks into
//! $UFLIX_IOS_LIBS (each `<Name>.xcframework/ios-arm64/` holds a static `<Name>.framework` or a `lib*.a`); on Linux
//! against the bundled `runtimes/linux-x64/native/libmpv.so.2` (local check only). Android: to do.

use std::path::{Path, PathBuf};

fn main() {
    slint_build::compile("ui/player.slint").expect("slint compile");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    println!("cargo:rerun-if-env-changed=UFLIX_IOS_LIBS");
    if os == "ios" {
        link_ios();
    } else if os == "linux" {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtimes/linux-x64/native");
        let dir = dir.canonicalize().expect("run Native/natives.sh fetch first");
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-arg=-l:libmpv.so.2");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
    }
}

fn link_ios() {
    let root = PathBuf::from(std::env::var("UFLIX_IOS_LIBS").expect("UFLIX_IOS_LIBS: folder of MPVKit xcframeworks"));
    let mut entries: Vec<_> = std::fs::read_dir(&root).expect("UFLIX_IOS_LIBS").flatten().map(|e| e.path()).collect();
    entries.sort();
    for xcf in entries.iter().filter(|p| p.extension().is_some_and(|e| e == "xcframework")) {
        // the device slice only (not "-simulator" / "-maccatalyst")
        let Some(slice) = std::fs::read_dir(xcf).unwrap().flatten().map(|e| e.path()).find(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("ios-arm64") && !n.contains("simulator") && !n.contains("maccatalyst")
        }) else {
            println!("cargo:warning=no ios-arm64 slice in {}", xcf.display());
            continue;
        };
        for item in std::fs::read_dir(&slice).unwrap().flatten().map(|e| e.path()) {
            let name = item.file_name().unwrap().to_string_lossy().to_string();
            if let Some(fw) = name.strip_suffix(".framework") {
                println!("cargo:rustc-link-search=framework={}", slice.display());
                println!("cargo:rustc-link-lib=framework={fw}");
            } else if let Some(lib) = name.strip_prefix("lib").and_then(|n| n.strip_suffix(".a")) {
                println!("cargo:rustc-link-search=native={}", slice.display());
                println!("cargo:rustc-link-lib=static={lib}");
            }
        }
    }
    // What MPVKit's Package.swift links, plus what MoltenVK, libass (CoreText) and the TLS stack need.
    for fw in [
        "AVFoundation",
        "AudioToolbox",
        "CoreAudio",
        "CoreFoundation",
        "CoreGraphics",
        "CoreMedia",
        "CoreText",
        "CoreVideo",
        "Foundation",
        "IOSurface",
        "Metal",
        "QuartzCore",
        "Security",
        "UIKit",
        "VideoToolbox",
    ] {
        println!("cargo:rustc-link-lib=framework={fw}");
    }
    for lib in ["bz2", "iconv", "expat", "resolv", "xml2", "z", "c++"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}
