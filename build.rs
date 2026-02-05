use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use toybox::clap::bundle::{windows_bundle_name, windows_bundle_paths};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".into());
    let package_name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "tension_field".into());
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "linux".into());

    if target_os == "windows" {
        let bundle_paths = windows_bundle_paths(&package_name, &version);
        let dist_dir = env::var("TENSION_FIELD_DIST_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from("C:\\dist")));
        let mut output_path = if profile == "release" {
            if let Some(dir) = dist_dir {
                dir.join(&bundle_paths.bundle_name)
            } else {
                bundle_paths.dist_path.clone()
            }
        } else {
            bundle_paths.target_path.clone()
        };

        if let Some(parent) = output_path.parent() {
            if fs::create_dir_all(parent).is_err() {
                output_path = bundle_paths.target_path.clone();
                if let Some(fallback_parent) = output_path.parent() {
                    let _ = fs::create_dir_all(fallback_parent);
                }
            }
        }

        println!("cargo:rustc-cdylib-link-arg=/OUT:{}", output_path.display());
        println!("cargo:warning=writing bundle to {}", output_path.display());
    } else {
        let bundle_name = windows_bundle_name(&package_name, &version);
        let target_dir = cargo_target_dir();
        let bundle_path = target_dir.join(&profile).join(&bundle_name);
        let artifact_src = target_dir
            .join(&profile)
            .join("deps")
            .join(artifact_name(&target_os));
        copy_artifact(&artifact_src, &bundle_path);
    }
}

fn cargo_target_dir() -> PathBuf {
    if let Ok(dir) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(dir)
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("target")
    }
}

fn lib_basename() -> String {
    env::var("CARGO_PKG_NAME")
        .unwrap_or_else(|_| "tension_field".into())
        .replace('-', "_")
}

fn artifact_name(target_os: &str) -> String {
    match target_os {
        "windows" => format!("{}.dll", lib_basename()),
        "macos" => format!("lib{}.dylib", lib_basename()),
        _ => format!("lib{}.so", lib_basename()),
    }
}

fn copy_artifact(src: &Path, dst: &Path) {
    if let Err(err) = fs::copy(src, dst) {
        eprintln!(
            "warning: failed to copy {} -> {} ({})",
            src.display(),
            dst.display(),
            err
        );
    }
}
