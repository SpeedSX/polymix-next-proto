//! §10 test 8 — the engine uses integer money only; fail on any float type.

use std::fs;
use std::path::Path;

#[test]
fn engine_src_contains_no_float_types() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    scan(&src, &mut offenders);
    assert!(offenders.is_empty(), "float types found in: {offenders:?}");
}

fn scan(dir: &Path, offenders: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan(&path, offenders);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = fs::read_to_string(&path).unwrap();
            if text.contains("f32") || text.contains("f64") {
                offenders.push(path.display().to_string());
            }
        }
    }
}
