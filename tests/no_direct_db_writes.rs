// tests/no_direct_db_writes.rs
// Fails if direct SQLite write calls are present in runtime code.
// Allowed: tests and explicitly whitelisted helper/test files.

use std::fs;
use std::path::{Path, PathBuf};

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_rs_files(&p, files);
            } else if p.extension().map(|s| s == "rs").unwrap_or(false) {
                files.push(p);
            }
        }
    }
}

fn is_whitelisted(path: &Path) -> bool {
    let p = path.to_string_lossy();
    // Whitelist test-only helper files or known unit-test heavy files
    p.contains("/writer/test_helpers.rs") ||
    p.contains("\\writer\\test_helpers.rs") ||
    p.contains("/writer/helpers_tests.rs") ||
    p.contains("\\writer\\helpers_tests.rs") ||
    p.contains("/writer/mod.rs") || p.contains("\\writer\\mod.rs") ||
    // Allow PRAGMA and WAL maintenance modules to use execute_batch
    p.contains("/database/connection.rs") || p.contains("\\database\\connection.rs") ||
    p.contains("/database/checkpoint.rs") || p.contains("\\database\\checkpoint.rs") ||
    p.contains("/database/systems/migration_handler.rs") || p.contains("\\database\\systems\\migration_handler.rs") ||
    p.contains("/database/systems/upload_handler.rs") || p.contains("\\database\\systems\\upload_handler.rs") ||
    p.contains("/validation.rs") || // contains inline tests with direct writes
    p.contains("\\validation.rs") ||
    p.contains("/build.rs") || p.contains("\\build.rs") ||
    // CLI tools are standalone utilities that run outside the main app
    p.contains("_cli.rs") ||
    p.contains("/cli/") || p.contains("\\cli\\")
}

#[test]
fn no_direct_db_writes_in_runtime() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src_dir = Path::new(manifest_dir).join("src");

    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    // Patterns indicating direct DB writes via rusqlite
    let bad_patterns = [
        "conn.execute(",
        ".execute_batch(",
        "stmt.execute(",
        "Transaction::execute(",
    ];

    let mut offenders: Vec<(String, String)> = Vec::new();

    for file in files {
        if is_whitelisted(&file) { continue; }
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Quick skip if file is test-only
        if content.contains("#![cfg(test)]") { continue; }

        for pat in &bad_patterns {
            if content.contains(pat) {
                offenders.push((file.to_string_lossy().to_string(), pat.to_string()));
            }
        }
    }

    if !offenders.is_empty() {
        let mut msg = String::from("Direct DB write calls found in runtime code:\n");
        for (file, pat) in offenders {
            msg.push_str(&format!("  {} contains pattern '{}': route through daemon_client instead\n", file, pat));
        }
        panic!("{}", msg);
    }
}
