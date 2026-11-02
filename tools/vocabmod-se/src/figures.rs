//! The figures verb: consume the committed figure configurations through
//! the `figure` profile and compute every figure number from the corpus,
//! writing the deterministic per-figure data files (the figure data
//! spec) under `assets/figures/dist/`. Rendering itself stays external: the
//! data files are the deliverable.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vocab_proc::{Item, TypedItem, TypedReader};

use crate::corpus::{self, endpoint_of};
use crate::figures_compute::{Computed, Standardness, compute};
use crate::figures_eval::{Dataset, Eval, LocalInputs, Usage};
use crate::figures_model::{Figure, from_typed};
use crate::{CLEAN, FINDINGS, Paths, USAGE, fail};

pub struct Options {
    pub figures_dir: PathBuf,
    pub out: PathBuf,
    pub only: Vec<String>,
    /// The local Athena release directory (the one holding CONCEPT.csv)
    /// behind the `target-standard` leaf; the default resolves to
    /// `<root>/temp/athena/x`, and a run whose default is absent or
    /// passed `--athena` names it as a missing input.
    pub athena: Option<PathBuf>,
}

/// A run failure: a usage error names the caller's spelling (exit 2);
/// everything else is a failed run (exit 1).
enum RunError {
    Usage(String),
    Failed(String),
}

pub fn run(paths: &Paths, options: &Options) -> ExitCode {
    match figures(paths, options) {
        Ok(Ok(summary)) => {
            println!("{summary}");
            ExitCode::from(CLEAN)
        }
        Ok(Err(partial)) => {
            for diagnostic in &partial.diagnostics {
                eprintln!("vocabmod-se: {diagnostic}");
            }
            println!("{}", partial.summary);
            ExitCode::from(FINDINGS)
        }
        Err(RunError::Usage(error)) => {
            eprintln!("vocabmod-se: {error}");
            ExitCode::from(USAGE)
        }
        Err(RunError::Failed(error)) => fail(&error),
    }
}

/// A run whose figures computed with named missing inputs: partial
/// results, findings channel on stderr, exit 1.
struct Partial {
    summary: String,
    diagnostics: Vec<String>,
}

fn figures(paths: &Paths, options: &Options) -> Result<Result<String, Partial>, RunError> {
    let failed = |error: String| RunError::Failed(error);

    // The figure documents and the --only filter resolve before the
    // corpus read, so a typo fails in seconds, not minutes.
    let definition = corpus::compile(&paths.schema, &paths.root).map_err(failed)?;
    let reader = corpus::typed_reader(&definition, &paths.root).map_err(failed)?;
    let mut docs = read_figure_docs(&reader, &options.figures_dir).map_err(failed)?;
    if docs.is_empty() {
        return Err(failed(format!(
            "no figure documents under {}",
            options.figures_dir.display()
        )));
    }
    docs.sort_by(|a, b| a.id.cmp(&b.id));
    if !options.only.is_empty() {
        let known: BTreeSet<&str> = docs.iter().map(|figure| figure.id.as_str()).collect();
        for name in &options.only {
            if !known.contains(name.as_str()) {
                return Err(RunError::Usage(format!(
                    "--only \"{name}\" names no figure; the figures are: {}",
                    known.into_iter().collect::<Vec<_>>().join(", ")
                )));
            }
        }
        docs.retain(|figure| options.only.contains(&figure.id));
    }

    let vocab_root = paths.root.join("vocab");
    let corpus = corpus::read(&paths.schema, &paths.root, &vocab_root, &[]).map_err(failed)?;
    let local = read_local_inputs(&reader, &paths.root).map_err(failed)?;

    // The standardness resolver behind the target-standard leaf: loaded
    // once for the corpus's claim targets when the local Athena release
    // is present; a run without it still computes everything else.
    let athena_dir = options
        .athena
        .clone()
        .unwrap_or_else(|| paths.root.join("temp/athena/x"));
    let mut targets: BTreeSet<(String, String)> = BTreeSet::new();
    for mapping in &corpus.mappings {
        targets.insert(mapping.target.clone());
    }
    for binding in &corpus.bindings {
        targets.insert((binding.omop_system.clone(), binding.omop_code.clone()));
    }
    let standardness = match Standardness::load(&athena_dir, &targets).map_err(failed)? {
            Some(resolver) => Some(resolver),
            None => {
                eprintln!(
                    "vocabmod-se: no CONCEPT.csv under {} (pass --athena DIR); \
                     figures naming a target-standard selector skip those numbers",
                    athena_dir.display()
                );
                None
            }
        };

    let mut computed: Vec<(Figure, Computed)> = Vec::new();
    let mut eval = Eval::new(&corpus, standardness.as_ref());
    for figure in docs {
        let outcome = compute(&figure, &mut eval, &local)
            .map_err(|e| format!("figure {}: {e}", figure.id))
            .map_err(failed)?;
        computed.push((figure, outcome));
    }

    write_outputs(&options.out, &computed, &corpus.schema_digest).map_err(failed)?;
    let mut summary = format!(
        "wrote {} ({} figure(s), {} entries, {} concepts, {} mappings; schema digest {}):",
        options.out.display(),
        computed.len(),
        corpus.entries_read,
        corpus.concepts.len(),
        corpus.mappings.len(),
        corpus.schema_digest,
    );
    let mut diagnostics = Vec::new();
    for (figure, outcome) in &computed {
        summary.push_str(&format!(
            "\n{}\t{} data row(s)\t{} skipped row(s)",
            figure.id,
            outcome
                .rows
                .iter()
                .filter(|row| row.measure != "skipped")
                .count(),
            outcome
                .rows
                .iter()
                .filter(|row| row.measure == "skipped")
                .count(),
        ));
        for note in &outcome.notes {
            summary.push_str(&format!("\n{id}\t{note}", id = figure.id));
        }
        for missing in &outcome.missing {
            diagnostics.push(if missing == "athena" {
                format!(
                    "figure {} needs the local Athena release, not present on this host; the numbers that need it are skipped",
                    figure.id
                )
            } else {
                format!(
                    "figure {} needs local dataset {missing}, not present on this host; the numbers that need it are skipped",
                    figure.id
                )
            });
        }
    }
    if diagnostics.is_empty() {
        Ok(Ok(summary))
    } else {
        Ok(Err(Partial {
            summary,
            diagnostics,
        }))
    }
}

/// Every `figure` declaration under the figures directory, read one
/// document per reading under the figure profile.
fn read_figure_docs(reader: &TypedReader<'_, Item>, figures_dir: &Path) -> Result<Vec<Figure>, String> {
    let mut docs = Vec::new();
    for entry in kdl_documents(figures_dir)? {
        let shown = entry.display().to_string();
        let mut failure = None;
        let mut diagnostics: Vec<String> = Vec::new();
        let mut sink = |item: TypedItem<Item>| match item {
            TypedItem::Declaration {
                value: Item::Figure(figure),
                ..
            } => match from_typed(&figure) {
                Ok(figure) => docs.push(figure),
                Err(error) => {
                    failure.get_or_insert(format!("{shown}: {error}"));
                }
            },
            TypedItem::Diagnostic(record) => {
                if diagnostics.len() < 5 {
                    diagnostics.push(record.code.clone());
                }
            }
            _ => {}
        };
        reader
            .read(&[&entry], &mut sink)
            .map_err(|error| format!("reading {shown}: {error}"))?;
        if let Some(error) = failure {
            return Err(error);
        }
        if !diagnostics.is_empty() {
            return Err(format!(
                "{shown} reported diagnostics: {}",
                diagnostics.join(", ")
            ));
        }
    }
    let mut seen = BTreeSet::new();
    for figure in &docs {
        if !seen.insert(figure.id.as_str()) {
            return Err(format!("figure id {} declared twice", figure.id));
        }
    }
    Ok(docs)
}

/// Every `dataset` and `usage` record under the local tree, when the
/// host carries one; absent local inputs are not an error.
fn read_local_inputs(reader: &TypedReader<'_, Item>, root: &Path) -> Result<LocalInputs, String> {
    let mut local = LocalInputs::default();
    let local_root = root.join("local");
    if !local_root.is_dir() {
        return Ok(local);
    }
    for entry in kdl_documents(&local_root)? {
        let shown = entry.display().to_string();
        let mut failure = None;
        let mut diagnostics = 0usize;
        let mut sink = |item: TypedItem<Item>| {
            let value = match item {
                TypedItem::Declaration { value, .. } | TypedItem::Record { value, .. } => value,
                TypedItem::Diagnostic(_) => {
                    diagnostics += 1;
                    return;
                }
                _ => return,
            };
            match value {
                Item::Dataset(dataset) => {
                    let counts = dataset
                        .counts
                        .iter()
                        .map(|(name, count)| (name.clone(), count.unit.member().to_owned()))
                        .collect();
                    local.datasets.insert(
                        dataset.id.to_string(),
                        Dataset {
                            id: dataset.id.to_string(),
                            counts,
                        },
                    );
                }
                Item::Usage(usage) => match endpoint_of(&usage.concept) {
                    Some(endpoint) => local.usage.push(Usage {
                        dataset: usage.dataset.to_string(),
                        endpoint,
                        count: usage.count,
                        unit: usage.unit.member().to_owned(),
                    }),
                    None => {
                        failure.get_or_insert(format!(
                            "{shown}: a usage record binds no dataset, concept, count, or unit"
                        ));
                    }
                },
                _ => {}
            }
        };
        reader
            .read(&[&entry], &mut sink)
            .map_err(|error| format!("reading {shown}: {error}"))?;
        if let Some(error) = failure {
            return Err(error);
        }
        if diagnostics > 0 {
            return Err(format!("{shown} reported {diagnostics} diagnostic(s)"));
        }
    }
    Ok(local)
}

/// Every KDL document under `dir`, recursively, sorted.
fn kdl_documents(dir: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        let mut children: Vec<_> = std::fs::read_dir(dir)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|entry| entry.path())
            .collect();
        children.sort();
        for path in children {
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().is_some_and(|ext| ext == "kdl") {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(dir, &mut found).map_err(|e| format!("walking {}: {e}", dir.display()))?;
    Ok(found)
}

/// Publish the figure data spec: one deterministic data file per figure
/// plus the manifest recording what computed and what was skipped.
fn write_outputs(
    out: &Path,
    computed: &[(Figure, Computed)],
    schema_digest: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("creating {}: {e}", out.display()))?;
    for (figure, outcome) in computed {
        let labels: std::collections::BTreeMap<(&str, &str), &str> = figure
            .sections
            .iter()
            .flat_map(|(section_id, section)| {
                section.rows.iter().map(move |(row_id, row)| {
                    ((section_id.as_str(), row_id.as_str()), row.label.as_str())
                })
            })
            .collect();
        let display = crate::figures_render::display_rows(figure, &outcome.rows);
        let all: Vec<&crate::figures_compute::DataRow> = outcome
            .rows
            .iter()
            .chain(display.iter().flatten())
            .collect();
        let rows: Vec<[String; 10]> = all
            .iter()
            .map(|row| {
                [
                    row.section.clone(),
                    row.row.clone(),
                    labels
                        .get(&(row.section.as_str(), row.row.as_str()))
                        .map(|label| (*label).to_owned())
                        .unwrap_or_default(),
                    row.track.clone(),
                    row.measure.clone(),
                    row.item.clone(),
                    row.denominator.clone(),
                    row.numerator.clone(),
                    row.share_pct.clone(),
                    row.note.clone(),
                ]
            })
            .collect();
        let text = crate::csv::table(
            &[
                "section",
                "row",
                "label",
                "track",
                "measure",
                "item",
                "denominator",
                "numerator",
                "share_pct",
                "note",
            ],
            &rows,
        );
        crate::omop_export::write_atomically(&out.join(format!("{}.csv", figure.id)), &text)?;
        // A figure with a display policy and a declared svg output gets
        // its README rendering beside the data file, from the same rows;
        // a partial run (a named missing input) renders nothing, so it
        // never overwrites a complete rendering with an empty one.
        if let Some(display) = display.as_ref().filter(|_| outcome.missing.is_empty()) {
            if figure.output_formats.iter().any(|format| format == "svg") {
                let mut labelled: Vec<crate::figures_compute::DataRow> = Vec::new();
                for row in outcome.rows.iter().chain(display.iter()) {
                    labelled.push(crate::figures_compute::DataRow {
                        label: labels
                            .get(&(row.section.as_str(), row.row.as_str()))
                            .map(|label| (*label).to_owned())
                            .unwrap_or_default(),
                        ..row.clone()
                    });
                }
                let svg = crate::figures_render::coverage_svg(&labelled);
                crate::omop_export::write_atomically(&out.join(format!("{}.svg", figure.id)), &svg)?;
            }
        }
    }
    let manifest_rows: Vec<[String; 6]> = computed
        .iter()
        .map(|(figure, outcome)| {
            [
                figure.id.clone(),
                figure.kind.clone(),
                outcome
                    .rows
                    .iter()
                    .filter(|row| row.measure != "skipped")
                    .count()
                    .to_string(),
                outcome
                    .rows
                    .iter()
                    .filter(|row| row.measure == "skipped")
                    .count()
                    .to_string(),
                outcome.missing.join("+"),
                outcome.notes.join(";"),
            ]
        })
        .collect();
    let manifest = crate::csv::table(
        &[
            "figure",
            "kind",
            "rows",
            "skipped_rows",
            "missing_inputs",
            "notes",
        ],
        &manifest_rows,
    );
    let manifest = format!("{manifest}# schema_digest={schema_digest}\n");
    crate::omop_export::write_atomically(&out.join("MANIFEST.csv"), &manifest)?;
    Ok(())
}
