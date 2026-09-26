use std::{env, path::PathBuf};

fn main() {
    slint_build::compile("ui/app-window.slint").expect("Slint UI compilation failed");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let native_dir = PathBuf::from(
            env::var_os("DEP_TSDRLIBRARY_LIBRARY_DIR")
                .expect("tempest-tsdr did not provide its library directory"),
        );
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", native_dir.display());
    }
}
