fn main() {
    // the app version: $UFLIX_VERSION (CI test builds), else the VERSION file at the repo root
    println!("cargo:rerun-if-env-changed=UFLIX_VERSION");
    println!("cargo:rerun-if-changed=../VERSION");
    let version = std::env::var("UFLIX_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::fs::read_to_string("../VERSION").ok())
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=UFLIX_VERSION={version}");
    println!("cargo:rerun-if-env-changed=UFLIX_UPDATE_REPO");

    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/main.slint", config).expect("Slint UI failed to compile");
}
