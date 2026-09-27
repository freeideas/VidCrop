//! The screencapturekit crate links Swift code but only tells the linker where Swift's support
//! libraries live inside a full Xcode. Point it at whichever Swift toolchain is installed
//! (the Command Line Tools are enough).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(out) = std::process::Command::new("xcrun").args(["--find", "swiftc"]).output() else { return };
    let swiftc = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    if let Some(usr) = swiftc.parent().and_then(|bin| bin.parent()) {
        let lib = usr.join("lib/swift/macosx");
        if lib.is_dir() {
            println!("cargo:rustc-link-search=native={}", lib.display());
        }
    }
}
