//! The fhir-export verb: one FHIR R5 ConceptMap document per mapping
//! collection, selected and filtered per run with the omop-export
//! grammar, written atomically beside an optional manifest. The document
//! contract lives in the verb's --help.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use crate::corpus::{self, Corpus, MappingData};
use crate::json::{self, Value};
use crate::omop_export::write_atomically;
use crate::{CLEAN, Paths, fail};

/// The base every identifier this export mints sits under: the corpus's
/// public home. The identifiers name; they are not promised to resolve.
const BASE: &str = "https://github.com/ludvigalden/vocabmod-se/fhir";

/// The HL7-published canonical URIs for the external systems the corpus
/// names, from the FHIR terminology pages and HL7 Terminology. A system
/// declaring its own `canonical-uri` overrides this table; every other
/// system takes a minted `BASE/CodeSystem/<id>` identifier. The OMOP id
/// spaces are OMOP concept ids, never the codes of the vocabulary they
/// index, so they never take that vocabulary's URI.
const HL7_SYSTEM_URIS: [(&str, &str); 10] = [
    ("ATC", "http://www.whocc.no/atc"),
    ("ICD_10", "http://hl7.org/fhir/sid/icd-10"),
    ("ICD_10_CM", "http://hl7.org/fhir/sid/icd-10-cm"),
    ("ICD_O_3", "http://terminology.hl7.org/CodeSystem/icd-o-3"),
    ("LOINC", "http://loinc.org"),
    ("MEDDRA", "https://www.meddra.org"),
    ("RXNORM", "http://www.nlm.nih.gov/research/umls/rxnorm"),
    ("SNOMED_CT", "http://snomed.info/sct"),
    ("SNOMED_CT_SE_EXT", "http://snomed.info/sct"),
    ("UCUM", "http://unitsofmeasure.org"),
];

/// The ConceptMap property that carries a corpus kind the R5
/// relationship value set has no code for.
const KIND_PROPERTY: &str = "vocabmod-relationship";

/// The selected filters one export run applies.
pub struct Options {
    pub systems: BTreeSet<String>,
    pub collections: BTreeSet<String>,
    /// The default gate: only mappings with a bound quality value.
    pub require_assessed: bool,
    pub manifest: bool,
    pub check: bool,
    /// Imported-layer roots merged into the reading (`--imports`).
    pub imports: Vec<std::path::PathBuf>,
}

impl Options {
    pub fn new(
        systems: Vec<String>,
        collections: Vec<String>,
        include_unassessed: bool,
        manifest: bool,
        check: bool,
    ) -> Self {
        Self {
            systems: systems.into_iter().collect(),
            collections: collections.into_iter().collect(),
            require_assessed: !include_unassessed,
            manifest,
            check,
            imports: Vec::new(),
        }
    }

    /// The filters cell every manifest row carries, spelled as
    /// omop-export spells it.
    fn filters_cell(&self) -> String {
        let joined = |set: &BTreeSet<String>| {
            if set.is_empty() {
                "all".to_owned()
            } else {
                set.iter().cloned().collect::<Vec<_>>().join("+")
            }
        };
        format!(
            "systems={};collections={};assessed={}",
            joined(&self.systems),
            joined(&self.collections),
            if self.require_assessed {
                "required"
            } else {
                "included-unassessed"
            },
        )
    }
}

/// The R5 ConceptMapRelationship code for a corpus kind, and whether the
/// code states the kind exactly. The kinds read source to target, as the
/// R5 codes do; partial_overlap has no R5 code and takes related-to, the
/// parent code, with the kind itself kept as a target property.
fn relationship_code(kind: &str) -> Result<(&'static str, bool), String> {
    Ok(match kind {
        "equivalent" => ("equivalent", true),
        "source_is_narrower_than_target" => ("source-is-narrower-than-target", true),
        "source_is_broader_than_target" => ("source-is-broader-than-target", true),
        "related_to" => ("related-to", true),
        "not_related_to" => ("not-related-to", true),
        "partial_overlap" => ("related-to", false),
        other => return Err(format!("mapping relationship \"{other}\" has no FHIR code")),
    })
}

/// R5 requires a comment on these codes unless the map is a draft; the
/// documents are active, so a target without one is skipped.
fn needs_comment(code: &str) -> bool {
    matches!(code, "source-is-broader-than-target" | "not-related-to")
}

/// Why mapping assertions were skipped, tallied per run or per document.
#[derive(Default)]
struct Tally {
    unassessed: usize,
    collection: usize,
    system: usize,
    comment_required: usize,
    undeclared_system: usize,
    comments_withheld: usize,
    deduped: usize,
}

/// One target entry as it is emitted, ordered for byte stability.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Target {
    code: String,
    relationship: &'static str,
    /// The corpus kind, kept only where the code does not state it.
    kind: Option<String>,
    display: Option<String>,
    comment: Option<String>,
}

/// One collection's built document with the counts its manifest row and
/// summary line carry.
struct Built {
    file: String,
    collection: String,
    text: String,
    groups: usize,
    elements: usize,
    targets: usize,
    notes: Vec<String>,
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
    let vocab_root = paths.root.join("vocab");
    let corpus = corpus::read(&paths.schema, &paths.root, &vocab_root, &options.imports)?;
    validate(&corpus, options)?;

    let names = designations(&corpus);
    let mut run_tally = Tally::default();
    // collection -> (source uri, target uri) -> (source system, code) -> targets
    type Groups = BTreeMap<(String, String), BTreeMap<(String, String), BTreeSet<Target>>>;
    let mut selected: BTreeMap<&str, (Groups, Tally)> = BTreeMap::new();
    for mapping in &corpus.mappings {
        if !gate(mapping, options, &mut run_tally) {
            continue;
        }
        let (relationship, exact) = relationship_code(&mapping.relationship)?;
        let (groups, tally) = selected.entry(mapping.collection.as_str()).or_default();
        // An endpoint whose system the corpus never declares has no
        // CodeSystem to name; it is skipped, never given a guessed one.
        let declared = corpus.systems.contains(&mapping.source.0)
            && corpus.systems.contains(&mapping.target.0);
        if !declared {
            tally.undeclared_system += 1;
            continue;
        }
        tally.comments_withheld += mapping.comments_withheld;
        let comment = (!mapping.comments.is_empty()).then(|| mapping.comments.join("\n\n"));
        if comment.is_none() && needs_comment(relationship) {
            tally.comment_required += 1;
            continue;
        }
        let target = Target {
            code: mapping.target.1.clone(),
            relationship,
            kind: (!exact).then(|| mapping.relationship.clone()),
            display: names.get(&mapping.target).cloned(),
            comment,
        };
        // Systems sharing one URI (SNOMED CT and its Swedish extension)
        // share one group.
        let inserted = groups
            .entry((
                system_uri(&corpus, &mapping.source.0),
                system_uri(&corpus, &mapping.target.0),
            ))
            .or_default()
            .entry(mapping.source.clone())
            .or_default()
            .insert(target);
        if !inserted {
            tally.deduped += 1;
        }
    }

    let mut built = Vec::new();
    for (collection, (groups, tally)) in &selected {
        if groups.is_empty() {
            continue;
        }
        built.push(document(&corpus, &names, collection, groups, tally)?);
    }

    if options.check {
        return check_against(out, &built);
    }
    write_all(out, &built, &corpus, options)?;
    Ok(summary(&built, &corpus, &run_tally, out, options))
}

/// An unknown --system or --collection is a typo, not an empty selection.
fn validate(corpus: &Corpus, options: &Options) -> Result<(), String> {
    for system in &options.systems {
        if !corpus.systems.contains(system) {
            return Err(format!(
                "--system \"{system}\" names no system the corpus declares"
            ));
        }
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

/// The shared collection, quality, and system gates; false means skip. A
/// mapping is in system scope when its source or target system is
/// selected.
fn gate(mapping: &MappingData, options: &Options, tally: &mut Tally) -> bool {
    if !options.collections.is_empty() && !options.collections.contains(&mapping.collection) {
        tally.collection += 1;
        return false;
    }
    if options.require_assessed && !mapping.assessed {
        tally.unassessed += 1;
        return false;
    }
    if !options.systems.is_empty()
        && !options.systems.contains(&mapping.source.0)
        && !options.systems.contains(&mapping.target.0)
    {
        tally.system += 1;
        return false;
    }
    true
}

/// The designation each tree concept carries, chosen as omop-export
/// chooses concept_name. Systems the tree never defines (the licensed
/// terminologies among them) carry none, so their codes never gain one.
fn designations(corpus: &Corpus) -> BTreeMap<(String, String), String> {
    corpus
        .concepts
        .iter()
        .filter_map(|concept| {
            crate::omop_nodes::pick_name(concept)
                .map(|name| ((concept.system.clone(), concept.code.clone()), name))
        })
        .collect()
}

/// The canonical URI a system is written under in group.source and
/// group.target.
fn system_uri(corpus: &Corpus, system: &str) -> String {
    if let Some(uri) = corpus.system_uris.get(system) {
        return uri.clone();
    }
    HL7_SYSTEM_URIS
        .iter()
        .find(|(id, _)| *id == system)
        .map(|(_, uri)| (*uri).to_owned())
        .unwrap_or_else(|| format!("{BASE}/CodeSystem/{system}"))
}

/// A collection id as a FHIR resource id: the id itself, which must
/// already satisfy the id grammar (letters, digits, '-' and '.', at most
/// 64 characters).
fn resource_id(collection: &str) -> Result<&str, String> {
    let legal = !collection.is_empty()
        && collection.len() <= 64
        && collection
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if legal {
        Ok(collection)
    } else {
        Err(format!(
            "collection \"{collection}\" is not a legal FHIR resource id"
        ))
    }
}

/// The computable name: the id's alphanumeric runs, each capitalized and
/// concatenated (icd-10-se-snomed-ct -> Icd10SeSnomedCt).
fn computable_name(id: &str) -> String {
    let mut name: String = id
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect();
    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
        name.insert_str(0, "Map");
    }
    name
}

/// The English text when one exists, else the first in declaration order.
fn english(texts: &[(String, String)]) -> Option<&str> {
    texts
        .iter()
        .find(|(locale, _)| locale == "en")
        .or_else(|| texts.first())
        .map(|(_, text)| text.as_str())
}

fn document(
    corpus: &Corpus,
    names: &BTreeMap<(String, String), String>,
    collection: &str,
    groups: &BTreeMap<(String, String), BTreeMap<(String, String), BTreeSet<Target>>>,
    tally: &Tally,
) -> Result<Built, String> {
    let id = resource_id(collection)?;
    let declared = corpus.collections.get(collection);
    let mut members: Vec<(&'static str, Value)> = vec![
        ("resourceType", Value::str("ConceptMap")),
        ("id", Value::str(id)),
        ("url", Value::String(format!("{BASE}/ConceptMap/{id}"))),
    ];
    if let Some(declared) = declared {
        members.push(("version", Value::str(&declared.version)));
    }
    members.push(("name", Value::String(computable_name(id))));
    if let Some(title) = declared.and_then(|declared| english(&declared.titles)) {
        members.push(("title", Value::str(title)));
    }
    members.push(("status", Value::str("active")));
    let publisher = declared
        .and_then(|declared| declared.publisher.as_ref())
        .and_then(|agent| corpus.agents.get(agent))
        .and_then(|agent| english(agent));
    if let Some(publisher) = publisher {
        members.push(("publisher", Value::str(publisher)));
    }
    let uses_kind = groups
        .values()
        .flat_map(BTreeMap::values)
        .flatten()
        .any(|target| target.kind.is_some());
    if uses_kind {
        members.push((
            "property",
            Value::Array(vec![Value::Object(vec![
                ("code", Value::str(KIND_PROPERTY)),
                (
                    "uri",
                    Value::String(format!("{BASE}/ConceptMap-property/{KIND_PROPERTY}")),
                ),
                (
                    "description",
                    Value::str(
                        "The corpus kind, where the relationship code is its nearest R5 parent",
                    ),
                ),
                ("type", Value::str("string")),
            ])]),
        ));
    }

    let (mut elements, mut targets) = (0, 0);
    let mut group_values = Vec::new();
    for ((source, target), codes) in groups {
        let mut element_values = Vec::new();
        for (key, entries) in codes {
            elements += 1;
            let mut element = vec![("code", Value::str(&key.1))];
            if let Some(display) = names.get(key) {
                element.push(("display", Value::str(display)));
            }
            let mut target_values = Vec::new();
            for entry in entries {
                targets += 1;
                let mut value = vec![("code", Value::str(&entry.code))];
                if let Some(display) = &entry.display {
                    value.push(("display", Value::str(display)));
                }
                value.push(("relationship", Value::str(entry.relationship)));
                if let Some(comment) = &entry.comment {
                    value.push(("comment", Value::str(comment)));
                }
                if let Some(kind) = &entry.kind {
                    value.push((
                        "property",
                        Value::Array(vec![Value::Object(vec![
                            ("code", Value::str(KIND_PROPERTY)),
                            ("valueString", Value::str(kind)),
                        ])]),
                    ));
                }
                target_values.push(Value::Object(value));
            }
            element.push(("target", Value::Array(target_values)));
            element_values.push(Value::Object(element));
        }
        group_values.push(Value::Object(vec![
            ("source", Value::str(source)),
            ("target", Value::str(target)),
            ("element", Value::Array(element_values)),
        ]));
    }
    let group_count = group_values.len();
    members.push(("group", Value::Array(group_values)));

    let mut notes = Vec::new();
    if tally.comment_required > 0 {
        notes.push(format!("comment-required={}", tally.comment_required));
    }
    if tally.undeclared_system > 0 {
        notes.push(format!("undeclared-system={}", tally.undeclared_system));
    }
    if tally.comments_withheld > 0 {
        notes.push(format!("comments-withheld={}", tally.comments_withheld));
    }
    notes.push(format!("deduped={}", tally.deduped));
    Ok(Built {
        file: format!("ConceptMap-{id}.json"),
        collection: collection.to_owned(),
        text: json::document(&Value::Object(members)),
        groups: group_count,
        elements,
        targets,
        notes,
    })
}

/// Compare each built document against the file on disk; write nothing.
fn check_against(out: &Path, built: &[Built]) -> Result<String, String> {
    let mut drifted = Vec::new();
    for document in built {
        match std::fs::read_to_string(out.join(&document.file)) {
            Ok(existing) if existing == document.text => {}
            Ok(_) => drifted.push(format!("{} differs", document.file)),
            Err(_) => drifted.push(format!("{} missing", document.file)),
        }
    }
    if drifted.is_empty() {
        return Ok(format!(
            "checked {}: {} document(s) identical",
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
    corpus: &Corpus,
    options: &Options,
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("creating {}: {e}", out.display()))?;
    for document in built {
        write_atomically(&out.join(&document.file), &document.text)?;
    }
    if options.manifest {
        let merged_note = (!options.imports.is_empty()).then(|| {
            format!(
                "merged-reading concepts={} mappings={}",
                corpus.concepts.iter().filter(|c| c.imported).count(),
                corpus.mappings.iter().filter(|m| m.layer_imported).count()
            )
        });
        let rows: Vec<[String; 8]> = built
            .iter()
            .map(|document| {
                let mut notes = document.notes.join(";");
                if let Some(note) = &merged_note {
                    notes.push(';');
                    notes.push_str(note);
                }
                [
                    document.file.clone(),
                    document.collection.clone(),
                    document.groups.to_string(),
                    document.elements.to_string(),
                    document.targets.to_string(),
                    options.filters_cell(),
                    corpus.schema_digest.clone(),
                    notes,
                ]
            })
            .collect();
        let mut text = crate::csv::table(
            &[
                "file",
                "collection",
                "groups",
                "elements",
                "targets",
                "filters",
                "schema_digest",
                "notes",
            ],
            &rows,
        );
        text.push_str(&format!(
            "# entries={};concepts={};mappings={};require-assessed={}\n",
            corpus.entries_read,
            corpus.concepts.len(),
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
    tally: &Tally,
    out: &Path,
    options: &Options,
) -> String {
    let mut text = format!(
        "wrote {} ({} entries, {} mappings; schema digest {}): {} ConceptMap document(s)",
        out.display(),
        corpus.entries_read,
        corpus.mappings.len(),
        corpus.schema_digest,
        built.len(),
    );
    for document in built {
        let notes = (!document.notes.is_empty())
            .then(|| format!(" ({})", document.notes.join("; ")))
            .unwrap_or_default();
        text.push_str(&format!(
            "\n{}\t{} groups\t{} elements\t{} targets{notes}",
            document.file, document.groups, document.elements, document.targets
        ));
    }
    if tally.system > 0 {
        text.push_str(&format!(
            "\nsystem filter: {} mapping(s) outside the selected systems",
            tally.system
        ));
    }
    if options.require_assessed && tally.unassessed > 0 {
        text.push_str(&format!(
            "\nmapping quality gate: {} unassessed mapping(s) excluded (--include-unassessed adds them)",
            tally.unassessed
        ));
    }
    text
}
