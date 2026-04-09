use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=RVL_REALSENSE_LIBDIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=LD_LIBRARY_PATH");

    if env::var_os("CARGO_FEATURE_REALSENSE").is_none() {
        return;
    }

    let mut candidates = Vec::new();
    if let Some(override_dir) = env::var_os("RVL_REALSENSE_LIBDIR") {
        candidates.push(PathBuf::from(override_dir));
    }

    candidates.extend([
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),
        PathBuf::from("/usr/local/lib/x86_64-linux-gnu"),
    ]);

    if let Some(lib_dir) = candidates
        .into_iter()
        .find(|dir| contains_realsense_library(dir))
    {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    }
}

fn contains_realsense_library(dir: &Path) -> bool {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|name| name.starts_with("librealsense2.so"))
}
