use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../aegis-core/src");
    println!("cargo:rerun-if-changed=../aegis-controller/src");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/package-lock.json");
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-env-changed=AEGIS_WEB_SKIP_FRONTEND_BUILD");

    if env::var_os("AEGIS_WEB_SKIP_FRONTEND_BUILD").is_some() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    if env::var_os("AEGIS_WEB_EXPORT_TS").is_some() {
        let workspace_dir = manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .expect("aegis-web should live under crates/")
            .to_path_buf();

        run(
            Command::new("cargo")
                .arg("test")
                .arg("--offline")
                .arg("-p")
                .arg("aegis-core")
                .arg("--features")
                .arg("ts-export")
                .arg("export_ts_bindings")
                .arg("--")
                .arg("--nocapture")
                .current_dir(&workspace_dir),
            "aegis-core TypeScript binding export failed",
        );

        run(
            Command::new("cargo")
                .arg("test")
                .arg("--offline")
                .arg("-p")
                .arg("aegis-controller")
                .arg("--features")
                .arg("ts-export")
                .arg("export_ts_bindings")
                .arg("--")
                .arg("--nocapture")
                .current_dir(&workspace_dir),
            "aegis-controller TypeScript binding export failed",
        );
    }

    run(
        Command::new("npm")
            .arg("run")
            .arg("build")
            .current_dir(manifest_dir.join("frontend")),
        "Vite frontend build failed; run `npm install` in crates/aegis-web/frontend",
    );
}

fn run(command: &mut Command, message: &str) {
    let status = command.status().expect(message);
    assert!(status.success(), "{message}");
}
