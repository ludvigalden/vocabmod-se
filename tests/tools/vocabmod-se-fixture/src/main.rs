//! A thin entry point over the harness library.

use std::process::ExitCode;

fn main() -> ExitCode {
    vocabmod_se_fixture::main_entry(std::env::args_os().collect())
}
