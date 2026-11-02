use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use ignore::WalkBuilder;

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = root.join("dist");
    let repo = root
        .parent()
        .and_then(Path::parent)
        .expect("repo root above crates/")
        .to_path_buf();
    let package = repo.join("packages/preview-protocol");
    let protocol = package.join("src/generated");

    // clean protocol dir
    fs::create_dir_all(&protocol).unwrap();
    for entry in fs::read_dir(&protocol).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "ts") {
            fs::remove_file(path).unwrap();
        }
    }
    // generate protocol bindings
    holotyp_preview_protocol::export_bindings(&protocol)
        .expect("generate TypeScript bindings");

    // pnpm install
    let install = run_pnpm(&["install", "--frozen-lockfile"], &repo);
    assert!(install.success(), "viewer dependency install failed");

    // pnpm build
    let result = run_pnpm(&["build"], &repo);
    assert!(result.success(), "viewer build failed");

    // generate "static_files.rs"
    generate_map(&dist);

    // Watch the crate's sources and the shared protocol package's
    for file in tracked_files(&root).into_iter().chain(tracked_files(&package)) {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    // The npm toolchain lives at the repository root.
    for file in [
        "../../package.json",
        "../../pnpm-lock.yaml",
        "../../pnpm-workspace.yaml",
        "../../tsconfig.json",
    ] {
        if root.join(file).is_file() {
            println!("cargo:rerun-if-changed={file}");
        }
    }
}

fn run_pnpm(args: &[&str], repo: &Path) -> ExitStatus {
    let pnpm = which::which("pnpm")
        .expect("find `pnpm` on PATH, needed to embed the preview viewer");
    Command::new(&pnpm)
        .args(args)
        .env("CI", "true")
        .current_dir(repo)
        .status()
        .unwrap_or_else(|error| panic!("run {} {args:?}: {error}", pnpm.display()))
}

/// Every file under `dir` that is not git-ignored, as absolute paths.
fn tracked_files(dir: &Path) -> Vec<PathBuf> {
    WalkBuilder::new(dir)
        .hidden(false)
        .require_git(false)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|ty| ty.is_file()))
        .map(|entry| entry.into_path())
        .collect()
}

fn generate_map(dist: &Path) {
    let mut files: Vec<_> = WalkBuilder::new(dist)
        // `dist/` is git-ignored itself, so no ignore source may apply here.
        .standard_filters(false)
        .hidden(false)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|ty| ty.is_file()))
        .map(|entry| {
            let rel = entry.path().strip_prefix(dist).unwrap();
            (rel.to_string_lossy().into_owned(), entry.into_path())
        })
        .collect();
    files.sort();

    let mut code =
        String::from("pub static DIST_FILES: phf::Map<&str, &[u8]> = phf::phf_map! {\n");
    for (rel, path) in files {
        code.push_str(&format!(
            "{rel:?} => include_bytes!({:?}),\n",
            path.to_string_lossy()
        ));
    }
    code.push_str("};\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("static_files.rs");
    fs::write(out, code).unwrap();
}
