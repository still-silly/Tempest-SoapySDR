use std::{env, path::PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo did not provide target OS");
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not provide manifest directory"),
    );

    let default_library_dir = manifest_dir
        .join("../..")
        .join("TempestSDR")
        .join("bin")
        .join(if target_os == "windows" {
            "WINDOWS"
        } else {
            "LINUX"
        })
        .join("X64");

    let library_dir = env::var_os("TSDR_LIBRARY_DIR")
        .map(PathBuf::from)
        .unwrap_or(default_library_dir);

    println!("cargo:rerun-if-env-changed=TSDR_LIBRARY_DIR");
    println!("cargo:rerun-if-changed=../../TempestSDR/src/include/TSDRLibrary.h");
    println!("cargo:rustc-link-search=native={}", library_dir.display());
    println!("cargo:library_dir={}", library_dir.display());

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=TSDRLibrary");
    } else {
        println!("cargo:rustc-link-lib=dylib=TSDRLibrary");
        println!("cargo:rustc-link-lib=dylib=m");
        println!("cargo:rustc-link-lib=dylib=dl");
        println!("cargo:rustc-link-lib=dylib=pthread");
    }
}
