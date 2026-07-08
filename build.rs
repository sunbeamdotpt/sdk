use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir =
        PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|e| panic!("OUT_DIR not set: {e}")));
    let target = env::var("TARGET").unwrap_or_default();
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|e| panic!("CARGO_MANIFEST_DIR not set: {e}")),
    );

    // Embed lima-sunbeam.yaml for VM provisioning
    let lima_yaml_src = manifest_dir.join("lima-sunbeam.yaml");
    let lima_yaml_dst = out_dir.join("lima-sunbeam.yaml");
    fs::copy(&lima_yaml_src, &lima_yaml_dst)
        .unwrap_or_else(|_| panic!("lima-sunbeam.yaml not found at {}", lima_yaml_src.display()));
    println!("cargo:rerun-if-changed={}", lima_yaml_src.display());

    // Generate sso-gateway ConnectRPC client stubs for the auth module.
    generate_connectrpc_module(
        &out_dir,
        "sso-gateway",
        "buf.build/sunbeamdotpt/sso-gateway",
        &["iam/v1"],
        None,
    );

    // Generate Kanban ConnectRPC client stubs for the kanban module.
    // Kanban protos import the local Ory Keto read_service.proto, so the
    // vendored proto directory is passed as an additional include root.
    let local_proto_dir = manifest_dir.join("proto");
    generate_connectrpc_module(
        &out_dir,
        "kanban",
        "buf.build/sunbeamdotpt/kanban",
        &["sunbeam/kanban/v1"],
        Some(&local_proto_dir),
    );

    // Set version info from git
    let commit = git_commit_sha();
    println!("cargo:rustc-env=SUNBEAM_COMMIT={commit}");

    // Build target triple and build date
    println!("cargo:rustc-env=SUNBEAM_TARGET={target}");
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    println!("cargo:rustc-env=SUNBEAM_BUILD_DATE={date}");

    // Rebuild if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
}

/// Export a Buf Schema Registry module and generate ConnectRPC client stubs.
///
/// `subdir` is the name of the directory created under `OUT_DIR` for this
/// module's exported protos and generated Rust code. `bsr_module` is the
/// BSR reference (e.g. `buf.build/sunbeamdotpt/sso-gateway`). `proto_dirs`
/// lists the directories under the exported module that contain `.proto`
/// files to generate. `extra_include` is an optional additional include root
/// for imports not resolved through the BSR module.
fn generate_connectrpc_module(
    out_dir: &Path,
    subdir: &str,
    bsr_module: &str,
    proto_dirs: &[&str],
    extra_include: Option<&Path>,
) {
    let export_dir = out_dir.join(format!("{subdir}-protos"));
    fs::create_dir_all(&export_dir)
        .unwrap_or_else(|e| panic!("failed to create {}: {e}", export_dir.display()));

    let buf_export = Command::new("buf")
        .args([
            "export",
            bsr_module,
            "--output",
            &export_dir.to_string_lossy(),
        ])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "failed to run `buf export {bsr_module}`; \
                 ensure `buf` is installed and on PATH: {e}"
            )
        });
    if !buf_export.status.success() {
        panic!(
            "`buf export {bsr_module}` failed: {}",
            String::from_utf8_lossy(&buf_export.stderr)
        );
    }

    let mut includes: Vec<PathBuf> = vec![export_dir.clone()];
    if let Some(extra) = extra_include {
        includes.push(extra.to_path_buf());
    }

    let mut files = Vec::new();
    for dir in proto_dirs {
        let path = export_dir.join(dir);
        let entries = fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("failed to read proto directory {}: {e}", path.display()));
        for entry in entries {
            let entry = entry.unwrap();
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("proto") {
                files.push(p);
            }
        }
    }

    let gen_dir = out_dir.join(subdir);
    fs::create_dir_all(&gen_dir)
        .unwrap_or_else(|e| panic!("failed to create {}: {e}", gen_dir.display()));

    connectrpc_build::Config::new()
        .files(&files)
        .includes(&includes)
        .out_dir(&gen_dir)
        .include_file("_connectrpc.rs")
        .emit_rerun_directives(false)
        .compile()
        .unwrap_or_else(|e| panic!("failed to compile {subdir} protos: {e}"));
}

fn git_commit_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}
