//! The omop-export verb: the seven Athena-complementary OMOP CDM tables,
//! selected and filtered per run, written atomically beside a manifest
//! that records what each table contains and which inputs produced it.
//! The per-table population contracts live in the verb's --help.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

use crate::corpus::{self, Corpus};
use crate::omop_links;
use crate::omop_nodes::{self, Population};
use crate::omop_registry::{self, OmopRegistry};
use crate::{CLEAN, Paths, fail};

/// Every id at or above this bound is ours; anything below it in an
/// emitted table is a standard id the tree itself named.
const OUR_ID_FLOOR: u64 = 2_000_000_000;

/// The selected tables and filters one export run applies.
pub struct Options {
    pub tables: BTreeSet<Table>,
    pub systems: BTreeSet<String>,
    pub collections: BTreeSet<String>,
    /// The default gate: only mappings with a bound quality value.
    pub require_assessed: bool,
    pub manifest: bool,
    pub check: bool,
    /// Imported-layer roots merged into the reading (`--imports`); the
    /// export merges beside them but stays Athena-column-compatible:
    /// per-layer provenance is recorded in the manifest, not as extra
    /// table columns.
    pub imports: Vec<std::path::PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Table {
    Concept,
    ConceptRelationship,
    SourceToConceptMap,
    ConceptSynonym,
    ConceptAncestor,
    ConceptClass,
    Relationship,
}

const TABLE_NAMES: [(&str, Table); 7] = [
    ("concept", Table::Concept),
    ("concept-relationship", Table::ConceptRelationship),
    ("source-to-concept-map", Table::SourceToConceptMap),
    ("concept-synonym", Table::ConceptSynonym),
    ("concept-ancestor", Table::ConceptAncestor),
    ("concept-class", Table::ConceptClass),
    ("relationship", Table::Relationship),
];

impl Table {
    fn file_name(self) -> &'static str {
        match self {
            Table::Concept => "CONCEPT.csv",
            Table::ConceptRelationship => "CONCEPT_RELATIONSHIP.csv",
            Table::SourceToConceptMap => "SOURCE_TO_CONCEPT_MAP.csv",
            Table::ConceptSynonym => "CONCEPT_SYNONYM.csv",
            Table::ConceptAncestor => "CONCEPT_ANCESTOR.csv",
            Table::ConceptClass => "CONCEPT_CLASS.csv",
            Table::Relationship => "RELATIONSHIP.csv",
        }
    }
}

impl Options {
    pub fn new(
        tables: &[String],
        systems: Vec<String>,
        collections: Vec<String>,
        include_unassessed: bool,
        manifest: bool,
        check: bool,
    ) -> Result<Self, String> {
        let tables = if tables.is_empty() {
            TABLE_NAMES.iter().map(|(_, table)| *table).collect()
        } else {
            let mut selected = BTreeSet::new();
            for name in tables {
                let Some((_, table)) = TABLE_NAMES.iter().find(|(spelling, _)| spelling == name)
                else {
                    return Err(format!(
                        "unknown table \"{name}\"; the spellings are: {}",
                        TABLE_NAMES
                            .iter()
                            .map(|(spelling, _)| *spelling)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                };
                selected.insert(*table);
            }
            selected
        };
        Ok(Self {
            imports: Vec::new(),
            tables,
            systems: systems.into_iter().collect(),
            collections: collections.into_iter().collect(),
            require_assessed: !include_unassessed,
            manifest,
            check,
        })
    }

    /// The filters cell every manifest row carries, spelled the same way
    /// for each table so a merge is auditable at a glance.
    fn filters_cell(&self) -> String {
        let joined = |set: &BTreeSet<String>| set.iter().cloned().collect::<Vec<_>>().join("+");
        format!(
            "systems={};collections={};assessed={}",
            if self.systems.is_empty() {
                "all".to_owned()
            } else {
                joined(&self.systems)
            },
            if self.collections.is_empty() {
                "all".to_owned()
            } else {
                joined(&self.collections)
            },
            if self.require_assessed {
                "required"
            } else {
                "included-unassessed"
            },
        )
    }
}

/// One table's built content with the notes its manifest row carries.
struct Built {
    table: Table,
    text: String,
    rows: usize,
    skipped: usize,
    notes: Vec<String>,
}

impl Built {
    fn new<const N: usize>(
        table: Table,
        header: &[&str; N],
        rows: &[[String; N]],
        skipped: usize,
        notes: Vec<String>,
    ) -> Self {
        Self {
            table,
            text: crate::csv::table(header, rows),
            rows: rows.len(),
            skipped,
            notes,
        }
    }
}

pub fn run(paths: &Paths, out: &Path, options: &Options) -> ExitCode {
    match export(paths, out, options) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::from(CLEAN)
        }
        Err(error) => fail(&error),
    }
}

fn export(paths: &Paths, out: &Path, options: &Options) -> Result<String, String> {
    let registry = omop_registry::read(&paths.schema, &paths.root, &paths.registry)
        .map_err(|e| format!("{}: {e}", paths.registry.display()))?;
    for system in &options.systems {
        if !registry.vocabulary_id.contains_key(system) {
            return Err(format!(
                "--system \"{system}\" names no system the registry maps"
            ));
        }
    }

    let vocab_root = paths.root.join("vocab");
    let corpus = corpus::read(&paths.schema, &paths.root, &vocab_root, &options.imports)?;
    validate_collections(&corpus, options)?;

    let (population, concept_rows, concept_stats) =
        omop_nodes::build_concept(&corpus, &registry, options)?;
    let mut built = Vec::new();
    if options.tables.contains(&Table::Concept) {
        let mut notes = vec![
            format!("committed-custom={}", concept_stats.committed),
            format!("generated={}", concept_stats.generated),
            format!("ext-bound-skipped={}", concept_stats.ext_skipped),
        ];
        if concept_stats.name_fallbacks > 0 {
            notes.push(format!("name-fallbacks={}", concept_stats.name_fallbacks));
        }
        if concept_stats.status_unbound > 0 {
            notes.push(format!("status-unbound={}", concept_stats.status_unbound));
        }
        built.push(Built::new(
            Table::Concept,
            &[
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
            ],
            &concept_rows,
            concept_stats.ext_skipped,
            notes,
        ));
    }
    if options.tables.contains(&Table::ConceptRelationship) {
        let (rows, stats) =
            omop_links::build_concept_relationship(&corpus, &registry, &population, options)?;
        verify_ids(Table::ConceptRelationship, &rows, &[0, 1], &population)?;
        built.push(Built::new(
            Table::ConceptRelationship,
            &[
                "concept_id_1",
                "concept_id_2",
                "relationship_id",
                "valid_start_date",
                "valid_end_date",
                "invalid_reason",
            ],
            &rows,
            stats.total_skipped(),
            stats.notes(),
        ));
    }
    if options.tables.contains(&Table::SourceToConceptMap) {
        let (rows, stats) =
            omop_links::build_source_to_concept_map(&corpus, &registry, &population, options)?;
        verify_ids(Table::SourceToConceptMap, &rows, &[1, 4], &population)?;
        built.push(Built::new(
            Table::SourceToConceptMap,
            &[
                "source_code",
                "source_concept_id",
                "source_vocabulary_id",
                "source_code_description",
                "target_concept_id",
                "target_vocabulary_id",
                "target_code",
                "target_code_description",
                "valid_start_date",
                "valid_end_date",
                "invalid_reason",
            ],
            &rows,
            stats.total_skipped(),
            stats.notes(),
        ));
    }
    if options.tables.contains(&Table::ConceptSynonym) {
        let (rows, stats) =
            omop_nodes::build_concept_synonym(&corpus, &registry, &population, options);
        verify_ids(Table::ConceptSynonym, &rows, &[0], &population)?;
        built.push(Built::new(
            Table::ConceptSynonym,
            &["concept_id", "concept_synonym_name", "language_concept_id"],
            &rows,
            stats.total_skipped(),
            stats.notes(),
        ));
    }
    if options.tables.contains(&Table::ConceptAncestor) {
        let (rows, stats) = omop_nodes::build_concept_ancestor(&population, options)?;
        verify_ids(Table::ConceptAncestor, &rows, &[0, 1], &population)?;
        built.push(Built::new(
            Table::ConceptAncestor,
            &[
                "ancestor_concept_id",
                "descendant_concept_id",
                "min_levels_of_separation",
                "max_levels_of_separation",
            ],
            &rows,
            stats.total_skipped(),
            stats.notes(),
        ));
    }
    if options.tables.contains(&Table::ConceptClass) {
        let rows = omop_nodes::build_concept_class(&concept_rows);
        built.push(Built::new(
            Table::ConceptClass,
            &[
                "concept_class_id",
                "concept_class_name",
                "concept_class_id_concept_id",
            ],
            &rows,
            0,
            Vec::new(),
        ));
    }
    if options.tables.contains(&Table::Relationship) {
        let rows = omop_links::build_relationship(&registry);
        built.push(Built::new(
            Table::Relationship,
            &[
                "relationship_id",
                "relationship_name",
                "is_hierarchical",
                "defines_ancestry",
                "reverse_relationship_id",
            ],
            &rows,
            0,
            Vec::new(),
        ));
    }

    if options.check {
        return check_against(out, &built);
    }
    write_all(out, &built, &registry, &corpus, options)?;
    Ok(summary(&built, &corpus, &registry, out, options))
}

/// A collection that never appears is a typo, not an empty selection.
fn validate_collections(corpus: &Corpus, options: &Options) -> Result<(), String> {
    if options.collections.is_empty() {
        return Ok(());
    }
    let present: BTreeSet<&str> = corpus
        .mappings
        .iter()
        .map(|mapping| mapping.collection.as_str())
        .collect();
    for collection in &options.collections {
        if !present.contains(collection.as_str()) {
            return Err(format!(
                "--collection \"{collection}\" names no collection the corpus asserts"
            ));
        }
    }
    Ok(())
}

/// Every id of ours a table names must be one CONCEPT.csv carries: the
/// consistency proof, enforced on the built rows before any write.
fn verify_ids<const N: usize>(
    table: Table,
    rows: &[[String; N]],
    columns: &[usize],
    population: &Population,
) -> Result<(), String> {
    for row in rows {
        for column in columns {
            let Ok(id) = row[*column].parse::<u64>() else {
                continue;
            };
            if id >= OUR_ID_FLOOR && !population.by_id.contains_key(&id) {
                return Err(format!(
                    "table {} names id {id} which no CONCEPT row carries",
                    table.file_name()
                ));
            }
        }
    }
    Ok(())
}

/// Compare each selected table against the file on disk; write nothing.
fn check_against(out: &Path, built: &[Built]) -> Result<String, String> {
    let mut drifted = Vec::new();
    for table in built {
        let path = out.join(table.table.file_name());
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == table.text => {}
            Ok(_) => drifted.push(format!("{} differs", table.table.file_name())),
            Err(_) => drifted.push(format!("{} missing", table.table.file_name())),
        }
    }
    if drifted.is_empty() {
        return Ok(format!(
            "checked {}: {} table(s) identical",
            out.display(),
            built.len()
        ));
    }
    Err(format!(
        "drift in {}: {}",
        out.display(),
        drifted.join("; ")
    ))
}

fn write_all(
    out: &Path,
    built: &[Built],
    registry: &OmopRegistry,
    corpus: &Corpus,
    options: &Options,
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("creating {}: {e}", out.display()))?;
    for table in built {
        let path = out.join(table.table.file_name());
        write_atomically(&path, &table.text)?;
    }
    if options.manifest {
        let bands = match (
            registry.id_bands.values().map(|(first, _)| *first).min(),
            registry.id_bands.values().map(|(_, last)| *last).max(),
        ) {
            (Some(first), Some(last)) => format!("{}-{}x{}", first, last, registry.id_bands.len()),
            _ => "none".to_owned(),
        };
        let manifest_row = |table: &Built| {
            [
                table.table.file_name().to_owned(),
                table.rows.to_string(),
                table.skipped.to_string(),
                options.filters_cell(),
                corpus.schema_digest.clone(),
                registry.version.clone(),
                bands.clone(),
                table.notes.join(";"),
            ]
        };
        let merged_note = (!options.imports.is_empty()).then(|| {
            format!(
                "merged-reading concepts={} mappings={}",
                corpus.concepts.iter().filter(|c| c.imported).count(),
                corpus.mappings.iter().filter(|m| m.layer_imported).count()
            )
        });
        let manifest_row = |table: &Built| -> [String; 8] {
            let mut notes = table.notes.join(";");
            if let Some(note) = &merged_note {
                if !notes.is_empty() {
                    notes.push(';');
                }
                notes.push_str(note);
            }
            [
                table.table.file_name().to_owned(),
                table.rows.to_string(),
                table.skipped.to_string(),
                options.filters_cell(),
                corpus.schema_digest.clone(),
                registry.version.clone(),
                bands.clone(),
                notes,
            ]
        };
        let mut text = crate::csv::table(
            &[
                "table",
                "rows",
                "skipped",
                "filters",
                "schema_digest",
                "registry_version",
                "id_bands",
                "notes",
            ],
            &built.iter().map(manifest_row).collect::<Vec<_>>(),
        );
        text.push_str(&format!(
            "# entries={};concepts={};bindings={};mappings={};require-assessed={}\n",
            corpus.entries_read,
            corpus.concepts.len(),
            corpus.bindings.len(),
            corpus.mappings.len(),
            options.require_assessed,
        ));
        write_atomically(&out.join("MANIFEST.csv"), &text)?;
    }
    Ok(())
}

fn summary(
    built: &[Built],
    corpus: &Corpus,
    registry: &OmopRegistry,
    out: &Path,
    options: &Options,
) -> String {
    let mut text = format!(
        "wrote {} ({} entries, {} concepts, {} mappings; schema digest {}, registry v{}):",
        out.display(),
        corpus.entries_read,
        corpus.concepts.len(),
        corpus.mappings.len(),
        corpus.schema_digest,
        registry.version,
    );
    for table in built {
        let notes = (!table.notes.is_empty())
            .then(|| format!(" ({})", table.notes.join("; ")))
            .unwrap_or_default();
        text.push_str(&format!(
            "\n{}\t{} rows\t{} skipped{notes}",
            table.table.file_name(),
            table.rows,
            table.skipped
        ));
    }
    if options.require_assessed {
        let unassessed = corpus.mappings.iter().filter(|m| !m.assessed).count();
        if unassessed > 0 {
            text.push_str(&format!(
                "\nmapping quality gate: {unassessed} unassessed mapping(s) excluded (--include-unassessed adds them)"
            ));
        }
    }
    text
}

/// Publish atomically: stage beside the destination, then rename over it,
/// so a reader never sees a partial table.
pub(crate) fn write_atomically(out: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    let mut staged = out.as_os_str().to_owned();
    staged.push(".part");
    let staged = Path::new(&staged);
    std::fs::write(staged, text).map_err(|e| format!("writing {}: {e}", staged.display()))?;
    std::fs::rename(staged, out).map_err(|e| format!("publishing {}: {e}", out.display()))?;
    Ok(())
}
