use std::{env, path::PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo did not provide target OS");
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not provide manifest directory"),
    );

    let repository_root = manifest_dir.join("../..");
    let default_library_dir = match target_os.as_str() {
        "android" => repository_root.join("build/android-arm64"),
        "windows" => repository_root.join("TempestSDR/bin/WINDOWS/X64"),
        _ => repository_root.join("TempestSDR/bin/LINUX/X64"),
    };

    let library_dir = env::var_os("TSDR_LIBRARY_DIR")
        .map(PathBuf::from)
        .unwrap_or(default_library_dir);

    println!("cargo:rerun-if-env-changed=TSDR_LIBRARY_DIR");
    println!("cargo:rerun-if-changed=../../TempestSDR/src/include/TSDRLibrary.h");
    println!("cargo:rustc-link-search=native={}", library_dir.display());
    println!("cargo:library_dir={}", library_dir.display());

    println!("cargo:rustc-link-lib=dylib=TSDRLibrary");
    match target_os.as_str() {
        "android" => {
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=dl");
        }
        "windows" => {}
        _ => {
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=pthread");
        }
    }
}
