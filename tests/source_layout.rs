//! Guards the repository's file-size rule: no Rust file over 400 lines.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 400;

#[test]
fn every_rust_file_stays_under_the_line_ceiling() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut oversized = Vec::new();
    for directory in ["src", "tests"] {
        for path in rust_files(&root.join(directory)) {
            let lines = fs::read_to_string(&path)
                .expect("source file is readable")
                .lines()
                .count();
            if lines > MAX_LINES {
                oversized.push(format!("{} ({lines} lines)", path.display()));
            }
        }
    }
    assert!(
        oversized.is_empty(),
        "files over {MAX_LINES} lines: {oversized:?}"
    );
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(directory).expect("directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found
}
