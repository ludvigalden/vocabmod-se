//! Reading the corpus: one streaming reading per entry document (each
//! system's definitions.kdl plus the registry fragments), the shape proven
//! bounded on this tree. One whole-tree complete-scope reading instead
//! retains link-stage state for every reference and does not fit this
//! host's memory; the tool never asks for it.
//!
//! The reading goes through the schema-generated processor
//! (`vocab-proc`): every item arrives as its generated type, and a
//! reference — written, or fed by a defaults or context slot — as a typed
//! reference carrying its codec's segments and qualifier context.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use vocab_proc::types::{
    Collection, Concept, Designation, EndpointRef, Mapping, Member, Note, OmopBinding, Text,
};
use vocab_proc::{
    CheckScope, Definition, Extensions, Item, ReadOptions, Ref, TypedItem, TypedReader,
};

/// The compact projection of one concept: everything the OMOP tables and
/// the figure computations need, nothing else retained.
#[derive(PartialEq, Clone)]
pub struct ConceptData {
    pub system: String,
    pub code: String,
    pub class: Option<String>,
    /// The class reference's resolved system, so a class selector's
    /// qualified spelling (e.g. "KSI:code") can match it.
    pub class_system: Option<String>,
    pub status: Option<String>,
    /// (system, code) of each declared parent link, in declaration order.
    pub parents: Vec<(String, String)>,
    /// (locale, text, preferred) in declaration order.
    pub names: Vec<(String, String, bool)>,
    pub synonyms: Vec<(String, String, bool)>,
    /// True when the row was read from an imported layer (`--imports`),
    /// never from the committed tree.
    pub imported: bool,
}

/// One omop-binding, its target concept keyed by (system, code) as the
/// binding spelled it.
#[derive(PartialEq, Clone)]
pub struct BindingData {
    pub system: String,
    pub code: String,
    pub omop_system: String,
    pub omop_code: String,
    pub imported: bool,
}

/// One mapping assertion: both endpoints keyed by (system, code) as the
/// reference's scope spelled them.
#[derive(PartialEq, Clone)]
pub struct MappingData {
    pub collection: String,
    pub source: (String, String),
    pub target: (String, String),
    pub relationship: String,
    /// A bound quality rubric value marks an assessed mapping; an absent
    /// one is the schema's own "not assessed".
    pub assessed: bool,
    pub quality: Option<String>,
    pub preference: Option<String>,
    pub method: Option<String>,
    /// (table spelling, label) of each imported marker, in order.
    pub imported: Vec<(String, String)>,
    /// (score, scheme spelling) of the bound confidence, if any.
    pub confidence: Option<(String, String)>,
    /// The texts of the mapping's comments, in declaration order,
    /// excluding every note the corpus marks suspect of quoting a
    /// licensed source.
    pub comments: Vec<String>,
    /// How many of the mapping's comments carry a suspect marker and are
    /// therefore never re-emitted.
    pub comments_withheld: usize,
    /// True when the row was read from an imported layer (`--imports`).
    pub layer_imported: bool,
}

/// One collection declaration: what a ConceptMap document's metadata
/// needs, nothing else retained.
#[derive(Clone)]
pub struct CollectionData {
    pub version: String,
    /// (locale, text) of each title, in declaration order.
    pub titles: Vec<(String, String)>,
    /// The publisher's agent id as its reference spells it.
    pub publisher: Option<String>,
}

pub struct Corpus {
    pub concepts: Vec<ConceptData>,
    pub bindings: Vec<BindingData>,
    pub mappings: Vec<MappingData>,
    /// The committed custom-id-range rows: (id, first, last).
    pub committed_bands: Vec<(String, u64, u64)>,
    /// Every declared system id, for validating selector spellings.
    pub systems: BTreeSet<String>,
    /// The canonical URI of each system that declares one.
    pub system_uris: BTreeMap<String, String>,
    /// Every collection declaration, keyed by collection id.
    pub collections: BTreeMap<String, CollectionData>,
    /// Agent id to the (locale, text) of each declared name.
    pub agents: BTreeMap<String, Vec<(String, String)>>,
    /// Value-set id (as its references spell it) to member endpoints.
    pub value_sets: BTreeMap<String, Vec<(String, String)>>,
    pub entries_read: usize,
    /// The committed census rows, for the index cross-check.
    pub census: crate::census::Census,
    /// The schema digest of the compiled definition, for manifests.
    pub schema_digest: String,
}

/// The entry documents: every definitions.kdl under the vocab tree plus
/// the registry fragments, sorted — the same set the whole-tree vet reads.
pub fn entries(vocab_root: &Path) -> std::io::Result<Vec<PathBuf>> {
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
            } else if path.is_file()
                && path.extension().is_some_and(|ext| ext == "kdl")
                && (path
                    .file_name()
                    .is_some_and(|name| name == "definitions.kdl")
                    || path
                        .parent()
                        .and_then(Path::to_str)
                        .is_some_and(|parent| parent.ends_with("_registry")))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(vocab_root, &mut found)?;
    Ok(found)
}

/// Compile the schema definition every reading of this tool binds under.
pub fn compile(schema: &Path, root: &Path) -> Result<Definition, String> {
    let schema_path = schema.to_path_buf();
    vocab_proc::load_with(
        &[&schema_path],
        &Extensions::standard(),
        CheckScope::Complete,
        read_options(root),
    )
    .map_err(|records| {
        format!(
            "schema {} rejected: {} diagnostic(s), first: {:?}",
            schema.display(),
            records.len(),
            records.first()
        )
    })
}

/// Read every entry through the generated typed reader, projecting
/// concepts, omop bindings, mappings, and committed id bands. Any
/// diagnostic item fails the run, naming it. Entries from the `imports`
/// roots are read after the committed tree and carry `imported = true`;
/// an imported row whose identity already exists with identical content
/// is deduplicated (committed wins), and one with differing content is a
/// finding.
pub fn read(
    schema: &Path,
    root: &Path,
    vocab_root: &Path,
    imports: &[PathBuf],
) -> Result<Corpus, String> {
    let definition = compile(schema, root)?;
    let reader = typed_reader(&definition, root)?;

    let mut corpus = Corpus {
        concepts: Vec::new(),
        bindings: Vec::new(),
        mappings: Vec::new(),
        committed_bands: Vec::new(),
        systems: BTreeSet::new(),
        system_uris: BTreeMap::new(),
        collections: BTreeMap::new(),
        agents: BTreeMap::new(),
        value_sets: BTreeMap::new(),
        entries_read: 0,
        census: Default::default(),
        schema_digest: definition.digest(),
    };
    let mut index: BTreeMap<(String, String), (bool, ConceptData)> = BTreeMap::new();
    let mut binding_index: BTreeMap<(String, String, String, String), (bool, BindingData)> =
        BTreeMap::new();
    let mut mapping_index: BTreeMap<
        (String, String, String, String, String, String),
        (bool, MappingData),
    > = BTreeMap::new();

    let mut layers: Vec<(bool, PathBuf)> = vec![(false, vocab_root.to_path_buf())];
    layers.extend(imports.iter().map(|p| (true, p.clone())));

    for (imported, layer_root) in &layers {
        for entry in
            entries(layer_root).map_err(|e| format!("walking {}: {e}", layer_root.display()))?
        {
            let mut concepts = 0usize;
            let mut bindings = 0usize;
            let mut mappings = 0usize;
            let mut diagnostics: Vec<String> = Vec::new();
            let mut failure: Option<String> = None;
            let mut sink = |item: TypedItem<Item>| {
                let value = match item {
                    TypedItem::Declaration { value, .. } | TypedItem::Record { value, .. } => value,
                    TypedItem::Diagnostic(record) => {
                        if diagnostics.len() < 5 {
                            diagnostics.push(format!(
                                "{} — {}",
                                record.code,
                                record.message.as_deref().unwrap_or("")
                            ));
                        }
                        return;
                    }
                    _ => return,
                };
                let projected = match value {
                    Item::Concept(concept) => {
                        concepts += 1;
                        project_concept(&mut corpus, &mut index, &concept, *imported)
                    }
                    Item::OmopBinding(binding) => {
                        bindings += 1;
                        project_binding(&mut corpus, &mut binding_index, &binding, *imported)
                    }
                    Item::Mapping(mapping) => {
                        mappings += 1;
                        project_mapping(&mut corpus, &mut mapping_index, &mapping, *imported)
                    }
                    Item::CustomIdRange(band) => {
                        corpus
                            .committed_bands
                            .push((band.id.to_string(), band.first, band.last));
                        Ok(())
                    }
                    Item::System(system) => {
                        corpus.systems.insert(system.id.to_string());
                        if let Some(uri) = &system.canonical_uri {
                            corpus
                                .system_uris
                                .insert(system.id.to_string(), uri.to_string());
                        }
                        Ok(())
                    }
                    Item::Collection(collection) => {
                        project_collection(&mut corpus, &collection);
                        Ok(())
                    }
                    Item::Agent(agent) => {
                        corpus
                            .agents
                            .insert(agent.id.to_string(), texts(&agent.name));
                        Ok(())
                    }
                    Item::ValueSet(set) => {
                        corpus.value_sets.entry(set.id.to_string()).or_default();
                        Ok(())
                    }
                    Item::Member(member) => {
                        project_member(&mut corpus, &member);
                        Ok(())
                    }
                    Item::Census(census) => {
                        if !imported {
                            crate::census::project(&mut corpus.census, &census);
                        }
                        Ok(())
                    }
                    _ => Ok(()),
                };
                if let Err(error) = projected {
                    failure.get_or_insert(error);
                }
            };
            let failed = reader
                .read(&[&entry], &mut sink)
                .map_err(|error| format!("reading {}: {error}", entry.display()))?;
            if let Some(error) = failure {
                return Err(error);
            }
            if !diagnostics.is_empty() {
                return Err(format!(
                    "entry {} reported diagnostics: {}",
                    entry.display(),
                    diagnostics.join("; ")
                ));
            }
            if failed {
                return Err(format!(
                    "entry {} reported error diagnostics without surfacing them",
                    entry.display()
                ));
            }
            corpus.entries_read += 1;
            eprintln!(
                "read {}: {concepts} concepts, {bindings} bindings, {mappings} mappings",
                entry.display()
            );
        }
    }
    Ok(corpus)
}

/// The generated typed reader over `definition`, reading under the same
/// root and invocation bound every reading of this tool uses.
pub fn typed_reader<'d>(
    definition: &'d Definition,
    root: &Path,
) -> Result<TypedReader<'d, Item>, String> {
    let reader = vocab_proc::reader(definition).map_err(|records| {
        format!(
            "the generated processor disagrees with the schema: {} diagnostic(s), first: {:?}",
            records.len(),
            records.first()
        )
    })?;
    Ok(reader.with_options(read_options(root)))
}

/// The options every reading of this tool binds under.
pub fn read_options(root: &Path) -> ReadOptions {
    ReadOptions::new()
        .with_root(root)
        .with_max_invocations(40000)
}

type Project = Result<(), String>;

fn project_concept(
    corpus: &mut Corpus,
    index: &mut BTreeMap<(String, String), (bool, ConceptData)>,
    concept: &Concept,
    imported: bool,
) -> Project {
    let data = ConceptData {
        system: concept.system.to_string(),
        code: concept.code.to_string(),
        // The class as its reference spelled it, written or fed by the
        // defaults' concept-class slot alike, and the system its
        // reference resolves under.
        class: concept
            .class
            .as_ref()
            .map(|class| class.written().to_owned()),
        class_system: concept
            .class
            .as_ref()
            .and_then(|class| endpoint_of(class))
            .map(|(system, _)| system),
        status: concept.status.map(|status| status.member().to_owned()),
        parents: concept
            .parents
            .iter()
            .filter_map(|p| endpoint_of(p))
            .collect(),
        names: designations(&concept.names),
        synonyms: designations(&concept.synonyms),
        imported,
    };
    let key = (data.system.clone(), data.code.clone());
    if let Some((other_layer, existing)) = index.get(&key) {
        // Layer precedence: an imported row matching a committed row
        // with identical content is deduplicated (committed wins);
        // differing content is a finding. Within one layer a duplicate
        // identity is a finding.
        if *other_layer == imported {
            return Err(format!(
                "duplicate concept identity {}.{}",
                data.system, data.code
            ));
        }
        if existing.same_content(&data) {
            return Ok(());
        }
        return Err(format!(
            "imported concept {}.{}, already committed with differing content \
             (committed class {:?}, status {:?}; imported class {:?}, status {:?})",
            existing.system,
            existing.code,
            existing.class,
            existing.status,
            data.class,
            data.status
        ));
    }
    index.insert(key, (imported, data.clone()));
    corpus.concepts.push(data);
    Ok(())
}

impl ConceptData {
    /// Content equality across layers: everything but the layer flag.
    fn same_content(&self, other: &Self) -> bool {
        self.system == other.system
            && self.code == other.code
            && self.class == other.class
            && self.class_system == other.class_system
            && self.status == other.status
            && self.parents == other.parents
            && self.names == other.names
            && self.synonyms == other.synonyms
    }
}

impl MappingData {
    /// Content equality across layers: everything but the layer flag.
    fn same_content(&self, other: &Self) -> bool {
        self.collection == other.collection
            && self.source == other.source
            && self.target == other.target
            && self.relationship == other.relationship
            && self.assessed == other.assessed
            && self.quality == other.quality
            && self.preference == other.preference
            && self.method == other.method
            && self.imported == other.imported
            && self.confidence == other.confidence
            && self.comments == other.comments
            && self.comments_withheld == other.comments_withheld
    }
}

impl BindingData {
    /// Content equality across layers: everything but the layer flag.
    fn same_content(&self, other: &Self) -> bool {
        self.system == other.system
            && self.code == other.code
            && self.omop_system == other.omop_system
            && self.omop_code == other.omop_code
    }
}

fn project_mapping(
    corpus: &mut Corpus,
    index: &mut BTreeMap<(String, String, String, String, String, String), (bool, MappingData)>,
    mapping: &Mapping,
    imported: bool,
) -> Project {
    let endpoint = |name: &str, reference: &EndpointRef| {
        endpoint_of(reference).ok_or_else(|| format!("a mapping binds no {name} endpoint"))
    };
    let source = endpoint("source", &mapping.source)?;
    let target = endpoint("target", &mapping.target)?;
    let quality = mapping.quality.map(|quality| quality.to_string());
    let data = MappingData {
        collection: mapping.collection.to_string(),
        source,
        target,
        relationship: mapping.relationship.member().to_owned(),
        assessed: quality.is_some(),
        quality,
        preference: mapping.preference.map(|p| p.member().to_owned()),
        method: mapping.assessment_method.clone(),
        imported: mapping
            .imported
            .iter()
            .map(|marker| (marker.table.written().to_owned(), marker.label.clone()))
            .collect(),
        confidence: mapping.confidence.as_ref().map(|confidence| {
            (
                confidence.score.as_str().to_owned(),
                confidence.scheme.written().to_owned(),
            )
        }),
        comments: publishable(&mapping.comments),
        comments_withheld: mapping
            .comments
            .iter()
            .filter(|note| note.suspect.is_some())
            .count(),
        layer_imported: imported,
    };
    let key = (
        data.collection.clone(),
        data.source.0.clone(),
        data.source.1.clone(),
        data.target.0.clone(),
        data.target.1.clone(),
        data.relationship.clone(),
    );
    if let Some((other_layer, existing)) = index.get(&key) {
        if *other_layer != imported {
            if existing.same_content(&data) {
                return Ok(()); // identical across layers: committed wins
            }
            return Err(format!(
                "imported mapping {} {}:{} -> {}:{}, already committed with differing content",
                existing.collection,
                existing.source.0,
                existing.source.1,
                existing.target.0,
                existing.target.1
            ));
        }
        // Within one layer, repeated endpoints are separate assertions.
    }
    index.insert(key, (imported, data.clone()));
    corpus.mappings.push(data);
    Ok(())
}

/// One collection declaration; the first declaration of an id wins, the
/// reader having already rejected a duplicate identity.
fn project_collection(corpus: &mut Corpus, collection: &Collection) {
    corpus
        .collections
        .entry(collection.id.to_string())
        .or_insert_with(|| CollectionData {
            version: collection.version.to_string(),
            titles: texts(&collection.title),
            publisher: collection
                .publisher
                .as_ref()
                .map(|publisher| publisher.written().to_owned()),
        });
}

/// The comment texts a mapping may re-emit: every note without a
/// suspect marker, in declaration order.
fn publishable(notes: &[Note]) -> Vec<String> {
    notes
        .iter()
        .filter(|note| note.suspect.is_none())
        .map(|note| note.text.clone())
        .collect()
}

/// Localized texts as (locale, text), in declaration order.
fn texts(items: &[Text]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|item| (item.locale.to_string(), item.text.clone()))
        .collect()
}

/// One value-set member record: the set as its reference spells it, the
/// concept as an endpoint.
fn project_member(corpus: &mut Corpus, member: &Member) {
    let Some(concept) = endpoint_of(&member.concept) else {
        return;
    };
    corpus
        .value_sets
        .entry(member.set.written().to_owned())
        .or_default()
        .push(concept);
}

fn project_binding(
    corpus: &mut Corpus,
    index: &mut BTreeMap<(String, String, String, String), (bool, BindingData)>,
    binding: &OmopBinding,
    imported: bool,
) -> Project {
    let (Some((system, code)), Some((omop_system, omop_code))) =
        (endpoint_of(&binding.concept), endpoint_of(&binding.omop))
    else {
        return Err("an omop-binding names no concept or omop endpoint".into());
    };
    let data = BindingData {
        system,
        code,
        omop_system,
        omop_code,
        imported,
    };
    let key = (
        data.system.clone(),
        data.code.clone(),
        data.omop_system.clone(),
        data.omop_code.clone(),
    );
    if let Some((other_layer, existing)) = index.get(&key) {
        if *other_layer != imported {
            if existing.same_content(&data) {
                return Ok(()); // identical across layers: committed wins
            }
            return Err(format!(
                "imported omop-binding {}:{} -> {}:{}, already committed with differing content",
                existing.system, existing.code, existing.omop_system, existing.omop_code
            ));
        }
    }
    index.insert(key, (imported, data.clone()));
    corpus.bindings.push(data);
    Ok(())
}

/// The (system, code) a concept endpoint names: the system from an
/// explicit qualifier, else the leading name segment of a qualified
/// spelling, else the reference's own context value; the code is the last
/// segment.
pub(crate) fn endpoint_of<T>(reference: &Ref<T>) -> Option<(String, String)> {
    let name = reference.name();
    let code = name.last()?.clone();
    let system = reference
        .qualifiers()
        .iter()
        .find(|(name, _)| name == "system")
        .map(|(_, value)| value.clone())
        .or_else(|| (name.len() > 1).then(|| name[0].clone()))
        .or_else(|| reference.context().map(str::to_owned))?;
    Some((system, code))
}

/// Designations as (locale, text, preferred), in declaration order.
fn designations(items: &[Designation]) -> Vec<(String, String, bool)> {
    items
        .iter()
        .map(|item| {
            (
                item.locale.to_string(),
                item.text.clone(),
                item.preferred == Some(true),
            )
        })
        .collect()
}
