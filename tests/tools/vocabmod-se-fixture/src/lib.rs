//! The vocabmod-se fixture harness: discovers declarative scenarios
//! under a suite root, drives the built vocabmod-se binary as a
//! subprocess in a private scratch directory, and evaluates every
//! expectation itself — the tool's output is evidence, never the
//! interpreter of its own expectations.

mod checks;
mod checkspec;
mod csv;
mod fxcfg;
mod runner;
mod scenario;
mod suite;
mod tree;

use std::ffi::OsString;
use std::process::ExitCode;

/// The process entry point over argv.
pub fn main_entry(argv: Vec<OsString>) -> ExitCode {
    let argv: Vec<String> = match argv.into_iter().map(OsString::into_string).collect() {
        Ok(a) => a,
        Err(bad) => {
            eprintln!("vocabmod-se-fixture: argument is not valid UTF-8: {bad:?}");
            return ExitCode::from(runner::EXIT_USAGE);
        }
    };
    ExitCode::from(runner::run(&argv))
}
