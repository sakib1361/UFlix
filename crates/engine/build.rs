//! Compiles the git-ignored secrets.env at the workspace root into the engine (see secrets.env.example).
//! An environment variable with the same name overrides a value at runtime.
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let src = std::env::var_os("UFLIX_SECRETS_FILE").map(PathBuf::from).unwrap_or_else(|| root.join("secrets.env"));
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-env-changed=UFLIX_SECRETS_FILE");
    let text = std::fs::read_to_string(&src).unwrap_or_else(|_| {
        println!("cargo:warning=secrets.env not found: TMDB and subtitle search will not work (copy secrets.env.example)");
        String::new()
    });
    std::fs::write(PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("secrets.env"), text).unwrap();
}
