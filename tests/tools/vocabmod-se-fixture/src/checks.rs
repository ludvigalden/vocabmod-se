//! The checks the harness itself performs over the scratch tree:
//! determinism comparison, emitted file shape, the Athena-complementary
//! OMOP invariants, manifest and row-count reconciliation, and
//! column-uniformity. The tool under test never interprets an
//! expectation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::checkspec::{Check, CmpOp, RowCmp};
use crate::csv::{read_csv, read_delimited};
use crate::tree::tree_census;

/// The lower bound of every concept id we own.
const OUR_ID_FLOOR: i64 = 2_000_000_000;

/// Judge one check in `cwd`; `repo` is the repository root for the tree
/// census. Returns every failure as a human-readable line.
pub(crate) fn judge(check: &Check, cwd: &Path, repo: &Path) -> Vec<String> {
    match check {
        Check::Determinism { trees } => determinism(trees, cwd),
        Check::FilesExist { paths } => files_exist(paths, cwd),
        Check::FilesAbsent { paths } => files_absent(paths, cwd),
        Check::OmopInvariants {
            dir,
            reconcile_tree,
        } => omop_invariants(dir, *reconcile_tree, cwd, repo),
        Check::RowCount {
            file,
            equals,
            at_least,
        } => row_count(file, *equals, *at_least, cwd),
        Check::RowCounts { expect } => row_counts(expect, cwd),
        Check::ManifestRows { dir } => manifest_rows(dir, cwd),
        Check::ColumnUniform {
            file,
            column,
            value,
        } => column_uniform(file, column, value, cwd),
    }
}

/// Require the named trees to hold exactly the same files, each with
/// exactly the same bytes.
fn determinism(trees: &[String], cwd: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    let baseline = match tree_files(&cwd.join(&trees[0])) {
        Ok(files) => files,
        Err(e) => return vec![format!("determinism: {e}")],
    };
    for other in &trees[1..] {
        let candidate = match tree_files(&cwd.join(other)) {
            Ok(files) => files,
            Err(e) => {
                failures.push(format!("determinism: {e}"));
                continue;
            }
        };
        if candidate != baseline {
            let only_baseline: Vec<_> = baseline
                .iter()
                .map(|(p, _)| p.clone())
                .filter(|p| !candidate.iter().any(|(q, _)| q == p))
                .collect();
            let only_candidate: Vec<_> = candidate
                .iter()
                .map(|(p, _)| p.clone())
                .filter(|p| !baseline.iter().any(|(q, _)| q == p))
                .collect();
            if !only_baseline.is_empty() {
                failures.push(format!(
                    "determinism: files only in {}: {}",
                    trees[0],
                    only_baseline.join(", ")
                ));
            }
            if !only_candidate.is_empty() {
                failures.push(format!(
                    "determinism: files only in {other}: {}",
                    only_candidate.join(", ")
                ));
            }
        }
        for (path, baseline_bytes) in &baseline {
            let Some(candidate_bytes) = candidate
                .iter()
                .find(|(q, _)| q == path)
                .map(|(_, b)| b.clone())
            else {
                continue;
            };
            if candidate_bytes != *baseline_bytes {
                let at = baseline_bytes
                    .iter()
                    .zip(candidate_bytes.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| baseline_bytes.len().min(candidate_bytes.len()));
                failures.push(format!(
                    "determinism: {other}/{path} differs from {}/{path} at byte {} ({} vs {} bytes)",
                    trees[0],
                    at,
                    candidate_bytes.len(),
                    baseline_bytes.len()
                ));
            }
        }
    }
    failures
}

/// Every regular file below `root` with its bytes, as sorted pairs.
fn tree_files(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    let mut out = Vec::new();
    collect_files(root, "", &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let read =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for item in read {
        let item = item.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let file_type = item
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", item.path().display()))?;
        let name = item.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if file_type.is_dir() {
            collect_files(&item.path(), &rel, out)?;
        } else if file_type.is_file() {
            let bytes = std::fs::read(item.path())
                .map_err(|e| format!("cannot read {}: {e}", item.path().display()))?;
            out.push((rel, bytes));
        }
    }
    Ok(())
}

fn files_exist(paths: &[String], cwd: &Path) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| {
            let path = cwd.join(p);
            if path.is_file() {
                None
            } else {
                Some(format!("files-exist: {p} is not a regular file"))
            }
        })
        .collect()
}

fn files_absent(paths: &[String], cwd: &Path) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| {
            if cwd.join(p).exists() {
                Some(format!("files-absent: {p} exists"))
            } else {
                None
            }
        })
        .collect()
}

/// The Athena-complementary invariants over an OMOP export directory:
/// CONCEPT ids all ours, unique, ascending; standard_concept empty;
/// every id column's our-side values present in CONCEPT; ids below our
/// floor only ever in target-side columns; and, when declared, the
/// CONCEPT row count reconciled against the committed tree's own
/// concept and `OMOP_EXT`-binding line counts.
fn omop_invariants(dir: &str, reconcile: bool, cwd: &Path, repo: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    let out = cwd.join(dir);
    let concept_path = out.join("CONCEPT.csv");
    let concept = match read_csv(&concept_path) {
        Ok(table) => table,
        Err(e) => return vec![format!("omop-invariants: {e}")],
    };
    const CONCEPT_HEADER: [&str; 10] = [
        "concept_id",
        "concept_name",
        "domain_id",
        "vocabulary_id",
        "concept_class_id",
        "concept_code",
        "standard_concept",
        "valid_start_date",
        "valid_end_date",
        "invalid_reason",
    ];
    if concept.header != CONCEPT_HEADER {
        failures.push(format!(
            "omop-invariants: CONCEPT.csv header is {:?}, expected {CONCEPT_HEADER:?}",
            concept.header
        ));
        return failures;
    }
    let mut ids: Vec<i64> = Vec::with_capacity(concept.rows.len());
    for (index, row) in concept.rows.iter().enumerate() {
        let raw = row.first().map(String::as_str).unwrap_or_default();
        let Some(id) = raw.parse::<i64>().ok() else {
            failures.push(format!(
                "omop-invariants: CONCEPT.csv row {} id {raw:?} is not an integer",
                index + 2
            ));
            continue;
        };
        if id < OUR_ID_FLOOR {
            failures.push(format!(
                "omop-invariants: CONCEPT.csv row {} id {id} is below {OUR_ID_FLOOR}",
                index + 2
            ));
        }
        if row.get(6).is_some_and(|s| !s.is_empty()) {
            failures.push(format!(
                "omop-invariants: CONCEPT.csv row {} carries standard_concept {:?}",
                index + 2,
                &row[6]
            ));
        }
        ids.push(id);
    }
    let sorted = ids.is_sorted();
    let unique = ids.iter().collect::<BTreeSet<_>>().len() == ids.len();
    if !sorted {
        failures.push("omop-invariants: CONCEPT.csv concept_id is not ascending".to_owned());
    }
    if !unique {
        failures.push("omop-invariants: CONCEPT.csv concept_id has duplicates".to_owned());
    }
    let id_set: BTreeSet<i64> = ids.iter().copied().collect();

    let entries = match std::fs::read_dir(&out) {
        Ok(read) => read.flatten().collect::<Vec<_>>(),
        Err(e) => {
            failures.push(format!("omop-invariants: cannot read {}: {e}", out.display()));
            return failures;
        }
    };
    let mut tables: Vec<PathBuf> = entries
        .iter()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            matches!(&name, Some(n) if n.ends_with(".csv") && n != "MANIFEST.csv")
        })
        .collect();
    tables.sort();
    for table_path in &tables {
        let Some(name) = table_path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let table = match read_csv(table_path) {
            Ok(table) => table,
            Err(e) => {
                failures.push(format!("omop-invariants: {e}"));
                continue;
            }
        };
        for (column, header) in table.header.iter().enumerate() {
            if !header.contains("concept_id") || header == "language_concept_id" {
                continue;
            }
            // Which id columns may legitimately carry an id below our
            // floor: CONCEPT_RELATIONSHIP names Athena standard ids the
            // tree itself binds (standard endpoint codes) on either
            // side, and SOURCE_TO_CONCEPT_MAP only in its target column.
            // Every other id column is ours alone.
            let may_name_standard = match name.as_str() {
                "CONCEPT_RELATIONSHIP.csv" => true,
                "SOURCE_TO_CONCEPT_MAP.csv" => header == "target_concept_id",
                _ => false,
            };
            for (index, row) in table.rows.iter().enumerate() {
                let Some(raw) = row.get(column) else { continue };
                if raw.is_empty() {
                    continue;
                }
                let Ok(id) = raw.parse::<i64>() else {
                    failures.push(format!(
                        "omop-invariants: {name} row {} column {header} carries non-integer id {raw:?}",
                        index + 2
                    ));
                    continue;
                };
                if id >= OUR_ID_FLOOR && !id_set.contains(&id) {
                    failures.push(format!(
                        "omop-invariants: {name} row {} column {header} names id {id} no CONCEPT row carries",
                        index + 2
                    ));
                }
                if id < OUR_ID_FLOOR && !may_name_standard {
                    failures.push(format!(
                        "omop-invariants: {name} row {} column {header} carries id {id} below {OUR_ID_FLOOR} where only our ids belong",
                        index + 2
                    ));
                }
            }
        }
    }

    if reconcile {
        let census = match tree_census(repo) {
            Ok(census) => census,
            Err(e) => {
                failures.push(format!("omop-invariants: {e}"));
                return failures;
            }
        };
        let expected = census.concepts - census.ext_bindings;
        if concept.rows.len() as i64 != expected {
            failures.push(format!(
                "omop-invariants: CONCEPT.csv has {} rows; the tree scan counts {} concepts minus {} OMOP_EXT bindings = {expected}",
                concept.rows.len(),
                census.concepts,
                census.ext_bindings
            ));
        }
    }
    failures
}

fn data_rows(path: &Path) -> Result<u64, String> {
    Ok(read_delimited(path)?.rows.len() as u64)
}

fn row_count(file: &str, equals: Option<u64>, at_least: Option<u64>, cwd: &Path) -> Vec<String> {
    let path = cwd.join(file);
    let count = match data_rows(&path) {
        Ok(count) => count,
        Err(e) => return vec![format!("row-count: {e}")],
    };
    let mut failures = Vec::new();
    if let Some(want) = equals && count != want {
        failures.push(format!("row-count: {file} has {count} data rows, expected {want}"));
    }
    if let Some(floor) = at_least && count < floor {
        failures.push(format!(
            "row-count: {file} has {count} data rows, expected at least {floor}"
        ));
    }
    failures
}

fn row_counts(expect: &[RowCmp], cwd: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    for cmp in expect {
        let left = match data_rows(&cwd.join(&cmp.file)) {
            Ok(n) => n,
            Err(e) => {
                failures.push(format!("row-counts: {e}"));
                continue;
            }
        };
        let right = match data_rows(&cwd.join(&cmp.than)) {
            Ok(n) => n,
            Err(e) => {
                failures.push(format!("row-counts: {e}"));
                continue;
            }
        };
        if !cmp.op.judge(left, right) {
            failures.push(format!(
                "row-counts: {} has {left} rows, expected {} {}'s {right}",
                cmp.file,
                op_text(cmp.op),
                cmp.than
            ));
        }
    }
    failures
}

fn op_text(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Lt => "fewer than",
        CmpOp::Le => "at most",
        CmpOp::Eq => "equal to",
        CmpOp::Ge => "at least",
        CmpOp::Gt => "more than",
    }
}

fn manifest_rows(dir: &str, cwd: &Path) -> Vec<String> {
    let out = cwd.join(dir);
    let manifest_path = out.join("MANIFEST.csv");
    let manifest = match read_csv(&manifest_path) {
        Ok(table) => table,
        Err(e) => return vec![format!("manifest-rows: {e}")],
    };
    const MANIFEST_HEADER: [&str; 8] = [
        "table",
        "rows",
        "skipped",
        "filters",
        "schema_digest",
        "registry_version",
        "id_bands",
        "notes",
    ];
    let mut failures = Vec::new();
    if manifest.header != MANIFEST_HEADER {
        failures.push(format!(
            "manifest-rows: MANIFEST.csv header is {:?}, expected {MANIFEST_HEADER:?}",
            manifest.header
        ));
        return failures;
    }
    let mut named: BTreeSet<String> = BTreeSet::new();
    for (index, row) in manifest.rows.iter().enumerate() {
        let table = row.first().map(String::as_str).unwrap_or_default();
        let declared = row.get(1).map(String::as_str).unwrap_or_default();
        let Ok(want) = declared.parse::<u64>() else {
            failures.push(format!(
                "manifest-rows: row {} table {table:?} rows cell {declared:?} is not a count",
                index + 2
            ));
            continue;
        };
        named.insert(table.to_owned());
        let path = out.join(table);
        match data_rows(&path) {
            Ok(count) if count == want => {}
            Ok(count) => failures.push(format!(
                "manifest-rows: {table} manifest says {want} rows, the file has {count}"
            )),
            Err(e) => failures.push(format!("manifest-rows: {e}")),
        }
    }
    let emitted: BTreeSet<String> = std::fs::read_dir(&out)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.ends_with(".csv") && name != "MANIFEST.csv"
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if emitted != named {
        failures.push(format!(
            "manifest-rows: emitted tables {:?} do not match manifest tables {:?}",
            emitted, named
        ));
    }
    failures
}

fn column_uniform(file: &str, column: &str, value: &str, cwd: &Path) -> Vec<String> {
    let path = cwd.join(file);
    let table = match read_delimited(&path) {
        Ok(table) => table,
        Err(e) => return vec![format!("column-uniform: {e}")],
    };
    let Some(index) = table.column(column) else {
        return vec![format!(
            "column-uniform: {file} has no column {column:?} (header {:?})",
            table.header
        )];
    };
    table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let cell = row.get(index).map(String::as_str).unwrap_or_default();
            (cell != value).then(|| {
                format!(
                    "column-uniform: {file} row {} column {column} is {cell:?}, expected {value:?}",
                    i + 2
                )
            })
        })
        .collect()
}
