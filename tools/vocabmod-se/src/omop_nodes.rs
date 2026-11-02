//! The concept-keyed tables: CONCEPT (the population every other table's
//! ids come from), CONCEPT_SYNONYM, CONCEPT_ANCESTOR, and CONCEPT_CLASS.

use std::collections::{BTreeMap, BTreeSet};

use crate::corpus::{ConceptData, Corpus};
use crate::idalloc;
use crate::omop_export::Options;
use crate::omop_registry::OmopRegistry;

/// One concept's export identity, shared by every table so cross-table
/// id consistency holds by construction.
pub struct Population {
    /// Our emitted concepts: key -> concept id (committed or generated).
    pub ids: BTreeMap<(String, String), u64>,
    /// `OMOP_EXT`-bound concepts: key -> Athena's own id for the concept.
    pub ext: BTreeMap<(String, String), u64>,
    /// Reverse of `ids`: id -> key.
    pub by_id: BTreeMap<u64, (String, String)>,
    /// Every tree concept in allocation order (system, code), ext-bound
    /// ones carrying no id of ours, with its export name.
    pub order: Vec<ExportConcept>,
    /// key -> index into `order`.
    pub index: BTreeMap<(String, String), usize>,
}

pub struct ExportConcept {
    pub key: (String, String),
    /// None for ext-bound concepts: Athena's own, outside our population.
    pub id: Option<u64>,
    pub name: String,
    /// The declared parent keys, in declaration order.
    pub parents: Vec<(String, String)>,
    /// Index into the corpus's concept vector, for designation access.
    pub source: usize,
}

/// How one mapping or binding endpoint resolves against the population.
pub enum Endpoint {
    Ours(u64),
    Standard {
        id: u64,
        key: Option<(String, String)>,
    },
    Unresolved,
}

impl Population {
    /// Resolve one (system, code) endpoint. An OMOP_* system outside the
    /// registry's decisions is a hard error, not a guess.
    pub fn resolve(
        &self,
        registry: &OmopRegistry,
        endpoint: &(String, String),
    ) -> Result<Endpoint, String> {
        let (system, code) = endpoint;
        if system == "OMOP_CUSTOM" {
            let id: u64 = code
                .parse()
                .map_err(|_| format!("OMOP_CUSTOM endpoint {code} is not numeric"))?;
            return Ok(if self.by_id.contains_key(&id) {
                Endpoint::Ours(id)
            } else {
                Endpoint::Unresolved
            });
        }
        if registry.target_vocabulary.contains_key(system) {
            let id: u64 = code
                .parse()
                .map_err(|_| format!("{system} endpoint {code} is not a numeric OMOP id"))?;
            return Ok(Endpoint::Standard { id, key: None });
        }
        if system.starts_with("OMOP_") {
            return Err(format!(
                "endpoint system {system} is neither OMOP_CUSTOM nor a standard \
                 endpoint the registry maps"
            ));
        }
        let key = (system.clone(), code.clone());
        if let Some(id) = self.ext.get(&key) {
            return Ok(Endpoint::Standard {
                id: *id,
                key: Some(key),
            });
        }
        Ok(match self.ids.get(&key) {
            Some(id) => Endpoint::Ours(*id),
            None => Endpoint::Unresolved,
        })
    }

    /// The system of the tree concept an our-side id names.
    pub fn system_of(&self, id: u64) -> Option<&str> {
        self.by_id.get(&id).map(|(system, _)| system.as_str())
    }

    /// The export name of the tree concept a key names, if it is one.
    pub fn name_of(&self, key: &(String, String)) -> Option<&str> {
        self.index
            .get(key)
            .map(|&slot| self.order[slot].name.as_str())
    }
}

pub struct ConceptStats {
    pub committed: usize,
    pub generated: usize,
    pub ext_skipped: usize,
    pub name_fallbacks: usize,
    pub status_unbound: usize,
}

/// Build the CONCEPT population and its rows: committed `OMOP_CUSTOM` ids
/// stay, id-less concepts receive a deterministic band id, `OMOP_EXT`-bound
/// concepts are skipped (Athena's own ids), allocation order fixed at
/// system then code.
pub fn build_concept(
    corpus: &Corpus,
    registry: &OmopRegistry,
    options: &Options,
) -> Result<(Population, Vec<[String; 10]>, ConceptStats), String> {
    // Every concept's system must carry an authored spelling and band;
    // a system missing from either is a decision to make, not a guess.
    let unmapped: BTreeSet<&str> = corpus
        .concepts
        .iter()
        .map(|concept| concept.system.as_str())
        .filter(|system| {
            !registry.vocabulary_id.contains_key(*system)
                || !registry.domain_id.contains_key(*system)
                || !registry.id_bands.contains_key(*system)
        })
        .collect();
    if let Some(system) = unmapped.first() {
        return Err(format!("system {system} has no constants-registry entry"));
    }

    // Bindings by target concept: ours (`OMOP_CUSTOM`) fix the id; external
    // (`OMOP_EXT`) concepts are Athena's own and excluded from the export.
    let mut custom: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut ext: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut unassociated = 0usize;
    for binding in &corpus.bindings {
        let key = (binding.system.clone(), binding.code.clone());
        match binding.omop_system.as_str() {
            "OMOP_CUSTOM" => {
                let id: u64 = binding.omop_code.parse().map_err(|_| {
                    format!("OMOP_CUSTOM binding {} is not numeric", binding.omop_code)
                })?;
                if custom.insert(key, id).is_some() {
                    return Err(format!(
                        "concept {}.{} carries two OMOP_CUSTOM ids",
                        binding.system, binding.code
                    ));
                }
            }
            "OMOP_EXT" => {
                let id: u64 = binding.omop_code.parse().map_err(|_| {
                    format!("OMOP_EXT binding {} is not numeric", binding.omop_code)
                })?;
                ext.insert(key, id);
            }
            other => {
                return Err(format!(
                    "omop-binding with unexpected target system \"{other}\""
                ));
            }
        }
        if binding.system.is_empty() || binding.code.is_empty() {
            unassociated += 1;
        }
    }
    if unassociated > 0 {
        return Err(format!("{unassociated} omop-binding(s) name no concept"));
    }

    // The committed bands carry the containment check the schema assigns
    // to this tool: every committed custom id sits inside one of them.
    for ((system, code), id) in &custom {
        if !corpus
            .committed_bands
            .iter()
            .any(|&(_, first, last)| *id >= first && *id <= last)
        {
            return Err(format!(
                "custom id {id} of {system}.{code} lies outside every declared band"
            ));
        }
    }

    let mut sorted: Vec<(usize, &ConceptData)> = corpus.concepts.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| (&a.system, &a.code).cmp(&(&b.system, &b.code)));

    let mut taken: std::collections::HashSet<u64> = custom.values().copied().collect();
    let mut population = Population {
        ids: custom.clone(),
        ext,
        by_id: BTreeMap::new(),
        order: Vec::with_capacity(sorted.len()),
        index: BTreeMap::new(),
    };
    let mut rows: Vec<[String; 10]> = Vec::new();
    let mut stats = ConceptStats {
        committed: custom.len(),
        generated: 0,
        ext_skipped: 0,
        name_fallbacks: 0,
        status_unbound: 0,
    };

    for (source, concept) in sorted {
        let key = (concept.system.clone(), concept.code.clone());
        let id = if let Some(id) = population.ids.get(&key) {
            Some(*id)
        } else if population.ext.contains_key(&key) {
            stats.ext_skipped += 1;
            None
        } else {
            let band = registry.id_bands[&concept.system];
            let id = idalloc::stable_id(band, &concept.system, &concept.code, &mut taken);
            population.ids.insert(key.clone(), id);
            stats.generated += 1;
            Some(id)
        };
        let class = match &concept.class {
            Some(class) => class.clone(),
            None => {
                return Err(format!(
                    "concept {}.{} binds no class",
                    concept.system, concept.code
                ));
            }
        };
        // Status is schema-optional (files without a defaults block);
        // an unset status is the schema's own `unknown`: no invalid_reason.
        let status = concept.status.as_deref().unwrap_or("unknown");
        let invalid_reason = registry
            .invalid_reason
            .get(status)
            .map(String::as_str)
            .ok_or_else(|| format!("status {status} has no invalid_reason mapping"))?;
        if concept.status.is_none() {
            stats.status_unbound += 1;
        }
        let name = pick_name(concept).unwrap_or_else(|| {
            stats.name_fallbacks += 1;
            concept.code.clone()
        });
        population.index.insert(key.clone(), population.order.len());
        if let Some(id) = id {
            population.by_id.insert(id, key.clone());
            if options.systems.is_empty() || options.systems.contains(&concept.system) {
                rows.push([
                    id.to_string(),
                    name.clone(),
                    registry.domain_id[&concept.system].clone(),
                    registry.vocabulary_id[&concept.system].clone(),
                    class,
                    concept.code.clone(),
                    String::new(),
                    registry.validity_start.clone(),
                    registry.validity_end.clone(),
                    invalid_reason.to_owned(),
                ]);
            }
        }
        population.order.push(ExportConcept {
            key,
            id,
            name,
            parents: concept.parents.clone(),
            source,
        });
    }
    rows.sort_by_key(|row| row[0].parse::<u64>().expect("ids are numeric"));
    Ok((population, rows, stats))
}

pub struct SynonymStats {
    pub locale_skips: BTreeMap<String, usize>,
    pub name_elided: usize,
    pub deduped: usize,
}

impl SynonymStats {
    pub fn total_skipped(&self) -> usize {
        self.locale_skips.values().sum::<usize>() + self.name_elided
    }

    pub fn notes(&self) -> Vec<String> {
        let mut notes: Vec<String> = self
            .locale_skips
            .iter()
            .map(|(locale, count)| format!("locale-unmapped.{locale}={count}"))
            .collect();
        notes.push(format!("name-elided={}", self.name_elided));
        notes.push(format!("deduped={}", self.deduped));
        notes
    }
}

/// CONCEPT_SYNONYM: every designation of a population concept except the
/// text already serving as its concept_name; designations in a locale
/// without a confirmed OMOP language id are skipped, counted per locale.
pub fn build_concept_synonym(
    corpus: &Corpus,
    registry: &OmopRegistry,
    population: &Population,
    options: &Options,
) -> (Vec<[String; 3]>, SynonymStats) {
    let mut stats = SynonymStats {
        locale_skips: BTreeMap::new(),
        name_elided: 0,
        deduped: 0,
    };
    let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut rows: Vec<(u64, String, String)> = Vec::new();
    for concept in &population.order {
        let Some(id) = concept.id else { continue };
        if !options.systems.is_empty() && !options.systems.contains(&concept.key.0) {
            continue;
        }
        let source = &corpus.concepts[concept.source];
        for (locale, text, _) in source.names.iter().chain(source.synonyms.iter()) {
            if *text == concept.name {
                stats.name_elided += 1;
                continue;
            }
            let Some(language) = registry.language_concept_id.get(locale) else {
                *stats.locale_skips.entry(locale.clone()).or_default() += 1;
                continue;
            };
            if seen.insert((id.to_string(), text.clone(), language.clone())) {
                rows.push((id, text.clone(), language.clone()));
            } else {
                stats.deduped += 1;
            }
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    (
        rows.into_iter()
            .map(|(id, text, language)| [id.to_string(), text, language])
            .collect(),
        stats,
    )
}

pub struct AncestorStats {
    pub cut_links: usize,
}

impl AncestorStats {
    pub fn total_skipped(&self) -> usize {
        self.cut_links
    }

    pub fn notes(&self) -> Vec<String> {
        vec![format!("cut-links={}", self.cut_links)]
    }
}

/// CONCEPT_ANCESTOR: the transitive closure of declared parent links
/// between population concepts, min/max levels of separation. Parentless
/// concepts are roots and emit no rows; a link whose parent is not in
/// the population (filtered out or ext-bound) is cut, counted.
pub fn build_concept_ancestor(
    population: &Population,
    options: &Options,
) -> Result<(Vec<[String; 4]>, AncestorStats), String> {
    let mut stats = AncestorStats { cut_links: 0 };
    let in_scope =
        |key: &(String, String)| population.ids.contains_key(key) && system_in_scope(key, options);
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); population.order.len()];
    let mut pending = vec![0usize; population.order.len()];
    for (slot, concept) in population.order.iter().enumerate() {
        for parent_key in &concept.parents {
            if !in_scope(parent_key) {
                stats.cut_links += 1;
                continue;
            }
            let parent_slot = population.index[parent_key];
            parents[slot].push(parent_slot);
            pending[slot] += 1;
        }
    }

    // Kahn's order over child->parent edges: a slot is ready once every
    // parent's closure is computed; anything left over is a cycle.
    let mut closures: Vec<BTreeMap<usize, (u32, u32)>> = vec![BTreeMap::new(); parents.len()];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); parents.len()];
    for (slot, slots) in parents.iter().enumerate() {
        for parent in slots {
            children[*parent].push(slot);
        }
    }
    let mut ready: Vec<usize> = (0..parents.len())
        .filter(|slot| pending[*slot] == 0)
        .collect();
    let mut processed = 0usize;
    while let Some(slot) = ready.pop() {
        processed += 1;
        let mut extended: BTreeMap<usize, (u32, u32)> = closures[slot]
            .iter()
            .map(|(ancestor, (min, max))| (*ancestor, (min + 1, max + 1)))
            .collect();
        extended.insert(slot, (1, 1));
        for child in &children[slot] {
            let child_closure = &mut closures[*child];
            for (ancestor, levels) in &extended {
                child_closure
                    .entry(*ancestor)
                    .and_modify(|(min, max)| {
                        *min = (*min).min(levels.0);
                        *max = (*max).max(levels.1);
                    })
                    .or_insert(*levels);
            }
            pending[*child] -= 1;
            if pending[*child] == 0 {
                ready.push(*child);
            }
        }
    }
    if processed < parents.len() {
        let stuck = (0..parents.len())
            .find(|slot| pending[*slot] > 0)
            .expect("unprocessed slots have pending parents");
        let key = &population.order[stuck].key;
        return Err(format!(
            "parent links form a cycle through {}.{}",
            key.0, key.1
        ));
    }

    let mut rows: Vec<(u64, u64, u32, u32)> = Vec::new();
    for (slot, closure) in closures.iter().enumerate() {
        let Some(descendant) = population.order[slot].id else {
            continue;
        };
        for (ancestor, (min, max)) in closure {
            let ancestor_id = population.order[*ancestor]
                .id
                .expect("closure holds population concepts only");
            rows.push((ancestor_id, descendant, *min, *max));
        }
    }
    rows.sort();
    Ok((
        rows.into_iter()
            .map(|(ancestor, descendant, min, max)| {
                [
                    ancestor.to_string(),
                    descendant.to_string(),
                    min.to_string(),
                    max.to_string(),
                ]
            })
            .collect(),
        stats,
    ))
}

fn system_in_scope(key: &(String, String), options: &Options) -> bool {
    options.systems.is_empty() || options.systems.contains(&key.0)
}

/// CONCEPT_CLASS: one row per class spelling the CONCEPT rows carry. The
/// name is the spelling itself; the metadata concept id column stays
/// empty — those ids are Athena's, and the tree names none.
pub fn build_concept_class(concept_rows: &[[String; 10]]) -> Vec<[String; 3]> {
    let classes: BTreeSet<&str> = concept_rows.iter().map(|row| row[4].as_str()).collect();
    classes
        .into_iter()
        .map(|class| [class.to_owned(), class.to_owned(), String::new()])
        .collect()
}

/// The concept's own name: the preferred designation first, Swedish before
/// other locales, names before synonyms; declaration order breaks ties.
pub(crate) fn pick_name(concept: &ConceptData) -> Option<String> {
    let better = |a: &(String, String, bool), b: &(String, String, bool)| {
        b.2.cmp(&a.2)
            .then_with(|| (b.0 == "sv").cmp(&(a.0 == "sv")))
            .then_with(|| a.0.cmp(&b.0))
    };
    concept
        .names
        .iter()
        .chain(concept.synonyms.iter())
        .min_by(|a, b| better(a, b))
        .map(|(_, text, _)| text.clone())
}
