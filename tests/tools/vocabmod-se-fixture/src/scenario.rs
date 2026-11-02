//! Scenario format: the strict `scenario.json` manifest. Every key is
//! checked against the declared shape, so a typo fails the load loudly
//! instead of silently selecting nothing. The `checks` array's kinds
//! live in `checkspec.rs`.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

use crate::checkspec::{parse_check, Check};

/// The manifest file name inside a scenario directory.
pub(crate) const MANIFEST: &str = "scenario.json";

/// The repository placeholder: expands to the repository root above the
/// suite, so a scenario can name the committed corpus the tool reads.
pub(crate) const REPO_PLACEHOLDER: &str = "{fx-repo}";

/// A loaded scenario.
#[derive(Debug)]
pub(crate) struct Scenario {
    pub(crate) steps: Vec<Step>,
    pub(crate) checks: Vec<Check>,
    pub(crate) heavy: bool,
}

impl Scenario {
    /// Whether the manifest declares the scenario heavy: excluded from
    /// a default run and run only through `--fx-heavy` or an explicit
    /// `--fx-filter`.
    pub(crate) fn heavy(&self) -> bool {
        self.heavy
    }
}

/// One ordered step a scenario executes before its checks are judged.
#[derive(Debug)]
pub(crate) enum Step {
    /// Run the tool and judge its exit and streams.
    Run(RunStep),
    /// Append one marker line to a produced file, so a later run can be
    /// proven to detect the drift.
    Corrupt { path: String },
}

/// A tool invocation and its expectation.
#[derive(Debug)]
pub(crate) struct RunStep {
    pub(crate) id: String,
    pub(crate) args: Vec<String>,
    pub(crate) exit: i32,
    pub(crate) stdout_contains: Vec<String>,
    pub(crate) stdout_not_contains: Vec<String>,
    pub(crate) stderr_contains: Vec<String>,
    pub(crate) stderr_not_contains: Vec<String>,
    /// A committed golden the whole of stdout must equal, read at load
    /// time so a missing golden fails the load.
    pub(crate) stdout_golden: Option<(String, Vec<u8>)>,
}

/// Load and validate a scenario from its source directory.
pub(crate) fn load(dir: &Path) -> Result<Scenario, String> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{MANIFEST} is not valid JSON: {e}"))?;
    let root = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST} must be a JSON object"))?;
    check_keys(
        root,
        &["binary", "description", "heavy", "steps", "checks"],
        MANIFEST,
    )?;
    match root.get("binary") {
        Some(Value::String(s)) if s == "vocabmod-se" => {}
        Some(other) => {
            return Err(format!(
                "{MANIFEST}: \"binary\" must be \"vocabmod-se\" (the only supported value), got {other}"
            ));
        }
        None => return Err(format!("{MANIFEST}: missing required key \"binary\"")),
    }
    let heavy = match root.get("heavy") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(format!(
                "{MANIFEST}: \"heavy\" must be a boolean, got {other}"
            ));
        }
    };
    let steps_value = root
        .get("steps")
        .ok_or_else(|| format!("{MANIFEST}: missing required key \"steps\""))?
        .as_array()
        .ok_or_else(|| format!("{MANIFEST}: \"steps\" must be an array"))?;
    if steps_value.is_empty() {
        return Err(format!(
            "{MANIFEST}: \"steps\" must not be empty; a zero-step scenario never verifies anything"
        ));
    }
    let mut steps = Vec::with_capacity(steps_value.len());
    for (index, item) in steps_value.iter().enumerate() {
        steps.push(step(index, item, dir)?);
    }
    let checks = match root.get("checks") {
        None => Vec::new(),
        Some(v) => {
            let list = v
                .as_array()
                .ok_or_else(|| format!("{MANIFEST}: \"checks\" must be an array"))?;
            let mut out = Vec::with_capacity(list.len());
            for (index, item) in list.iter().enumerate() {
                out.push(parse_check(index, item)?);
            }
            out
        }
    };
    Ok(Scenario {
        steps,
        checks,
        heavy,
    })
}

const STEP_KINDS: &[&str] = &["run", "corrupt"];

/// Parse one entry of the `steps` array.
fn step(index: usize, value: &Value, dir: &Path) -> Result<Step, String> {
    let what = format!("steps[{index}]");
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} must be an object"))?;
    check_keys(obj, STEP_KINDS, &what)?;
    let declared = obj
        .keys()
        .filter(|k| STEP_KINDS.contains(&k.as_str()))
        .count();
    if declared != 1 {
        return Err(format!(
            "{MANIFEST}: {what} must carry exactly one of {}",
            STEP_KINDS.join(", ")
        ));
    }
    if let Some(v) = obj.get("run") {
        return Ok(Step::Run(run_step(&what, v, dir)?));
    }
    let v = obj.get("corrupt").ok_or_else(|| {
        format!("{MANIFEST}: {what} carries no step kind (internal parse error)")
    })?;
    let obj = v
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} corrupt must be an object"))?;
    check_keys(obj, &["path"], &format!("{what} corrupt"))?;
    let path = required_string(obj, "path", &format!("{what} corrupt"))?;
    contained(&path, &format!("{what} corrupt path"))?;
    Ok(Step::Corrupt { path })
}

/// Parse a `run` step body.
fn run_step(what: &str, value: &Value, dir: &Path) -> Result<RunStep, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{MANIFEST}: {what} run must be an object"))?;
    check_keys(
        obj,
        &[
            "id",
            "args",
            "exit",
            "stdout-contains",
            "stdout-not-contains",
            "stderr-contains",
            "stderr-not-contains",
            "stdout-golden",
        ],
        &format!("{what} run"),
    )?;
    let id = required_string(obj, "id", &format!("{what} run"))?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!(
            "{MANIFEST}: {what} run id must be a non-empty kebab id, got {id:?}"
        ));
    }
    let args = match obj.get("args") {
        None => return Err(format!("{MANIFEST}: {what} run is missing required key \"args\"")),
        Some(v) => string_array(v, &format!("{what} run args"))?,
    };
    if args.is_empty() {
        return Err(format!("{MANIFEST}: {what} run args must not be empty"));
    }
    for arg in &args {
        check_placeholder(arg, &format!("{what} run args entry"))?;
    }
    let exit = match obj.get("exit") {
        None => 0,
        Some(v) => v
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| format!("{MANIFEST}: {what} run exit must be an integer"))?,
    };
    let lists = |key: &str| -> Result<Vec<String>, String> {
        match obj.get(key) {
            None => Ok(Vec::new()),
            Some(v) => string_array(v, &format!("{what} run {key}")),
        }
    };
    let stdout_contains = lists("stdout-contains")?;
    let stdout_not_contains = lists("stdout-not-contains")?;
    let stderr_contains = lists("stderr-contains")?;
    let stderr_not_contains = lists("stderr-not-contains")?;
    let stdout_golden = match obj.get("stdout-golden") {
        None => None,
        Some(Value::String(s)) => {
            let path = contained(s, &format!("{what} run stdout-golden"))?;
            let bytes = std::fs::read(dir.join(&path)).map_err(|e| {
                format!("{MANIFEST}: cannot read {what} stdout-golden {path:?}: {e}")
            })?;
            Some((path, bytes))
        }
        Some(other) => {
            return Err(format!(
                "{MANIFEST}: {what} run stdout-golden must be a string, got {other}"
            ));
        }
    };
    Ok(RunStep {
        id,
        args,
        exit,
        stdout_contains,
        stdout_not_contains,
        stderr_contains,
        stderr_not_contains,
        stdout_golden,
    })
}

/// Reject unknown keys so a misspelled declaration fails the load.
pub(crate) fn check_keys(obj: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("{what}: unknown key {k:?}")),
        None => Ok(()),
    }
}

pub(crate) fn required_string(
    obj: &Map<String, Value>,
    key: &str,
    what: &str,
) -> Result<String, String> {
    match obj.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(format!("{MANIFEST}: {what} {key:?} must be a string, got {other}")),
        None => Err(format!("{MANIFEST}: {what} is missing required key {key:?}")),
    }
}

pub(crate) fn string_array(value: &Value, what: &str) -> Result<Vec<String>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{MANIFEST}: {what} must be an array of strings"))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{MANIFEST}: {what} must contain only strings"))
        })
        .collect()
}

/// A relative path that stays inside the scenario's scratch directory.
pub(crate) fn contained(raw: &str, what: &str) -> Result<String, String> {
    let path = PathBuf::from(raw);
    let inside = !raw.is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if inside {
        Ok(raw.to_owned())
    } else {
        Err(format!(
            "{MANIFEST}: {what} must be a relative path inside the scenario scratch, got {raw:?}"
        ))
    }
}

/// Reject any `{fx-` token other than the known placeholder, so a
/// misspelled placeholder fails the load instead of reaching a command.
fn check_placeholder(raw: &str, what: &str) -> Result<(), String> {
    let rest = raw.replace(REPO_PLACEHOLDER, "");
    if rest.contains("{fx-") {
        return Err(format!(
            "{MANIFEST}: {what} {raw:?} has an unknown placeholder; only {REPO_PLACEHOLDER} is supported"
        ));
    }
    Ok(())
}

/// Expand every placeholder in the argument list against the repo root.
pub(crate) fn expand_args(args: &[String], repo: &Path) -> Vec<String> {
    let root = repo.to_string_lossy();
    args.iter()
        .map(|arg| arg.replace(REPO_PLACEHOLDER, &root))
        .collect()
}
