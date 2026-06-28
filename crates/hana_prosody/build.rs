//! Build support for the Hana voice sidecar example.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        if let Some(developer_dir) = developer_dir() {
            println!(
                "cargo:rustc-link-arg=-Wl,-rpath,{developer_dir}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"
            );
            println!(
                "cargo:rustc-link-arg=-Wl,-rpath,{developer_dir}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx"
            );
            println!("cargo:rustc-link-arg=-Wl,-rpath,{developer_dir}/usr/lib/swift/macosx");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{developer_dir}/usr/lib/swift-5.5/macosx");
        }
    }
}

#[cfg(target_os = "macos")]
fn developer_dir() -> Option<String> {
    let output = Command::new("xcode-select").arg("-p").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|path| !path.is_empty())
}
