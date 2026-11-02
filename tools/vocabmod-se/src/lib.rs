//! vocabmod-se: the vocabmod-se vocabulary corpus's companion CLI.
//!
//! It reads the committed KDL corpus through the generated vocab processor
//! (pkg/rs/vocab-proc) and writes the OMOP CDM and analysis artifacts the
//! repository ships beside it. `omop-export` writes the seven
//! Athena-complementary OMOP tables; `fhir-export` writes one FHIR R5
//! ConceptMap per mapping collection; `figures` computes the figure data
//! spec; `index` writes the deterministic statistical export;
//! `search` queries designations and codes interactively.

mod census;
mod corpus;
mod csv;
mod fhir_export;
mod figures;
mod figures_compute;
mod figures_eval;
mod figures_model;
mod figures_render;
mod idalloc;
mod importer;
mod index;
mod json;
mod omop_export;
mod omop_links;
mod omop_nodes;
mod omop_registry;
mod search;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Exit 0 clean, 1 findings or a failed run, 2 usage.
const CLEAN: u8 = 0;
const FINDINGS: u8 = 1;
const USAGE: u8 = 2;

#[derive(Parser)]
#[command(
    name = "vocabmod-se",
    about = "Companion CLI for the vocabmod-se vocabulary corpus"
)]
struct Cli {
    /// The repository root every default path resolves against.
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// Bind a source alias: NAME=DIR lets --source spell $NAME/<rest>.
    #[arg(long, global = true, value_name = "NAME=DIR")]
    alias: Vec<String>,
    /// Merge an imported-layer root into every reading (repeatable):
    /// search, index, omop-export, and fhir-export read it beside the
    /// committed tree; committed values win on conflict.
    #[arg(long, global = true, value_name = "DIR")]
    imports: Vec<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write Athena-complementary OMOP CDM tables from the corpus.
    ///
    /// Every emitted row is ours alone: concept ids at or above
    /// 2,000,000,000 and an empty standard_concept, exactly the rows
    /// Athena's own exports never carry, so the files merge beside them.
    ///
    /// Table contracts:
    ///
    /// CONCEPT — one row per tree concept that is not `OMOP_EXT`-bound
    /// (ext-bound concepts are Athena's own); ids come from committed
    /// `OMOP_CUSTOM` bindings or deterministic band allocation.
    ///
    /// CONCEPT_RELATIONSHIP — mapping assertions where at least one side
    /// resolves to one of our concept ids and the other side resolves to
    /// an id the tree itself names (our id, or a standard id from an
    /// `OMOP_EXT`/`OMOP_SNOMED_CT`/`OMOP_ICD_10_CM` endpoint). Each pair is emitted
    /// in both directions under inverse spellings. Pairs whose other
    /// side is a plain external code (SNOMED, ATC, ...) are skipped:
    /// their codes are not OMOP ids and are never used as ids.
    ///
    /// SOURCE_TO_CONCEPT_MAP — mappings of equivalent, narrower, or
    /// broader kind whose source is one of our concepts and whose target
    /// resolves to a standard OMOP id (a standard endpoint code, or a
    /// tree concept carrying an `OMOP_EXT` binding). Custom-to-custom
    /// pairs live in CONCEPT_RELATIONSHIP only.
    ///
    /// CONCEPT_SYNONYM — designations of concepts in our CONCEPT
    /// population, excluding the text already serving as concept_name.
    /// Designations in a locale without a confirmed OMOP language
    /// concept id (Swedish today) are skipped with a per-locale count in
    /// the summary and manifest — never exported under a guessed id.
    ///
    /// CONCEPT_ANCESTOR — transitive closure of declared parent links
    /// within our systems, with min/max levels of separation; parentless
    /// concepts are roots and emit no rows.
    ///
    /// CONCEPT_CLASS — one row per class spelling our CONCEPT rows use.
    ///
    /// RELATIONSHIP — definitions and inverse pairs for our own
    /// relationship spellings, as authored in the constants registry.
    OmopExport {
        /// The schema definition the corpus reads under.
        #[arg(long)]
        schema: Option<PathBuf>,
        /// The OMOP constants registry document.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// The directory the tables land in (default assets/omop/).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Comma-separated table selection (default: all seven):
        /// concept, concept-relationship, source-to-concept-map,
        /// concept-synonym, concept-ancestor, concept-class, relationship.
        #[arg(long, value_delimiter = ',')]
        tables: Vec<String>,
        /// Restrict the export to these systems (repeatable); filters
        /// concepts and the our-side endpoints of mapping rows.
        #[arg(long)]
        system: Vec<String>,
        /// Restrict mapping assertions to these collections (repeatable).
        #[arg(long)]
        collection: Vec<String>,
        /// Only mappings with a bound quality value (the default).
        #[arg(long, conflicts_with = "include_unassessed")]
        require_assessed: bool,
        /// Also include mappings without a bound quality value.
        #[arg(long)]
        include_unassessed: bool,
        /// Write a MANIFEST.csv sidecar documenting per-table row counts,
        /// filters, and generation inputs.
        #[arg(long)]
        manifest: bool,
        /// Compare the selected tables against the existing files and
        /// write nothing; exit 1 naming each drifted or missing table.
        #[arg(long)]
        check: bool,
    },
    /// Write FHIR R5 ConceptMap resources from the corpus.
    ///
    /// One JSON document per mapping collection with at least one
    /// selected mapping, named ConceptMap-<collection>.json: one group
    /// per source and target system, one element per source code, one
    /// target per asserted target code. Every document is a ConceptMap
    /// with status active; its id is the collection id, its url
    /// https://github.com/ludvigalden/vocabmod-se/fhir/ConceptMap/<id>,
    /// its version the collection version, its title the collection
    /// title, and its publisher the collection publisher's agent name.
    ///
    /// Relationship codes come from the R5 ConceptMapRelationship value
    /// set: equivalent, source-is-narrower-than-target,
    /// source-is-broader-than-target, related-to, and not-related-to
    /// spell the corpus kinds of the same names; partial_overlap takes
    /// related-to with the kind kept as the vocabmod-relationship target
    /// property.
    ///
    /// Display text is carried only where the corpus itself carries a
    /// designation for the code, chosen as omop-export chooses
    /// concept_name; systems the tree never defines (SNOMED CT, LOINC,
    /// MedDRA, and the other external and licensed systems) are code
    /// only. A target comment is the mapping's own comment text, notes
    /// joined by a blank line; notes the corpus marks suspect of quoting
    /// a licensed source are never emitted. A
    /// source-is-broader-than-target or not-related-to mapping without a
    /// comment is skipped (R5 requires one on an active map) and counted
    /// as comment-required.
    ///
    /// Group systems use the system's declared canonical-uri, else the
    /// HL7-published URI (SNOMED CT, LOINC, UCUM, RxNorm, ATC, ICD-10,
    /// ICD-10-CM, ICD-O-3, MedDRA), else
    /// https://github.com/ludvigalden/vocabmod-se/fhir/CodeSystem/<id>.
    /// Groups, elements, and targets are sorted; two runs over the same
    /// tree are byte-identical.
    FhirExport {
        /// The schema definition the corpus reads under.
        #[arg(long)]
        schema: Option<PathBuf>,
        /// The directory the documents land in (default assets/fhir/).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Restrict the export to mappings whose source or target system
        /// is one of these (repeatable).
        #[arg(long)]
        system: Vec<String>,
        /// Restrict the export to these collections (repeatable).
        #[arg(long)]
        collection: Vec<String>,
        /// Only mappings with a bound quality value (the default).
        #[arg(long, conflicts_with = "include_unassessed")]
        require_assessed: bool,
        /// Also include mappings without a bound quality value.
        #[arg(long)]
        include_unassessed: bool,
        /// Write a MANIFEST.csv sidecar documenting per-document group,
        /// element, and target counts, filters, and generation inputs.
        #[arg(long)]
        manifest: bool,
        /// Compare the documents against the existing files and write
        /// nothing; exit 1 naming each drifted or missing document.
        #[arg(long)]
        check: bool,
    },
    /// Compute the configured figures from the corpus.
    ///
    /// Reads the committed figure documents under assets/figures/
    /// through the figure profile and computes every figure number from
    /// the tree (concepts, mapping claims, omop bindings); no value is
    /// ever taken from a configuration. The deliverable is the figure
    /// data spec: one deterministic CSV per figure plus MANIFEST.csv
    /// under the output directory, sorted and stable across runs —
    /// rendering itself stays external.
    ///
    /// coverage-funnel: per section and row, the population the row's
    /// selector selects, then per track the denominator (release and
    /// defined denominators come from the tree's definitions), the
    /// numerator of every stage (codes of the population carrying a
    /// claim matching the stage selector — a mapping or an omop
    /// binding), the extra reach of every tier (codes the tier adds
    /// beyond all stages), and the presence bands. composition-bar: per
    /// segment the count over the claim population (first matching
    /// segment wins, misses reported as `other`), in mappings or in
    /// distinct source concepts.
    ///
    /// Selector semantics: `not` is NOR over its members; unknown values
    /// never match. A `target` selector naming a system the corpus
    /// declares matches its endpoints directly, and with `chain` also
    /// through further mappings and through target concepts carrying an
    /// omop binding. A system spelling the corpus does not declare fails
    /// closed. A `target-standard` selector matches claims whose target
    /// resolves in the local Athena release to an OMOP-standard concept,
    /// by id or code, directly or through one valid `Maps to`; selector
    /// leaves other than `target-standard` fail closed on an omop
    /// binding.
    ///
    /// Numbers that need a local usage dataset (by-count tracks,
    /// present/visible denominators, presence bands) are skipped with a
    /// named missing-input row naming the figure and the dataset when
    /// the gitignored local/ tree is absent; everything else still
    /// computes, and the run then exits 1 after writing. The
    /// `target-standard` numbers likewise need the local Athena release
    /// (--athena DIR; the default is <root>/temp/athena/x) and skip
    /// under a named note when it is absent.
    Figures {
        /// The schema definition the corpus reads under.
        #[arg(long)]
        schema: Option<PathBuf>,
        /// The directory of figure documents (default assets/figures/).
        #[arg(long)]
        figures: Option<PathBuf>,
        /// The directory the data files land in (default assets/figures/dist).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Compute only these figures (repeatable); an unknown name is
        /// a usage error naming the figures that exist.
        #[arg(long)]
        only: Vec<String>,
        /// The local Athena release directory (holding CONCEPT.csv)
        /// behind the target-standard leaf; default <root>/temp/athena/x.
        #[arg(long)]
        athena: Option<PathBuf>,
    },
    /// Render an operator-local source into KDL fragments under the
    /// never-committed landing zone (imports/). The
    /// committed corpus is never written; merged readings (search,
    /// index, omop-export, fhir-export) admit the fragments through
    /// --imports.
    Import {
        /// The source format: claml, pdf-text, tdl, csv, or rf2.
        format: String,
        /// The source to read: a path, or $NAME/<rest> against an
        /// --alias NAME=DIR binding. Never copied into the repository;
        /// only the derived fragments land under --into.
        #[arg(long)]
        source: String,
        /// The landing directory (default
        /// imports/<source>/<release>).
        #[arg(long)]
        into: Option<PathBuf>,
        /// The system id the fragments bind (e.g. `ICD_10_SE`).
        #[arg(long)]
        system: String,
        /// The release id the fragments bind.
        #[arg(long)]
        release: String,
        /// Write a manifest.kdl beside the fragments (source digest,
        /// format, counts).
        #[arg(long)]
        manifest: bool,
        /// Report what would land, write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Allow --into under the gitignored local/ document root
        /// instead of the landing zone (still never the tracked tree).
        #[arg(long)]
        allow_committed: bool,
    },
    /// Search concept designations and codes in the corpus.
    ///
    /// One reading of the whole tree, matched in memory: there is no
    /// persistent search index at this corpus's scale. The query is
    /// matched as a case-insensitive substring. `--field` selects what
    /// is matched: names (name designations), synonyms, codes (concept
    /// codes), or all (the default). Each hit prints one tab-separated
    /// line — system, code, field, locale, matched text — sorted by
    /// system then code. `--system` restricts to one declared system
    /// (an unknown spelling fails naming the declared systems);
    /// `--limit` caps the printed hits (default 50), with the total
    /// reported on stderr when truncated. Exit 0 with results or a
    /// clean no-match line; exit 1 on a read error or an unknown
    /// system.
    Search {
        /// The substring every candidate text is matched against,
        /// case-insensitively.
        query: String,
        /// Restrict the search to this declared system.
        #[arg(long)]
        system: Option<String>,
        /// What to match: names, synonyms, codes, or all (default).
        #[arg(long)]
        field: Option<String>,
        /// Print at most this many hits (default 50).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Write the deterministic statistical index of the corpus.
    ///
    /// One reading of the whole tree, exported as sorted tab-separated
    /// tables under the output directory (default assets/index/,
    /// gitignored): concepts.tsv (one row per concept with class,
    /// status, designation and parent counts, and omop binding),
    /// designations.tsv (one row per name and synonym), mappings.tsv
    /// (one row per mapping assertion with kind, assessment, quality,
    /// confidence, and provenance), bindings.tsv (one row per
    /// omop-binding), summary.tsv (counts by system, class, status,
    /// designation availability, mapping collection, kind, assessment,
    /// quality band, confidence scheme, and binding targets), and
    /// census-check.tsv.
    ///
    /// Every count the committed census artifact
    /// (vocab/_registry/census.kdl) covers is cross-checked in
    /// the same run — per-system concept statistics, per-collection
    /// mapping, assessed, per-kind, and per-tier counts, bindings by
    /// omop target, and the whole-tree totals; any divergence is a
    /// finding (exit 1) naming each one, never a silent drift.
    ///
    /// Deterministic by construction: sorted rows, stable columns, no
    /// timestamps — two runs over the same tree are byte-identical.
    /// `--system` (repeatable) restricts every table and the summary to
    /// the selected systems, with mappings in scope when their source
    /// system is selected and bindings when their concept is; the
    /// census cross-check then covers the selected systems' rows only
    /// and the whole-tree totals are not checked. An unknown system
    /// spelling fails (exit 1) naming the declared systems.
    Index {
        /// The directory the index files land in (default assets/index/).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Restrict the index to these systems (repeatable).
        #[arg(long)]
        system: Vec<String>,
    },
}

/// The resolved per-run paths, defaults anchored at the repository root.
pub struct Paths {
    pub root: PathBuf,
    pub schema: PathBuf,
    pub registry: PathBuf,
}

impl Paths {
    fn resolve(root: &Path, schema: Option<&Path>, registry: Option<&Path>) -> Self {
        Self {
            root: root.to_path_buf(),
            schema: schema
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.join("schema/vocab.kdl")),
            registry: registry
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.join("vocab/_registry/omop/omop.kdl")),
        }
    }
}

/// NAME=DIR spellings into alias bindings; a malformed spelling is a
/// usage error.
fn parse_aliases(spellings: &[String]) -> Result<Vec<(String, PathBuf)>, ExitCode> {
    let mut out = Vec::new();
    for spelling in spellings {
        let Some((name, dir)) = spelling.split_once('=') else {
            eprintln!("vocabmod-se: --alias {spelling} is not NAME=DIR");
            return Err(ExitCode::from(USAGE));
        };
        if name.is_empty() || dir.is_empty() {
            eprintln!("vocabmod-se: --alias {spelling} is not NAME=DIR");
            return Err(ExitCode::from(USAGE));
        }
        out.push((name.to_owned(), PathBuf::from(dir)));
    }
    Ok(out)
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let root = cli
        .root
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let aliases = match parse_aliases(&cli.alias) {
        Ok(aliases) => aliases,
        Err(code) => return code,
    };
    match cli.command {
        Command::Import {
            format,
            source,
            into,
            system,
            release,
            manifest,
            dry_run,
            allow_committed,
        } => {
            let paths = Paths::resolve(&root, None, None);
            let Some(format) = importer::Format::parse(&format) else {
                eprintln!(
                    "vocabmod-se: unknown import format \"{format}\"; the formats are: \
                     claml, pdf-text, tdl, csv, rf2"
                );
                return ExitCode::from(USAGE);
            };
            importer::run(
                &paths,
                &importer::Options {
                    format,
                    source,
                    into,
                    system,
                    release,
                    manifest,
                    dry_run,
                    allow_local: allow_committed,
                    aliases,
                },
            )
        }
        Command::OmopExport {
            schema,
            registry,
            out,
            tables,
            system,
            collection,
            require_assessed: _,
            include_unassessed,
            manifest,
            check,
        } => {
            let paths = Paths::resolve(&root, schema.as_deref(), registry.as_deref());
            let out = out.unwrap_or_else(|| root.join("assets/omop"));
            let options = match omop_export::Options::new(
                &tables,
                system,
                collection,
                include_unassessed,
                manifest,
                check,
            ) {
                Ok(options) => options,
                Err(error) => {
                    eprintln!("vocabmod-se: {error}");
                    return ExitCode::from(USAGE);
                }
            };
            let mut options = options;
            options.imports = cli.imports.clone();
            omop_export::run(&paths, &out, &options)
        }
        Command::FhirExport {
            schema,
            out,
            system,
            collection,
            require_assessed: _,
            include_unassessed,
            manifest,
            check,
        } => {
            let paths = Paths::resolve(&root, schema.as_deref(), None);
            let out = out.unwrap_or_else(|| root.join("assets/fhir"));
            let mut options =
                fhir_export::Options::new(system, collection, include_unassessed, manifest, check);
            options.imports = cli.imports.clone();
            fhir_export::run(&paths, &out, &options)
        }
        Command::Figures {
            schema,
            figures,
            out,
            only,
            athena,
        } => {
            let paths = Paths::resolve(&root, schema.as_deref(), None);
            let options = figures::Options {
                figures_dir: figures.unwrap_or_else(|| root.join("assets/figures")),
                out: out.unwrap_or_else(|| root.join("assets/figures/dist")),
                only,
                athena,
            };
            figures::run(&paths, &options)
        }
        Command::Search {
            query,
            system,
            field,
            limit,
        } => {
            let paths = Paths::resolve(&root, None, None);
            let field = match search::Field::parse(field.as_deref()) {
                Ok(field) => field,
                Err(error) => {
                    eprintln!("vocabmod-se: {error}");
                    return ExitCode::from(USAGE);
                }
            };
            search::run(
                &paths,
                &search::Options {
                    query,
                    system,
                    field,
                    limit: limit.unwrap_or(50),
                    imports: cli.imports.clone(),
                },
            )
        }
        Command::Index { out, system } => {
            let paths = Paths::resolve(&root, None, None);
            let out = out.unwrap_or_else(|| root.join("assets/index"));
            index::run(&paths, &out, &system, &cli.imports)
        }
    }
}

/// One stderr line per problem, then a failed exit: the tool's findings
/// channel.
pub fn fail(message: &str) -> ExitCode {
    eprintln!("vocabmod-se: {message}");
    ExitCode::from(FINDINGS)
}
