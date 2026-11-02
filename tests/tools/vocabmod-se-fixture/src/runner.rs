//! The harness run: option validation, fail-closed suite discovery,
//! heavy-scenario gating, sequential scenario execution in per-scenario
//! scratch directories, and the exit classes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::checks;
use crate::fxcfg;
use crate::scenario::{self, Scenario, Step};
use crate::suite::{self, Entry};

pub(crate) const EXIT_OK: u8 = 0;
pub(crate) const EXIT_FAIL: u8 = 1;
pub(crate) const EXIT_USAGE: u8 = 2;

const USAGE: &str = "usage: vocabmod-se-fixture --fx-suite <dir> --fx-tool <path> \
[--fx-scratch-root <dir>] [--fx-filter <substr>] [--fx-heavy] [--fx-list] \
[--fx-keep-failed]";

/// Run the harness CLI over argv and return the process exit code.
pub(crate) fn run(argv: &[String]) -> u8 {
    let opts = match fxcfg::parse(argv) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("vocabmod-se-fixture: {msg}\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    let (Some(suite_arg), Some(tool_arg)) = (opts.suite.as_deref(), opts.tool.as_deref()) else {
        eprintln!("vocabmod-se-fixture: --fx-suite and --fx-tool are required\n{USAGE}");
        return EXIT_USAGE;
    };
    let Some(suite_root) = resolve_dir(suite_arg, "suite root") else {
        return EXIT_FAIL;
    };
    let Some(tool) = resolve_file(tool_arg, "vocabmod-se binary") else {
        return EXIT_FAIL;
    };
    let Some(repo) = find_repo(&suite_root) else {
        return EXIT_FAIL;
    };

    let found = match suite::discover(&suite_root) {
        Ok(found) => found,
        Err(msg) => {
            eprintln!("vocabmod-se-fixture: FAIL: {msg}");
            return EXIT_FAIL;
        }
    };
    for name in &found.skipped {
        eprintln!(
            "vocabmod-se-fixture: note: {} is not a scenario (no scenario.json); not run",
            suite_root.join(name).display()
        );
    }
    if found.entries.is_empty() {
        eprintln!(
            "vocabmod-se-fixture: FAIL: suite {} contains zero scenarios; an empty suite never passes",
            suite_root.display()
        );
        return EXIT_FAIL;
    }
    let total = found.entries.len();
    let listed: Vec<Entry> = match &opts.filter {
        None => found.entries,
        Some(f) => found
            .entries
            .into_iter()
            .filter(|e| e.name.contains(f.as_str()))
            .collect(),
    };
    if listed.is_empty() {
        eprintln!(
            "vocabmod-se-fixture: FAIL: --fx-filter {:?} selects zero of {total} scenario(s); an empty selection never passes",
            opts.filter.as_deref().unwrap_or_default()
        );
        return EXIT_FAIL;
    }
    if opts.list {
        for entry in &listed {
            let manifest = scenario::load(&entry.dir).ok();
            let heavy = manifest.as_ref().is_some_and(Scenario::heavy);
            println!("{}{}", entry.name, if heavy { " [heavy]" } else { "" });
        }
        println!("{} scenario(s)", listed.len());
        return EXIT_OK;
    }
    let runnable: Vec<Entry> = if opts.heavy || opts.filter.is_some() {
        listed
    } else {
        let (light, heavy): (Vec<_>, Vec<_>) = listed
            .into_iter()
            .partition(|e| !scenario::load(&e.dir).map(|s| s.heavy()).unwrap_or(false));
        if light.is_empty() {
            eprintln!(
                "vocabmod-se-fixture: FAIL: every scenario under {} is heavy; pass --fx-heavy (or an explicit --fx-filter) to run them; an empty selection never passes",
                suite_root.display()
            );
            return EXIT_FAIL;
        }
        eprintln!(
            "vocabmod-se-fixture: note: {} of {total} scenario(s) are heavy; run them deliberately with --fx-heavy or an explicit --fx-filter",
            heavy.len()
        );
        light
    };

    let scratch_root = opts
        .scratch_root
        .clone()
        .unwrap_or_else(fxcfg::default_scratch_root);
    let run_root = match make_run_root(&scratch_root) {
        Ok(root) => root,
        Err(msg) => {
            eprintln!("vocabmod-se-fixture: FAIL: {msg}");
            return EXIT_FAIL;
        }
    };

    let started = SystemTime::now();
    let mut passed = 0usize;
    let mut failed = 0usize;
    for entry in &runnable {
        let scenario_started = SystemTime::now();
        let cwd = run_root.join("scenarios").join(&entry.name);
        if let Err(msg) = std::fs::create_dir_all(&cwd) {
            eprintln!("vocabmod-se-fixture: FAIL: cannot create {}: {msg}", cwd.display());
            failed += 1;
            continue;
        }
        match run_scenario(entry, &cwd, &tool, &repo) {
            Ok(()) => {
                passed += 1;
                println!(
                    "PASS {} ({})",
                    entry.name,
                    elapsed(scenario_started)
                );
                let _ = std::fs::remove_dir_all(&cwd);
            }
            Err(report) => {
                failed += 1;
                println!("FAIL {} ({})", entry.name, elapsed(scenario_started));
                for line in report.lines() {
                    println!("  {line}");
                }
                if opts.keep_failed {
                    eprintln!(
                        "vocabmod-se-fixture: kept failed scenario at {}",
                        cwd.display()
                    );
                } else {
                    let _ = std::fs::remove_dir_all(&cwd);
                }
            }
        }
    }
    let wall = elapsed(started);
    let selected = runnable.len();
    println!(
        "test result: {passed} passed; {failed} failed ({selected} scenario(s), {wall})"
    );
    if passed + failed == 0 {
        eprintln!("vocabmod-se-fixture: FAIL: zero scenarios ran; an empty run never passes");
        let _ = std::fs::remove_dir_all(&run_root);
        return EXIT_FAIL;
    }
    if failed > 0 {
        if opts.keep_failed {
            eprintln!(
                "vocabmod-se-fixture: run scratch kept at {}",
                run_root.display()
            );
        } else {
            let _ = std::fs::remove_dir_all(&run_root);
        }
        EXIT_FAIL
    } else {
        let _ = std::fs::remove_dir_all(&run_root);
        EXIT_OK
    }
}

/// One scenario's execution: its steps in order, then its checks over
/// the scratch tree. `Err` carries the full failure report.
fn run_scenario(entry: &Entry, cwd: &Path, tool: &Path, repo: &Path) -> Result<(), String> {
    let manifest = scenario::load(&entry.dir).map_err(|e| format!("scenario load error: {e}"))?;
    let Scenario { steps, checks, .. } = manifest;
    let mut failures = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        match step {
            Step::Run(run) => {
                let args = scenario::expand_args(&run.args, repo);
                let step_started = SystemTime::now();
                let output = Command::new(tool)
                    .args(&args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .output()
                    .map_err(|e| format!("step {}: cannot run {}: {e}", index + 1, tool.display()))?;
                println!(
                    "  step {} (run {}) exit {} ({})",
                    index + 1,
                    run.id,
                    output.status.code().unwrap_or(-1),
                    elapsed(step_started)
                );
                failures.extend(judge_run(run, &output, index));
            }
            Step::Corrupt { path } => {
                let target = cwd.join(path);
                let failure = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&target)
                    .and_then(|mut file| {
                        use std::io::Write;
                        file.write_all(b"# fixture corruption marker\n")
                    })
                    .map_err(|e| format!("step {}: cannot corrupt {}: {e}", index + 1, path));
                if let Err(msg) = failure {
                    failures.push(msg);
                }
            }
        }
    }
    for (index, check) in checks.iter().enumerate() {
        failures.extend(
            checks::judge(check, cwd, repo)
                .into_iter()
                .map(|f| format!("check {} ({}): {f}", index + 1, check_name(check))),
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn check_name(check: &crate::checkspec::Check) -> &'static str {
    match check {
        crate::checkspec::Check::Determinism { .. } => "determinism",
        crate::checkspec::Check::FilesExist { .. } => "files-exist",
        crate::checkspec::Check::FilesAbsent { .. } => "files-absent",
        crate::checkspec::Check::OmopInvariants { .. } => "omop-invariants",
        crate::checkspec::Check::RowCount { .. } => "row-count",
        crate::checkspec::Check::RowCounts { .. } => "row-counts",
        crate::checkspec::Check::ManifestRows { .. } => "manifest-rows",
        crate::checkspec::Check::ColumnUniform { .. } => "column-uniform",
    }
}

/// Judge one run's exit and streams; every assertion is a plain
/// comparison made here.
fn judge_run(run: &crate::scenario::RunStep, output: &Output, index: usize) -> Vec<String> {
    let mut failures = Vec::new();
    let what = format!("step {} (run {})", index + 1, run.id);
    let exit = output.status.code().unwrap_or(-1);
    if exit != run.exit {
        failures.push(format!(
            "{what}: exit {exit}, expected {}",
            run.exit
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for needle in &run.stdout_contains {
        if !stdout.contains(needle.as_str()) {
            failures.push(format!("{what}: stdout does not contain {needle:?}"));
        }
    }
    for needle in &run.stdout_not_contains {
        if stdout.contains(needle.as_str()) {
            failures.push(format!("{what}: stdout contains forbidden {needle:?}"));
        }
    }
    for needle in &run.stderr_contains {
        if !stderr.contains(needle.as_str()) {
            failures.push(format!("{what}: stderr does not contain {needle:?}"));
        }
    }
    for needle in &run.stderr_not_contains {
        if stderr.contains(needle.as_str()) {
            failures.push(format!("{what}: stderr contains forbidden {needle:?}"));
        }
    }
    if let Some((golden, bytes)) = &run.stdout_golden && output.stdout != *bytes {
        let at = output
            .stdout
            .iter()
            .zip(bytes.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| output.stdout.len().min(bytes.len()));
        failures.push(format!(
            "{what}: stdout differs from golden {golden} at byte {} ({} vs {} bytes)",
            at,
            output.stdout.len(),
            bytes.len()
        ));
    }
    failures
}

fn resolve_dir(path: &Path, what: &str) -> Option<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(p) if p.is_dir() => Some(p),
        Ok(p) => {
            eprintln!("vocabmod-se-fixture: FAIL: {what} {} is not a directory", p.display());
            None
        }
        Err(e) => {
            eprintln!("vocabmod-se-fixture: FAIL: {what} {}: {e}", path.display());
            None
        }
    }
}

fn resolve_file(path: &Path, what: &str) -> Option<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(p) if p.is_file() => Some(p),
        Ok(p) => {
            eprintln!("vocabmod-se-fixture: FAIL: {what} {} is not a regular file", p.display());
            None
        }
        Err(e) => {
            eprintln!("vocabmod-se-fixture: FAIL: {what} {}: {e}", path.display());
            None
        }
    }
}

/// The repository root above the suite root: the nearest ancestor
/// carrying a Cargo.toml and a vocab directory. The corpus every verb
/// reads lives there; a suite misplaced outside the repository fails
/// closed instead of pointing the tool at the wrong tree.
fn find_repo(suite_root: &Path) -> Option<PathBuf> {
    let mut dir = Some(suite_root.to_path_buf());
    while let Some(current) = dir {
        if current.join("Cargo.toml").is_file() && current.join("vocab").is_dir() {
            return Some(current);
        }
        dir = current.parent().map(Path::to_path_buf);
    }
    eprintln!(
        "vocabmod-se-fixture: FAIL: no repository root (Cargo.toml and vocab) above {}",
        suite_root.display()
    );
    None
}

fn make_run_root(scratch_root: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(scratch_root)
        .map_err(|e| format!("cannot create scratch root {}: {e}", scratch_root.display()))?;
    let scratch_root = std::fs::canonicalize(scratch_root)
        .map_err(|e| format!("cannot resolve scratch root {}: {e}", scratch_root.display()))?;
    let root = scratch_root.join(run_id());
    std::fs::create_dir(&root)
        .map_err(|e| format!("cannot create run directory {}: {e}", root.display()))?;
    std::fs::create_dir(root.join("scenarios"))
        .map_err(|e| format!("cannot create scenarios directory under {}: {e}", root.display()))?;
    Ok(root)
}

/// The run id: `<UTC yyyymmddThhmmssZ>-<pid>`.
fn run_id() -> String {
    format!("{}-{}", utc_stamp(SystemTime::now()), std::process::id())
}

fn elapsed(from: SystemTime) -> String {
    let secs = from
        .elapsed()
        .unwrap_or(Duration::ZERO)
        .as_secs_f32();
    format!("{secs:.1}s")
}

/// Format an instant as `yyyymmddThhmmssZ` (civil-from-days conversion).
fn utc_stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}Z")
}
