//! The harness invocation: `--fx-*` options parsed out of argv; any
//! other argument is a usage error, so no argument is ever silently
//! ignored.

use std::path::PathBuf;

/// The default scratch root: a directory under the system temporary
/// directory, outside the repository tree, so the tool's in-repository
/// write guards treat scenario outputs as the operator's own surface.
pub(crate) fn default_scratch_root() -> PathBuf {
    std::env::temp_dir().join("vocabmod-se-fixture")
}

/// The parsed `--fx-*` options.
#[derive(Debug, Default)]
pub(crate) struct FxOptions {
    pub(crate) suite: Option<PathBuf>,
    pub(crate) tool: Option<PathBuf>,
    pub(crate) scratch_root: Option<PathBuf>,
    pub(crate) filter: Option<String>,
    pub(crate) heavy: bool,
    pub(crate) list: bool,
    pub(crate) keep_failed: bool,
}

/// Parse argv (executable name first) into the harness options. An
/// unknown `--fx-*` option, a value-taking option without its value, a
/// repeated option, or any non-`--fx-` argument besides the program
/// name is a usage error.
pub(crate) fn parse(argv: &[String]) -> Result<FxOptions, String> {
    let mut opts = FxOptions::default();
    let mut iter = argv.iter();
    if let Some(argv0) = iter.next() {
        if argv0.starts_with("--") {
            return Err(format!("the program name cannot be an option: {argv0}"));
        }
    }
    while let Some(arg) = iter.next() {
        if !arg.starts_with("--fx-") {
            return Err(format!(
                "unknown argument {arg:?}; the harness takes only --fx-* options"
            ));
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        match name {
            "--fx-list" | "--fx-keep-failed" | "--fx-heavy" => {
                if inline.is_some() {
                    return Err(format!("{name} takes no value"));
                }
                if name == "--fx-list" {
                    opts.list = true;
                } else if name == "--fx-keep-failed" {
                    opts.keep_failed = true;
                } else {
                    opts.heavy = true;
                }
            }
            "--fx-suite" | "--fx-tool" | "--fx-scratch-root" | "--fx-filter" => {
                let value = match inline {
                    Some(v) => v,
                    None => iter
                        .next()
                        .cloned()
                        .ok_or_else(|| format!("{name} needs a value"))?,
                };
                if value.is_empty() {
                    return Err(format!("{name} needs a non-empty value"));
                }
                let slot_taken = match name {
                    "--fx-suite" => opts.suite.replace(PathBuf::from(value)).is_some(),
                    "--fx-tool" => opts.tool.replace(PathBuf::from(value)).is_some(),
                    "--fx-scratch-root" => {
                        opts.scratch_root.replace(PathBuf::from(value)).is_some()
                    }
                    _ => opts.filter.replace(value).is_some(),
                };
                if slot_taken {
                    return Err(format!("{name} given more than once"));
                }
            }
            other => return Err(format!("unknown harness option {other}")),
        }
    }
    Ok(opts)
}
