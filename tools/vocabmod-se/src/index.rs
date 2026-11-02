//! The index verb: the deterministic statistical export of the corpus.
//! One per-entry reading projects concepts, designations, mappings, and
//! bindings into sorted TSV tables plus a compact count summary, and
//! every count the committed census covers is cross-checked against it
//! in the same run — a divergence is a finding (exit 1), never a silent
//! drift. No timestamps, no run-dependent ordering: two runs over the
//! same tree produce byte-identical files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use crate::corpus::{self, ConceptData, Corpus, MappingData};
use crate::omop_export::write_atomically;
use crate::{CLEAN, Paths, fail};

pub fn run(
    paths: &Paths,
    out: &Path,
    systems: &[String],
    imports: &[std::path::PathBuf],
) -> ExitCode {
    match export(paths, out, systems, imports) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::from(CLEAN)
        }
        Err(error) => fail(&error),
    }
}

/// The census spells collection ids with their version
/// ("icd-10-snomed-ct@2026.1"); the bound collection field carries the bare
/// id. Both sides normalize to the part before "@".
fn collection_key(id: &str) -> &str {
    id.split('@').next().unwrap_or(id)
}

fn quality_band(mapping: &MappingData) -> String {
    let Some(quality) = &mapping.quality else {
        return "unassessed".to_owned();
    };
    let score: i64 = quality.parse().unwrap_or(-1);
    match score {
        ..=59 => "low (0-59)".to_owned(),
        60..=79 => "mid (60-79)".to_owned(),
        80..=94 => "high (80-94)".to_owned(),
        _ => "top (95-100)".to_owned(),
    }
}

fn cell(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}

fn tsv<const N: usize>(header: &[&str; N], rows: Vec<[String; N]>) -> (String, usize) {
    let mut text = String::from(header.join("\t"));
    text.push('\n');
    for row in &rows {
        let cells: Vec<String> = row.iter().map(|c| cell(c)).collect();
        text.push_str(&cells.join("\t"));
        text.push('\n');
    }
    (text, rows.len())
}

fn export(
    paths: &Paths,
    out: &Path,
    systems: &[String],
    imports: &[std::path::PathBuf],
) -> Result<String, String> {
    let vocab_root = paths.root.join("vocab");
    let corpus = corpus::read(&paths.schema, &paths.root, &vocab_root, imports)?;
    let selected: BTreeSet<&str> = {
        let mut set = BTreeSet::new();
        for system in systems {
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
            set.insert(system.as_str());
        }
        set
    };
    let in_scope = |system: &str| selected.is_empty() || selected.contains(system);

    let by_concept = bindings_by_concept(&corpus);

    // Entity tables, each sorted lexicographically over its columns.
    let mut concepts: Vec<[String; 10]> = Vec::new();
    let mut designations: Vec<[String; 6]> = Vec::new();
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    for concept in &corpus.concepts {
        if !in_scope(&concept.system) {
            continue;
        }
        let omop = by_concept
            .get(&(concept.system.clone(), concept.code.clone()))
            .map(|endpoints| endpoints.join("; "))
            .unwrap_or_default();
        let sv = concept.names.iter().filter(|(l, _, _)| l == "sv").count();
        let en = concept.names.iter().filter(|(l, _, _)| l == "en").count();
        concepts.push([
            concept.system.clone(),
            concept.code.clone(),
            concept.class.clone().unwrap_or_default(),
            concept
                .status
                .clone()
                .unwrap_or_else(|| "active".to_owned()),
            concept.names.len().to_string(),
            concept.synonyms.len().to_string(),
            sv.to_string(),
            en.to_string(),
            concept.parents.len().to_string(),
            omop,
        ]);
        push_designations(&mut designations, concept, "name", &concept.names);
        push_designations(&mut designations, concept, "synonym", &concept.synonyms);
        *totals.entry("names").or_default() += concept.names.len() as u64;
        *totals.entry("synonyms").or_default() += concept.synonyms.len() as u64;
        *totals.entry("parent-links").or_default() += concept.parents.len() as u64;
        *totals.entry("concepts").or_default() += 1;
    }
    concepts.sort();

    let mut mappings: Vec<[String; 12]> = Vec::new();
    for mapping in &corpus.mappings {
        if !in_scope(&mapping.source.0) {
            continue;
        }
        let imported = mapping
            .imported
            .iter()
            .map(|(table, label)| format!("{table}:{label}"))
            .collect::<Vec<_>>()
            .join(";");
        mappings.push([
            mapping.collection.clone(),
            mapping.source.0.clone(),
            mapping.source.1.clone(),
            mapping.target.0.clone(),
            mapping.target.1.clone(),
            mapping.relationship.clone(),
            if mapping.assessed {
                "yes".into()
            } else {
                "no".into()
            },
            mapping.quality.clone().unwrap_or_default(),
            mapping
                .confidence
                .as_ref()
                .map(|(s, _)| s.clone())
                .unwrap_or_default(),
            mapping
                .confidence
                .as_ref()
                .map(|(_, p)| p.clone())
                .unwrap_or_default(),
            mapping.preference.clone().unwrap_or_default(),
            imported,
        ]);
        *totals.entry("mappings").or_default() += 1;
    }
    mappings.sort();

    let mut bindings: Vec<[String; 4]> = Vec::new();
    for binding in &corpus.bindings {
        if !in_scope(&binding.system) {
            continue;
        }
        bindings.push([
            binding.system.clone(),
            binding.code.clone(),
            binding.omop_system.clone(),
            binding.omop_code.clone(),
        ]);
        *totals.entry("bindings").or_default() += 1;
    }
    bindings.sort();

    let summary = summarize(&corpus, &selected, &mut totals);
    let (census_check, mismatches) = cross_check(&corpus, &selected);

    const CONCEPTS_HEADER: [&str; 10] = [
        "system", "code", "class", "status", "names", "synonyms", "sv_names", "en_names",
        "parents", "omop",
    ];
    const DESIGNATIONS_HEADER: [&str; 6] =
        ["system", "code", "kind", "locale", "preferred", "text"];
    const MAPPINGS_HEADER: [&str; 12] = [
        "collection",
        "source_system",
        "source_code",
        "target_system",
        "target_code",
        "relationship",
        "assessed",
        "quality",
        "confidence_score",
        "confidence_scheme",
        "preference",
        "imported",
    ];
    const BINDINGS_HEADER: [&str; 4] = ["system", "code", "omop_system", "omop_code"];
    const SUMMARY_HEADER: [&str; 3] = ["dimension", "key", "count"];
    const CENSUS_HEADER: [&str; 6] = ["scope", "key", "metric", "census", "index", "status"];
    let (concepts_text, concepts_rows) = tsv(&CONCEPTS_HEADER, concepts);
    let (designations_text, designations_rows) = tsv(&DESIGNATIONS_HEADER, designations);
    let (mappings_text, mappings_rows) = tsv(&MAPPINGS_HEADER, mappings);
    let (bindings_text, bindings_rows) = tsv(&BINDINGS_HEADER, bindings);
    let (summary_text, summary_rows) = tsv(&SUMMARY_HEADER, summary);
    let (census_text, census_rows) = tsv(&CENSUS_HEADER, census_check);

    std::fs::create_dir_all(out).map_err(|e| format!("creating {}: {e}", out.display()))?;
    let mut written = Vec::new();
    for (name, text, rows) in [
        ("concepts.tsv", concepts_text, concepts_rows),
        ("designations.tsv", designations_text, designations_rows),
        ("mappings.tsv", mappings_text, mappings_rows),
        ("bindings.tsv", bindings_text, bindings_rows),
        ("summary.tsv", summary_text, summary_rows),
        ("census-check.tsv", census_text, census_rows),
    ] {
        write_atomically(&out.join(name), &text)?;
        written.push(format!("{name}: {rows} row(s)"));
    }
    if !mismatches.is_empty() {
        for mismatch in &mismatches {
            eprintln!("vocabmod-se: census mismatch: {mismatch}");
        }
        return Err(format!(
            "{} census cross-check mismatch(es); the census-check table names each",
            mismatches.len()
        ));
    }
    Ok(format!(
        "wrote {} file(s) to {}; every census cross-check matches",
        written.len(),
        out.display()
    ))
}

fn push_designations(
    rows: &mut Vec<[String; 6]>,
    concept: &ConceptData,
    kind: &str,
    designations: &[(String, String, bool)],
) {
    for (locale, text, preferred) in designations {
        rows.push([
            concept.system.clone(),
            concept.code.clone(),
            kind.to_owned(),
            locale.clone(),
            if *preferred { "1".into() } else { "0".into() },
            text.clone(),
        ]);
    }
}

fn bindings_by_concept(corpus: &Corpus) -> BTreeMap<(String, String), Vec<String>> {
    let mut map: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for binding in &corpus.bindings {
        map.entry((binding.system.clone(), binding.code.clone()))
            .or_default()
            .push(format!("{}:{}", binding.omop_system, binding.omop_code));
    }
    map
}

/// The compact count table: one row per (dimension, key) over the
/// in-scope corpus.
fn summarize(
    corpus: &Corpus,
    selected: &BTreeSet<&str>,
    totals: &mut BTreeMap<&str, u64>,
) -> Vec<[String; 3]> {
    let in_scope = |system: &str| selected.is_empty() || selected.contains(system);
    let mut counts: BTreeMap<(&str, String), u64> = BTreeMap::new();
    let mut bump = |dimension: &'static str, key: String| {
        *counts.entry((dimension, key)).or_default() += 1;
    };
    for concept in &corpus.concepts {
        if !in_scope(&concept.system) {
            continue;
        }
        bump("concept system", concept.system.clone());
        bump(
            "concept class",
            concept.class.clone().unwrap_or_else(|| "(none)".to_owned()),
        );
        bump(
            "concept status",
            concept
                .status
                .clone()
                .unwrap_or_else(|| "active".to_owned()),
        );
        let locales: BTreeSet<&str> = concept.names.iter().map(|(l, _, _)| l.as_str()).collect();
        let availability = if locales.is_empty() {
            "none".to_owned()
        } else if locales.contains("sv") && locales.contains("en") {
            "sv+en".to_owned()
        } else if locales.contains("sv") {
            "sv".to_owned()
        } else if locales.contains("en") {
            "en".to_owned()
        } else {
            "other".to_owned()
        };
        bump("concept designation availability", availability);
        bump(
            "concept synonym availability",
            if concept.synonyms.is_empty() {
                "without-synonyms".to_owned()
            } else {
                "with-synonyms".to_owned()
            },
        );
    }
    for mapping in &corpus.mappings {
        if !in_scope(&mapping.source.0) {
            continue;
        }
        bump(
            "mapping collection",
            collection_key(&mapping.collection).to_owned(),
        );
        bump("mapping kind", mapping.relationship.clone());
        bump(
            "mapping assessment",
            if mapping.assessed {
                "assessed".to_owned()
            } else {
                "unassessed".to_owned()
            },
        );
        bump("mapping quality band", quality_band(mapping));
        bump(
            "mapping confidence scheme",
            mapping
                .confidence
                .as_ref()
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| "(none)".to_owned()),
        );
    }
    for binding in &corpus.bindings {
        if !in_scope(&binding.system) {
            continue;
        }
        bump("binding omop system", binding.omop_system.clone());
        bump("binding concept system", binding.system.clone());
    }
    let mut rows: Vec<[String; 3]> = counts
        .into_iter()
        .map(|((dimension, key), count)| [dimension.to_owned(), key, count.to_string()])
        .collect();
    for (key, count) in totals.iter() {
        rows.push(["total".to_owned(), key.to_string(), count.to_string()]);
    }
    rows.sort();
    rows
}

/// Cross-check every census-covered count against the corpus just read.
/// A filtered run cross-checks the selected systems' rows only; the
/// whole-tree totals are checked unfiltered.
fn cross_check(corpus: &Corpus, selected: &BTreeSet<&str>) -> (Vec<[String; 6]>, Vec<String>) {
    let in_scope = |system: &str| selected.is_empty() || selected.contains(system);
    let mut rows: Vec<[String; 6]> = Vec::new();
    let mut mismatches: Vec<String> = Vec::new();
    let record = |scope: &str,
                  key: &str,
                  metric: &str,
                  census: u64,
                  index: u64,
                  rows: &mut Vec<[String; 6]>,
                  mismatches: &mut Vec<String>| {
        let status = if census == index { "match" } else { "mismatch" };
        if census != index {
            mismatches.push(format!(
                "{scope} {key} {metric}: census={census} index={index}"
            ));
        }
        rows.push([
            scope.to_owned(),
            key.to_owned(),
            metric.to_owned(),
            census.to_string(),
            index.to_string(),
            status.to_owned(),
        ]);
    };

    // Per-system concept statistics.
    let mut system_stats: BTreeMap<&str, [u64; 6]> = BTreeMap::new();
    for concept in corpus.concepts.iter().filter(|c| !c.imported) {
        let stats = system_stats
            .entry(concept.system.as_str())
            .or_insert([0; 6]);
        stats[0] += 1;
        if concept.status.as_deref() == Some("inactive") {
            stats[1] += 1;
        }
        if concept.status.as_deref() == Some("retired") {
            stats[2] += 1;
        }
        if !concept.names.is_empty() {
            stats[3] += 1;
        }
        if concept.names.iter().any(|(l, _, _)| l == "sv") {
            stats[4] += 1;
        }
        if concept.names.iter().any(|(l, _, _)| l == "en") {
            stats[5] += 1;
        }
    }
    for census_system in &corpus.census.systems {
        if !in_scope(&census_system.id) {
            continue;
        }
        let index = system_stats
            .get(census_system.id.as_str())
            .copied()
            .unwrap_or([0; 6]);
        for (metric, (census, counted)) in [
            ("concepts", (census_system.concepts, index[0])),
            ("inactive", (census_system.inactive, index[1])),
            ("retired", (census_system.retired, index[2])),
            ("named", (census_system.named, index[3])),
            ("sv", (census_system.sv, index[4])),
            ("en", (census_system.en, index[5])),
        ] {
            record(
                "system",
                &census_system.id,
                metric,
                census,
                counted,
                &mut rows,
                &mut mismatches,
            );
        }
    }

    // Per-collection mapping statistics, kinds, and tiers.
    let mut collection_stats: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    let mut kind_stats: BTreeMap<(&str, &str), u64> = BTreeMap::new();
    let mut tier_stats: BTreeMap<(&str, &str), u64> = BTreeMap::new();
    for mapping in corpus.mappings.iter().filter(|m| !m.layer_imported) {
        let key = collection_key(&mapping.collection);
        let entry = collection_stats.entry(key).or_default();
        entry.0 += 1;
        if mapping.assessed {
            entry.1 += 1;
        }
        *kind_stats
            .entry((key, mapping.relationship.as_str()))
            .or_default() += 1;
        if let Some(tier) = tier_of(&mapping.confidence) {
            *tier_stats.entry((key, tier)).or_default() += 1;
        }
    }
    for collection in &corpus.census.collections {
        if !in_scope(&collection.source) {
            continue;
        }
        let key = collection_key(&collection.id).to_owned();
        let index = collection_stats
            .get(key.as_str())
            .copied()
            .unwrap_or((0, 0));
        record(
            "collection",
            &key,
            "mappings",
            collection.mappings,
            index.0,
            &mut rows,
            &mut mismatches,
        );
        record(
            "collection",
            &key,
            "assessed",
            collection.assessed,
            index.1,
            &mut rows,
            &mut mismatches,
        );
        for (kind, census) in &collection.kinds {
            let counted = kind_stats
                .get(&(key.as_str(), kind.as_str()))
                .copied()
                .unwrap_or(0);
            record(
                "collection",
                &key,
                &format!("kind:{kind}"),
                *census,
                counted,
                &mut rows,
                &mut mismatches,
            );
        }
        for (tier, census) in &collection.tiers {
            let counted = tier_stats
                .get(&(key.as_str(), tier.as_str()))
                .copied()
                .unwrap_or(0);
            record(
                "collection",
                &key,
                &format!("tier:{tier}"),
                *census,
                counted,
                &mut rows,
                &mut mismatches,
            );
        }
    }

    // Bindings by omop target system: the census attributes them per
    // collection; the corpus attributes by the bound omop endpoint.
    let mut index_bindings: BTreeMap<&str, u64> = BTreeMap::new();
    for binding in corpus.bindings.iter().filter(|b| !b.imported) {
        if !in_scope(&binding.system) {
            continue;
        }
        *index_bindings
            .entry(binding.omop_system.as_str())
            .or_default() += 1;
    }
    let mut census_bindings: BTreeMap<&str, u64> = BTreeMap::new();
    for collection in &corpus.census.collections {
        if !in_scope(&collection.source) {
            continue;
        }
        *census_bindings
            .entry(collection.target.as_str())
            .or_default() += collection.bindings;
    }
    let mut targets: BTreeSet<&str> = index_bindings.keys().copied().collect();
    targets.extend(census_bindings.keys().copied());
    for target in targets {
        record(
            "binding-target",
            target,
            "bindings",
            census_bindings.get(target).copied().unwrap_or(0),
            index_bindings.get(target).copied().unwrap_or(0),
            &mut rows,
            &mut mismatches,
        );
    }

    // Whole-tree totals, unfiltered runs only.
    if selected.is_empty() {
        let census_concepts: u64 = corpus.census.systems.iter().map(|s| s.concepts).sum();
        let census_mappings: u64 = corpus.census.collections.iter().map(|c| c.mappings).sum();
        let census_bindings: u64 = corpus.census.collections.iter().map(|c| c.bindings).sum();
        let committed_concepts = corpus.concepts.iter().filter(|c| !c.imported).count() as u64;
        let committed_mappings =
            corpus.mappings.iter().filter(|m| !m.layer_imported).count() as u64;
        let committed_bindings = corpus.bindings.iter().filter(|b| !b.imported).count() as u64;
        record(
            "total",
            "concepts",
            "concepts",
            census_concepts,
            committed_concepts,
            &mut rows,
            &mut mismatches,
        );
        record(
            "total",
            "mappings",
            "mappings",
            census_mappings,
            committed_mappings,
            &mut rows,
            &mut mismatches,
        );
        record(
            "total",
            "bindings",
            "bindings",
            census_bindings,
            committed_bindings,
            &mut rows,
            &mut mismatches,
        );
    }
    rows.sort();
    mismatches.sort();
    (rows, mismatches)
}

/// A mapping's confidence tier from its bound confidence: under scheme
/// `vocabmod-se-confidence@1` the calibrated tier scores map H (>= 0.87),
/// M (>= 0.71), else L; under `vocabmod-se-confidence-unassessable@1`
/// every reason code is U. A mapping carrying no confidence enters no
/// tier, so the tier rows of a collection sum to at most its mappings.
fn tier_of(confidence: &Option<(String, String)>) -> Option<&'static str> {
    match confidence {
        Some((score, scheme)) if scheme == "vocabmod-se-confidence@1" => {
            if crate::figures_eval::decimal_at_least(score, "0.87") {
                Some("H")
            } else if crate::figures_eval::decimal_at_least(score, "0.71") {
                Some("M")
            } else {
                Some("L")
            }
        }
        Some((_, scheme)) if scheme == "vocabmod-se-confidence-unassessable@1" => Some("U"),
        _ => None,
    }
}
