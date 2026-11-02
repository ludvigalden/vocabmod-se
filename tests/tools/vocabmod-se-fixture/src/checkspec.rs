//! The `checks` array of a scenario manifest: the declarative check
//! kinds and their strict parser. Each check names a judgement the
//! harness itself makes over the scratch tree (see `checks.rs`).

use serde_json::Value;

use crate::scenario::{check_keys, contained, required_string, string_array, MANIFEST};

/// One check the harness itself judges over the scratch tree after the
/// steps ran. No check ever asks the tool to interpret anything.
#[derive(Debug)]
pub(crate) enum Check {
    /// The named trees (relative directories) hold exactly the same
    /// files with exactly the same bytes.
    Determinism { trees: Vec<String> },
    /// Each named path is a regular file.
    FilesExist { paths: Vec<String> },
    /// Each named path does not exist.
    FilesAbsent { paths: Vec<String> },
    /// The Athena-complementary invariants over an OMOP export
    /// directory; with `reconcile-tree`, the CONCEPT row count must
    /// equal the committed tree's concept count minus its `OMOP_EXT`
    /// binding count, counted by the harness's own line scan.
    OmopInvariants {
        dir: String,
        reconcile_tree: bool,
    },
    /// The named table file's data-row count equals (or is at least) a
    /// declared number.
    RowCount {
        file: String,
        equals: Option<u64>,
        at_least: Option<u64>,
    },
    /// Row-count comparisons between two table files.
    RowCounts { expect: Vec<RowCmp> },
    /// Every MANIFEST.csv row's table file exists and its rows cell
    /// equals the file's data-row count, and the manifest names exactly
    /// the emitted table files.
    ManifestRows { dir: String },
    /// Every data row of a delimited file carries the declared value in
    /// the named column.
    ColumnUniform {
        file: String,
        column: String,
        value: String,
    },
}

/// One row-count comparison between two files.
#[derive(Debug)]
pub(crate) struct RowCmp {
    pub(crate) file: String,
    pub(crate) op: CmpOp,
    pub(crate) than: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CmpOp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl CmpOp {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "<" => Ok(Self::Lt),
            "<=" => Ok(Self::Le),
            "==" => Ok(Self::Eq),
            ">=" => Ok(Self::Ge),
            ">" => Ok(Self::Gt),
            other => Err(format!(
                "comparison op must be one of <, <=, ==, >=, >, got {other:?}"
            )),
        }
    }

    pub(crate) fn judge(&self, left: u64, right: u64) -> bool {
        match self {
            Self::Lt => left < right,
            Self::Le => left <= right,
            Self::Eq => left == right,
            Self::Ge => left >= right,
            Self::Gt => left > right,
        }
    }
}

/// The step kinds a manifest's `checks` array may declare.
const CHECK_KINDS: &[&str] = &[
    "determinism",
    "files-exist",
    "files-absent",
    "omop-invariants",
    "row-count",
    "row-counts",
    "manifest-rows",
    "column-uniform",
];

/// Parse one entry of the `checks` array.
pub(crate) fn parse_check(index: usize, value: &Value) -> Result<Check, String> {
    let what = format!("checks[{index}]");
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} must be an object"))?;
    check_keys(obj, CHECK_KINDS, &what)?;
    let declared = obj
        .keys()
        .filter(|k| CHECK_KINDS.contains(&k.as_str()))
        .count();
    if declared != 1 {
        return Err(format!(
            "{MANIFEST}: {what} must carry exactly one of {}",
            CHECK_KINDS.join(", ")
        ));
    }
    if let Some(v) = obj.get("determinism") {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: {what} determinism must be an object"))?;
        check_keys(obj, &["trees"], &format!("{what} determinism"))?;
        let list = obj
            .get("trees")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{MANIFEST}: {what} determinism trees must be an array"))?;
        let trees = string_array(&Value::Array(list.clone()), &format!("{what} determinism trees"))?;
        if trees.len() < 2 {
            return Err(format!(
                "{MANIFEST}: {what} determinism needs at least two trees to compare"
            ));
        }
        for tree in &trees {
            contained(tree, &format!("{what} determinism tree"))?;
        }
        return Ok(Check::Determinism { trees });
    }
    if let Some(v) = obj.get("files-exist") {
        let paths = path_list(v, &format!("{what} files-exist"))?;
        return Ok(Check::FilesExist { paths });
    }
    if let Some(v) = obj.get("files-absent") {
        let paths = path_list(v, &format!("{what} files-absent"))?;
        return Ok(Check::FilesAbsent { paths });
    }
    if let Some(v) = obj.get("omop-invariants") {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: {what} omop-invariants must be an object"))?;
        check_keys(obj, &["dir", "reconcile-tree"], &format!("{what} omop-invariants"))?;
        let dir = required_string(obj, "dir", &format!("{what} omop-invariants"))?;
        contained(&dir, &format!("{what} omop-invariants dir"))?;
        let reconcile_tree = match obj.get("reconcile-tree") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(other) => {
                return Err(format!(
                    "{MANIFEST}: {what} omop-invariants reconcile-tree must be a boolean, got {other}"
                ));
            }
        };
        return Ok(Check::OmopInvariants {
            dir,
            reconcile_tree,
        });
    }
    if let Some(v) = obj.get("row-count") {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: {what} row-count must be an object"))?;
        check_keys(obj, &["file", "equals", "at-least"], &format!("{what} row-count"))?;
        let file = required_string(obj, "file", &format!("{what} row-count"))?;
        contained(&file, &format!("{what} row-count file"))?;
        let number = |key: &str| -> Result<Option<u64>, String> {
            match obj.get(key) {
                None => Ok(None),
                Some(v) => v.as_u64().map(Some).ok_or_else(|| {
                    format!("{MANIFEST}: {what} row-count {key} must be a non-negative integer")
                }),
            }
        };
        let equals = number("equals")?;
        let at_least = number("at-least")?;
        if equals.is_none() && at_least.is_none() {
            return Err(format!(
                "{MANIFEST}: {what} row-count needs \"equals\" or \"at-least\""
            ));
        }
        return Ok(Check::RowCount {
            file,
            equals,
            at_least,
        });
    }
    if let Some(v) = obj.get("row-counts") {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: {what} row-counts must be an object"))?;
        check_keys(obj, &["expect"], &format!("{what} row-counts"))?;
        let list = obj
            .get("expect")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{MANIFEST}: {what} row-counts expect must be an array"))?;
        if list.is_empty() {
            return Err(format!(
                "{MANIFEST}: {what} row-counts expect must not be empty"
            ));
        }
        let mut expect = Vec::with_capacity(list.len());
        for (i, item) in list.iter().enumerate() {
            let obj = item
                .as_object()
                .ok_or_else(|| format!("{MANIFEST}: {what} row-counts expect[{i}] must be an object"))?;
            check_keys(obj, &["file", "op", "than"], &format!("{what} row-counts expect[{i}]"))?;
            let file = required_string(obj, "file", &format!("{what} row-counts expect[{i}]"))?;
            contained(&file, &format!("{what} row-counts expect[{i}] file"))?;
            let op = required_string(obj, "op", &format!("{what} row-counts expect[{i}]"))?;
            let op = CmpOp::parse(&op)
                .map_err(|e| format!("{MANIFEST}: {what} row-counts expect[{i}] {e}"))?;
            let than = required_string(obj, "than", &format!("{what} row-counts expect[{i}]"))?;
            contained(&than, &format!("{what} row-counts expect[{i}] than"))?;
            expect.push(RowCmp { file, op, than });
        }
        return Ok(Check::RowCounts { expect });
    }
    if let Some(v) = obj.get("manifest-rows") {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("{MANIFEST}: {what} manifest-rows must be an object"))?;
        check_keys(obj, &["dir"], &format!("{what} manifest-rows"))?;
        let dir = required_string(obj, "dir", &format!("{what} manifest-rows"))?;
        contained(&dir, &format!("{what} manifest-rows dir"))?;
        return Ok(Check::ManifestRows { dir });
    }
    let v = obj
        .get("column-uniform")
        .ok_or_else(|| format!("{MANIFEST}: {what} carries no check kind (internal parse error)"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} column-uniform must be an object"))?;
    check_keys(obj, &["file", "column", "value"], &format!("{what} column-uniform"))?;
    let file = required_string(obj, "file", &format!("{what} column-uniform"))?;
    contained(&file, &format!("{what} column-uniform file"))?;
    let column = required_string(obj, "column", &format!("{what} column-uniform"))?;
    let value = required_string(obj, "value", &format!("{what} column-uniform"))?;
    Ok(Check::ColumnUniform {
        file,
        column,
        value,
    })
}

/// An object with a non-empty `paths` array of contained relative paths.
fn path_list(value: &Value, what: &str) -> Result<Vec<String>, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} must be an object"))?;
    check_keys(obj, &["paths"], what)?;
    let list = obj
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{MANIFEST}: {what} paths must be an array"))?;
    let paths = list
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or_else(|| format!("{MANIFEST}: {what} paths must contain only strings"))
        })
        .collect::<Result<Vec<&str>, _>>()?;
    if paths.is_empty() {
        return Err(format!("{MANIFEST}: {what} paths must not be empty"));
    }
    for path in &paths {
        contained(path, &format!("{what} paths entry"))?;
    }
    Ok(paths.into_iter().map(str::to_string).collect())
}
