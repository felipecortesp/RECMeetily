#[path = "build/ffmpeg.rs"]
mod ffmpeg;

fn main() {
    // GPU Acceleration Detection and Build Guidance
    detect_and_report_gpu_capabilities();

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=Foundation");

        // Let the enhanced_macos crate handle its own Swift compilation
        // The swift-rs crate build will be handled in the enhanced_macos crate's build.rs
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_rec_diarizer();
    }

    // Download and bundle FFmpeg binary at build-time
    ffmpeg::ensure_ffmpeg_binary();

    tauri_build::build()
}

/// Detects GPU acceleration capabilities and provides build guidance
fn detect_and_report_gpu_capabilities() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    println!("cargo:warning=🚀 Building RECMeetily for: {}", target_os);

    match target_os.as_str() {
        "macos" => {
            println!("cargo:warning=✅ macOS: Metal GPU acceleration ENABLED by default");
            #[cfg(feature = "coreml")]
            println!("cargo:warning=✅ CoreML acceleration ENABLED");
        }
        _ => {
            println!("cargo:warning=ℹ️  Unsupported platform: {} (RECMeetily targets macOS only)", target_os);
        }
    }
}

/// Builds the RecDiarizer Swift package (FluidAudio offline diarizer bridge)
/// as a static library and links it, together with the Swift runtime, into the app.
fn build_rec_diarizer() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let package = manifest_dir.join("swift/RecDiarizer");
    let build_path = out_dir.join("recdiarizer-build");

    println!("cargo:rerun-if-changed=swift/RecDiarizer/Package.swift");
    println!("cargo:rerun-if-changed=swift/RecDiarizer/Sources");
    println!("cargo:rerun-if-changed=../../vendor/FluidAudio/Sources");
    println!("cargo:rerun-if-changed=../../vendor/FluidAudio/Package.swift");

    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        _ => "x86_64",
    };
    let status = Command::new("swift")
        .args(["build", "-c", "release", "--arch", arch, "--package-path"])
        .arg(&package)
        .arg("--build-path")
        .arg(&build_path)
        .status()
        .unwrap_or_else(|e| {
            panic!("`swift` not found or failed to start ({e}); install Xcode or the Xcode Command Line Tools")
        });
    if !status.success() {
        panic!("swift build of swift/RecDiarizer failed");
    }

    println!(
        "cargo:rustc-link-search=native={}",
        build_path.join("release").display()
    );
    println!("cargo:rustc-link-lib=static=RecDiarizer");
    for framework in [
        "Foundation",
        "AVFoundation",
        "CoreML",
        "Accelerate",
        "Metal",
        "MetalPerformanceShaders",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=c++");

    // The static library autolinks the Swift runtime; point the linker at the
    // toolchain/SDK copies but load the runtime from the OS at run time.
    let xcrun = |args: &[&str]| -> Option<String> {
        let out = Command::new("xcrun").args(args).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    if let Some(swiftc) = xcrun(&["--toolchain", "default", "--find", "swiftc"]) {
        if let Some(usr) = std::path::Path::new(&swiftc).parent().and_then(|p| p.parent()) {
            println!(
                "cargo:rustc-link-search=native={}",
                usr.join("lib/swift/macosx").display()
            );
        }
    }
    if let Some(sdk) = xcrun(&["--show-sdk-path"]) {
        println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
