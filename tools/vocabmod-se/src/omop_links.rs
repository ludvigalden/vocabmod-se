//! The mapping-keyed tables: CONCEPT_RELATIONSHIP and
//! SOURCE_TO_CONCEPT_MAP from the corpus's mapping assertions, and the
//! RELATIONSHIP definitions the registry authors for them.

use std::collections::{BTreeMap, BTreeSet};

use crate::corpus::Corpus;
use crate::omop_export::Options;
use crate::omop_nodes::{Endpoint, Population};
use crate::omop_registry::{self, OmopRegistry};

/// The kinds that assert a source maps to a target: everything else
/// (related_to, not_related_to, partial_overlap) says less than a map.
const STCM_KINDS: [&str; 3] = [
    "equivalent",
    "source_is_narrower_than_target",
    "source_is_broader_than_target",
];

/// Why mapping assertions were skipped, tallied per table.
pub struct LinkStats {
    pub read: usize,
    pub unassessed: usize,
    pub collection: usize,
    pub system: usize,
    pub unresolved_source: usize,
    /// Unresolved target endpoints, tallied by the target's system so the
    /// plain external vocabularies (SNOMED, ATC, ...) show as skips, not
    /// silent loss.
    pub unresolved_target: BTreeMap<String, usize>,
    pub neither_ours: usize,
    pub source_not_ours: usize,
    pub custom_target: usize,
    pub non_mapping_kind: usize,
    pub deduped: usize,
}

impl LinkStats {
    fn new() -> Self {
        Self {
            read: 0,
            unassessed: 0,
            collection: 0,
            system: 0,
            unresolved_source: 0,
            unresolved_target: BTreeMap::new(),
            neither_ours: 0,
            source_not_ours: 0,
            custom_target: 0,
            non_mapping_kind: 0,
            deduped: 0,
        }
    }

    pub fn total_skipped(&self) -> usize {
        self.unassessed
            + self.collection
            + self.system
            + self.unresolved_source
            + self.unresolved_target.values().sum::<usize>()
            + self.neither_ours
            + self.source_not_ours
            + self.custom_target
            + self.non_mapping_kind
    }

    pub fn notes(&self) -> Vec<String> {
        let mut notes = vec![
            format!("read={}", self.read),
            format!("unassessed={}", self.unassessed),
            format!("collection-filtered={}", self.collection),
            format!("system-filtered={}", self.system),
            format!("unresolved-source={}", self.unresolved_source),
        ];
        for (system, count) in &self.unresolved_target {
            notes.push(format!("unresolved-target.{system}={count}"));
        }
        if self.neither_ours > 0 {
            notes.push(format!("neither-side-ours={}", self.neither_ours));
        }
        if self.source_not_ours > 0 {
            notes.push(format!("source-not-ours={}", self.source_not_ours));
        }
        if self.custom_target > 0 {
            notes.push(format!("custom-target-skipped={}", self.custom_target));
        }
        if self.non_mapping_kind > 0 {
            notes.push(format!("non-mapping-kind={}", self.non_mapping_kind));
        }
        notes.push(format!("deduped={}", self.deduped));
        notes
    }

    /// The shared collection and quality gates; false means skip.
    fn gate(&mut self, mapping: &crate::corpus::MappingData, options: &Options) -> bool {
        self.read += 1;
        if !options.collections.is_empty() && !options.collections.contains(&mapping.collection) {
            self.collection += 1;
            return false;
        }
        if options.require_assessed && !mapping.assessed {
            self.unassessed += 1;
            return false;
        }
        true
    }

    /// The system gate: a row is in scope when any our-side endpoint's
    /// concept belongs to a selected system.
    fn in_system_scope(
        &self,
        endpoints: &[(&Endpoint, &(String, String))],
        population: &Population,
        options: &Options,
    ) -> bool {
        endpoints.iter().any(|(endpoint, _)| match endpoint {
            Endpoint::Ours(id) => population
                .system_of(*id)
                .is_some_and(|system| options.systems.contains(system)),
            _ => false,
        })
    }
}

/// CONCEPT_RELATIONSHIP: mapping assertions where at least one side is
/// one of our concept ids and the other side resolves to an id the tree
/// itself names. Each pair is emitted in both directions, the inverse
/// under the registry's inverse spelling; identical triples dedupe.
pub fn build_concept_relationship(
    corpus: &Corpus,
    registry: &OmopRegistry,
    population: &Population,
    options: &Options,
) -> Result<(Vec<[String; 6]>, LinkStats), String> {
    let mut stats = LinkStats::new();
    let mut rows: BTreeSet<(u64, u64, String)> = BTreeSet::new();
    for mapping in &corpus.mappings {
        if !stats.gate(mapping, options) {
            continue;
        }
        let source = population.resolve(registry, &mapping.source)?;
        let target = population.resolve(registry, &mapping.target)?;
        if matches!(source, Endpoint::Unresolved) {
            stats.unresolved_source += 1;
            continue;
        }
        if matches!(target, Endpoint::Unresolved) {
            *stats
                .unresolved_target
                .entry(mapping.target.0.clone())
                .or_default() += 1;
            continue;
        }
        let (source_id, target_id) = match (&source, &target) {
            (Endpoint::Ours(a), Endpoint::Ours(b)) => (*a, *b),
            (Endpoint::Ours(a), Endpoint::Standard { id, .. })
            | (Endpoint::Standard { id, .. }, Endpoint::Ours(a)) => (*a, *id),
            (Endpoint::Standard { .. }, Endpoint::Standard { .. }) => {
                stats.neither_ours += 1;
                continue;
            }
            (Endpoint::Unresolved, _) | (_, Endpoint::Unresolved) => unreachable!("gated above"),
        };
        if !options.systems.is_empty()
            && !stats.in_system_scope(
                &[(&source, &mapping.source), (&target, &mapping.target)],
                population,
                options,
            )
        {
            stats.system += 1;
            continue;
        }
        let spelling = registry
            .relationship_id
            .get(&mapping.relationship)
            .ok_or_else(|| {
                format!(
                    "mapping relationship \"{}\" has no registry spelling",
                    mapping.relationship
                )
            })?;
        let inverse = omop_registry::inverse_of(registry, spelling)
            .ok_or_else(|| format!("relationship spelling \"{spelling}\" has no inverse"))?;
        let before = rows.len();
        rows.insert((source_id, target_id, spelling.clone()));
        rows.insert((target_id, source_id, inverse.to_owned()));
        if rows.len() == before {
            stats.deduped += 1;
        }
    }
    Ok((
        rows.into_iter()
            .map(|(one, two, relationship)| {
                [
                    one.to_string(),
                    two.to_string(),
                    relationship,
                    registry.validity_start.clone(),
                    registry.validity_end.clone(),
                    String::new(),
                ]
            })
            .collect(),
        stats,
    ))
}

/// SOURCE_TO_CONCEPT_MAP: mapping assertions of a mapping kind whose
/// source is one of our concepts and whose target resolves to a standard
/// OMOP id the tree names itself — a standard endpoint code, or one of
/// our concepts carrying an `OMOP_EXT` binding. Standard ids are never
/// derived from plain external codes.
pub fn build_source_to_concept_map(
    corpus: &Corpus,
    registry: &OmopRegistry,
    population: &Population,
    options: &Options,
) -> Result<(Vec<[String; 11]>, LinkStats), String> {
    let mut stats = LinkStats::new();
    let mut rows: Vec<(u64, u64, String, [String; 11])> = Vec::new();
    let mut seen: BTreeSet<(u64, u64, String)> = BTreeSet::new();
    for mapping in &corpus.mappings {
        if !stats.gate(mapping, options) {
            continue;
        }
        if !STCM_KINDS.contains(&mapping.relationship.as_str()) {
            stats.non_mapping_kind += 1;
            continue;
        }
        let source = population.resolve(registry, &mapping.source)?;
        let target = population.resolve(registry, &mapping.target)?;
        let Endpoint::Ours(source_id) = source else {
            if matches!(source, Endpoint::Unresolved) {
                stats.unresolved_source += 1;
            } else {
                stats.source_not_ours += 1;
            }
            continue;
        };
        let Endpoint::Standard { id: target_id, key } = target else {
            if matches!(target, Endpoint::Unresolved) {
                *stats
                    .unresolved_target
                    .entry(mapping.target.0.clone())
                    .or_default() += 1;
            } else {
                stats.custom_target += 1;
            }
            continue;
        };
        if !options.systems.is_empty()
            && !stats.in_system_scope(&[(&source, &mapping.source)], population, options)
        {
            stats.system += 1;
            continue;
        }
        let source_key = &population.by_id[&source_id];
        let target_vocabulary = registry
            .target_vocabulary
            .get(&mapping.target.0)
            .cloned()
            .unwrap_or_default();
        let (target_code, target_name) = match &key {
            Some(key) => (
                key.1.clone(),
                population.name_of(key).unwrap_or_default().to_owned(),
            ),
            None => (String::new(), String::new()),
        };
        let row = [
            source_key.1.clone(),
            source_id.to_string(),
            registry.vocabulary_id[&source_key.0].clone(),
            population
                .name_of(source_key)
                .unwrap_or_default()
                .to_owned(),
            target_id.to_string(),
            target_vocabulary,
            target_code,
            target_name,
            registry.validity_start.clone(),
            registry.validity_end.clone(),
            String::new(),
        ];
        if seen.insert((source_id, target_id, source_key.1.clone())) {
            rows.push((source_id, target_id, source_key.1.clone(), row));
        } else {
            stats.deduped += 1;
        }
    }
    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    Ok((rows.into_iter().map(|(_, _, _, row)| row).collect(), stats))
}

/// RELATIONSHIP: the definitions and inverse pairs our own spellings
/// carry, exactly as the registry authors them.
pub fn build_relationship(registry: &OmopRegistry) -> Vec<[String; 5]> {
    let mut rows: Vec<[String; 5]> = registry
        .relationships
        .iter()
        .map(|def| {
            [
                def.relationship_id.clone(),
                def.relationship_name.clone(),
                if def.hierarchical { "1" } else { "0" }.to_owned(),
                if def.defines_ancestry { "1" } else { "0" }.to_owned(),
                def.inverse.clone(),
            ]
        })
        .collect();
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    rows
}
