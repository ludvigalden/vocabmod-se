//! The search verb: one per-entry reading of the tree, matched in
//! memory. There is no persistent search index at this corpus's scale
//! (~100k concepts); the deterministic index export is the persisted
//! artifact, and search is the interactive read over the same corpus
//! projection.

use std::process::ExitCode;

use crate::{CLEAN, Paths, corpus, fail};

/// Which bound values a query is matched against.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Names,
    Synonyms,
    Codes,
    All,
}

impl Field {
    pub fn parse(spelling: Option<&str>) -> Result<Self, String> {
        match spelling {
            None | Some("all") => Ok(Self::All),
            Some("names") => Ok(Self::Names),
            Some("synonyms") => Ok(Self::Synonyms),
            Some("codes") => Ok(Self::Codes),
            Some(other) => Err(format!(
                "unknown field \"{other}\"; the spellings are: names, synonyms, codes, all"
            )),
        }
    }

    fn matches(self) -> (bool, bool, bool) {
        match self {
            Self::Names => (false, true, false),
            Self::Synonyms => (false, false, true),
            Self::Codes => (true, false, false),
            Self::All => (true, true, true),
        }
    }
}

pub struct Options {
    pub query: String,
    pub system: Option<String>,
    pub field: Field,
    pub limit: usize,
    /// Imported-layer roots merged into the reading (`--imports`).
    pub imports: Vec<std::path::PathBuf>,
}

pub fn run(paths: &Paths, options: &Options) -> ExitCode {
    match search(paths, options) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::from(CLEAN)
        }
        Err(error) => fail(&error),
    }
}

struct Hit {
    system: String,
    code: String,
    field: &'static str,
    locale: String,
    text: String,
    imported: bool,
}

fn search(paths: &Paths, options: &Options) -> Result<String, String> {
    let vocab_root = paths.root.join("vocab");
    let corpus = corpus::read(&paths.schema, &paths.root, &vocab_root, &options.imports)?;
    if let Some(system) = &options.system {
        if !corpus.systems.contains(system) {
            return Err(format!(
                "unknown system \"{system}\"; the declared systems are: {}",
                corpus
                    .systems
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let needle = options.query.to_lowercase();
    let (codes, names, synonyms) = options.field.matches();
    let mut hits: Vec<Hit> = Vec::new();
    for concept in &corpus.concepts {
        if options
            .system
            .as_ref()
            .is_some_and(|s| s != &concept.system)
        {
            continue;
        }
        if codes && concept.code.to_lowercase().contains(&needle) {
            hits.push(Hit {
                system: concept.system.clone(),
                code: concept.code.clone(),
                field: "codes",
                locale: "-".to_owned(),
                text: concept.code.clone(),
                imported: concept.imported,
            });
        }
        for (field, matches, designations) in [
            ("names", names, &concept.names),
            ("synonyms", synonyms, &concept.synonyms),
        ] {
            if !matches {
                continue;
            }
            for (locale, text, _) in designations {
                if text.to_lowercase().contains(&needle) {
                    hits.push(Hit {
                        system: concept.system.clone(),
                        code: concept.code.clone(),
                        field,
                        locale: locale.clone(),
                        text: text.clone(),
                        imported: concept.imported,
                    });
                }
            }
        }
    }
    hits.sort_by(|a, b| {
        (&a.system, &a.code, a.field, &a.locale, &a.text, a.imported)
            .cmp(&(&b.system, &b.code, b.field, &b.locale, &b.text, b.imported))
    });
    let total = hits.len();
    let shown = total.min(options.limit);
    for hit in &hits[..shown] {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            hit.system,
            hit.code,
            hit.field,
            hit.locale,
            hit.text,
            if hit.imported {
                "imported"
            } else {
                "committed"
            }
        );
    }
    if total == 0 {
        return Ok(format!("no matches for \"{}\"", options.query));
    }
    if shown < total {
        eprintln!("vocabmod-se: {total} hit(s) total, showing the first {shown} (raise --limit)");
    }
    Ok(format!("{total} hit(s) for \"{}\"", options.query))
}
