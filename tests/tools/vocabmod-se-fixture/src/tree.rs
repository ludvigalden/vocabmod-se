//! The independent line-scan census of the committed vocabulary tree:
//! concept declarations and `OMOP_EXT` bindings counted from the raw
//! text, never through the tool under test.

use std::path::{Path, PathBuf};

/// The census counts the OMOP invariants reconcile against.
pub(crate) struct TreeCensus {
    pub(crate) concepts: i64,
    pub(crate) ext_bindings: i64,
}

/// Count concept declarations (`concept "..."` lines) and `OMOP_EXT`
/// `omop-binding` lines under the repository's `vocab` tree.
pub(crate) fn tree_census(repo: &Path) -> Result<TreeCensus, String> {
    let vocab = repo.join("vocab");
    if !vocab.is_dir() {
        return Err(format!("tree census: {} is not a directory", vocab.display()));
    }
    let mut kdl_files = Vec::new();
    collect_kdl(&vocab, &mut kdl_files)?;
    kdl_files.sort();
    let mut concepts = 0i64;
    let mut ext_bindings = 0i64;
    for path in &kdl_files {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("concept \"") {
                concepts += 1;
            } else if trimmed.starts_with("omop-binding ") && trimmed.contains("omop=\"OMOP_EXT:") {
                ext_bindings += 1;
            }
        }
    }
    Ok(TreeCensus {
        concepts,
        ext_bindings,
    })
}

fn collect_kdl(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let read =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for item in read {
        let item = item.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let file_type = item
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", item.path().display()))?;
        if file_type.is_dir() {
            collect_kdl(&item.path(), out)?;
        } else if file_type.is_file() && item.path().extension().is_some_and(|e| e == "kdl") {
            out.push(item.path());
        }
    }
    Ok(())
}
