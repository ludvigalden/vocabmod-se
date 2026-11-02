//! Figure computation: every number of the figure data spec computed from
//! the corpus. A number whose input is a local dataset the host does not
//! carry is skipped under a named missing-input note; nothing is guessed.
//! The coverage rule resolves through the standardness resolver here, fed
//! by the local Athena release, so the descriptor's coverage figure is
//! reproducible from tree + Athena like the rest.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufRead;
use std::path::Path;

use crate::figures_eval::{Claim, Eval, LocalInputs, Usage, share, usage_of};
use crate::figures_model::Figure;

/// How a claim target spelled in the tree resolves in Athena: by code in
/// one vocabulary, by code or as a concept id, or only as a concept id.
/// Target systems absent from this table resolve outside the release.
enum Lookup {
    Code(&'static str),
    CodeOrId(&'static str),
    Id,
}

/// The tree's target systems the local Athena release carries, with the
/// lookup each resolves under (UCUM, EDQM, and Orphanet resolve by code
/// where the release carries the vocabulary; the OMOP id endpoints and
/// OSM resolve by id).
fn target_lookup(system: &str) -> Option<Lookup> {
    Some(match system {
        "SNOMED_CT" | "SNOMED_CT_SE_EXT" => Lookup::Code("SNOMED"),
        "LOINC" => Lookup::CodeOrId("LOINC"),
        "RXNORM" => Lookup::CodeOrId("RxNorm"),
        "ATC" => Lookup::Code("ATC"),
        "ICD_10_CM" => Lookup::Code("ICD10CM"),
        "UCUM" => Lookup::Code("UCUM"),
        "EDQM" => Lookup::Code("EDQM"),
        "ORPHANET" => Lookup::Code("Orphanet"),
        "OMOP_SNOMED_CT" | "OMOP_ICD_10_CM" | "OMOP_EXT" | "OSM" => Lookup::Id,
        _ => return None,
    })
}

/// One resolved Athena concept: id, whether OMOP-standard, and whether
/// the release retires it.
struct Concept {
    id: String,
    vocabulary: String,
    standard: bool,
    invalid: bool,
}

/// The standardness resolver: whether a claim's target resolves in
/// the local Athena release to an OMOP-standard concept, by id or by
/// code, directly or through one valid `Maps to`. Loaded once per run
/// from the release's CONCEPT.csv and
/// CONCEPT_RELATIONSHIP.csv, restricted to the claim targets the corpus
/// carries.
pub struct Standardness {
    by_code: HashMap<(String, String), Concept>,
    by_id: HashMap<String, Concept>,
    /// Valid `Maps to` edges out of the resolved non-standard concepts.
    mapsto: HashMap<String, Vec<String>>,
    /// Standardness of every `Maps to` target.
    targets: HashMap<String, (bool, bool)>,
}

impl Standardness {
    /// Load the resolver from the release directory (the one holding
    /// CONCEPT.csv) for the corpus's claim targets. `Ok(None)` when the
    /// directory carries no CONCEPT.csv — the missing-input case, never
    /// an error.
    pub fn load(dir: &Path, targets: &BTreeSet<(String, String)>) -> Result<Option<Self>, String> {
        let concepts_csv = dir.join("CONCEPT.csv");
        if !concepts_csv.is_file() {
            return Ok(None);
        }
        // Requests by lookup shape: codes per Athena vocabulary, ids.
        let mut codes: HashSet<(&str, &str)> = HashSet::new();
        let mut ids: HashSet<&str> = HashSet::new();
        for (system, code) in targets {
            let Some(lookup) = target_lookup(system) else {
                continue;
            };
            match lookup {
                Lookup::Code(vocab) | Lookup::CodeOrId(vocab) => {
                    codes.insert((vocab, code.as_str()));
                }
                Lookup::Id => {}
            }
            if matches!(lookup, Lookup::CodeOrId(_) | Lookup::Id)
                && code.chars().all(|c| c.is_ascii_digit())
            {
                ids.insert(code.as_str());
            }
        }
        let mut by_code: HashMap<(String, String), Concept> = HashMap::new();
        let mut by_id: HashMap<String, Concept> = HashMap::new();
        scan_concepts(&concepts_csv, &codes, &ids, &mut by_code, &mut by_id)?;
        // One valid `Maps to` hop out of the resolved non-standard
        // concepts, then the standardness of those targets.
        let nonstandard: HashSet<&str> = by_code
            .values()
            .chain(by_id.values())
            .filter(|c| !c.standard)
            .map(|c| c.id.as_str())
            .collect();
        let mapsto = scan_mapsto(&dir.join("CONCEPT_RELATIONSHIP.csv"), &nonstandard)?;
        let mut target_ids: BTreeSet<&str> = BTreeSet::new();
        for list in mapsto.values() {
            target_ids.extend(list.iter().map(String::as_str));
        }
        let targets = scan_target_concepts(&concepts_csv, &target_ids)?;
        Ok(Some(Self {
            by_code,
            by_id,
            mapsto,
            targets,
        }))
    }


    /// Whether one claim target (system spelling, code or id) resolves
    /// to an OMOP-standard concept directly or through one valid
    /// `Maps to`.
    pub fn covered(&self, system: &str, code: &str) -> bool {
        let Some(lookup) = target_lookup(system) else {
            return false;
        };
        let entry = 'entry: {
            if let Lookup::Code(vocab) | Lookup::CodeOrId(vocab) = lookup
                && let Some(concept) = self.by_code.get(&(vocab.to_owned(), code.to_owned()))
            {
                break 'entry Some(concept);
            }
            if let Lookup::CodeOrId(_) | Lookup::Id = lookup {
                let matched = self.by_id.get(code).filter(|concept| match lookup {
                    // An id match must land in the vocabulary the
                    // target system resolves against.
                    Lookup::CodeOrId(vocab) => concept.vocabulary == vocab,
                    _ => true,
                });
                if matched.is_some() {
                    break 'entry matched;
                }
            }
            break 'entry None;
        };
        let Some(concept) = entry else {
            return false;
        };
        if concept.standard && !concept.invalid {
            return true;
        }
        self.mapsto
            .get(&concept.id)
            .is_some_and(|list| {
                list.iter()
                    .any(|target| self.targets.get(target).is_some_and(|(std, invalid)| *std && !invalid))
            })
    }
}

/// One pass over CONCEPT.csv (tab-separated; header line skipped),
/// keeping the rows for the requested codes and ids.
fn scan_concepts(
    path: &Path,
    codes: &HashSet<(&str, &str)>,
    ids: &HashSet<&str>,
    by_code: &mut HashMap<(String, String), Concept>,
    by_id: &mut HashMap<String, Concept>,
) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut first = true;
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        if first {
            first = false;
            continue;
        }
        let row = line.trim_end_matches(['\n', '\r']);
        let fields: Vec<&str> = row.split('\t').collect();
        if fields.len() < 10 {
            continue;
        }
        let (id, vocabulary, code) = (fields[0], fields[3], fields[6]);
        let concept = || Concept {
            id: id.to_owned(),
            vocabulary: vocabulary.to_owned(),
            standard: fields[5] == "S",
            invalid: !fields[9].is_empty(),
        };
        if codes.contains(&(vocabulary, code)) {
            by_code.insert((vocabulary.to_owned(), code.to_owned()), concept());
        }
        if ids.contains(id) {
            by_id.insert(id.to_owned(), concept());
        }
    }
    Ok(())
}

/// One pass over CONCEPT_RELATIONSHIP.csv: valid `Maps to` edges out of
/// `sources` (self-edges excluded).
fn scan_mapsto(
    path: &Path,
    sources: &HashSet<&str>,
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let file = std::fs::File::open(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut first = true;
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        if first {
            first = false;
            continue;
        }
        let Some(row) = line.strip_suffix('\n') else {
            continue;
        };
        let row = row.strip_suffix('\r').unwrap_or(row);
        if !row.contains("\tMaps to\t") {
            continue;
        }
        let fields: Vec<&str> = row.split('\t').collect();
        if fields.len() < 6 {
            continue;
        }
        let (source, target, invalid) = (fields[0], fields[1], fields[5]);
        if !invalid.is_empty() || source == target || !sources.contains(source) {
            continue;
        }
        let list = out.entry(source.to_owned()).or_default();
        if !list.iter().any(|t| t == target) {
            list.push(target.to_owned());
        }
    }
    Ok(out)
}

/// One pass over CONCEPT.csv: standardness of the `Maps to` target ids.
fn scan_target_concepts(
    path: &Path,
    ids: &BTreeSet<&str>,
) -> Result<HashMap<String, (bool, bool)>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut out = HashMap::new();
    let mut line = String::new();
    let mut first = true;
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| format!("reading {}: {e}", path.display()))?
        ;
        if read == 0 {
            break;
        }
        if first {
            first = false;
            continue;
        }
        let row = line.trim_end_matches(['\n', '\r']);
        let fields: Vec<&str> = row.split('\t').collect();
        if fields.len() < 10 {
            continue;
        }
        if ids.contains(fields[0]) {
            out.insert(
                fields[0].to_owned(),
                (fields[5] == "S", !fields[9].is_empty()),
            );
        }
    }
    Ok(out)
}

/// A computed data row of the figure data spec.
#[derive(Clone)]
pub struct DataRow {
    pub section: String,
    pub row: String,
    /// The row's configured title in the figure locale; filled when the
    /// data file is written.
    pub label: String,
    pub track: String,
    pub measure: String,
    pub item: String,
    pub denominator: String,
    pub numerator: String,
    pub share_pct: String,
    pub note: String,
}

/// What one figure run produced: its data rows, the inputs it named as
/// missing (figure id + dataset id, for the diagnostic channel), and the
/// notes its manifest row carries.
pub struct Computed {
    pub rows: Vec<DataRow>,
    pub missing: Vec<String>,
    pub notes: Vec<String>,
}

impl Computed {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            missing: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn skip(&mut self, section: &str, row: &str, track: &str, note: String) {
        self.rows.push(DataRow {
            section: section.to_owned(),
            row: row.to_owned(),
            label: String::new(),
            track: track.to_owned(),
            measure: "skipped".to_owned(),
            item: String::new(),
            denominator: String::new(),
            numerator: String::new(),
            share_pct: String::new(),
            note,
        });
    }

    fn push(
        &mut self,
        section: &str,
        row: &str,
        track: &str,
        measure: &str,
        item: &str,
        denominator: usize,
        numerator: usize,
        note: String,
    ) {
        self.rows.push(DataRow {
            section: section.to_owned(),
            row: row.to_owned(),
            label: String::new(),
            track: track.to_owned(),
            measure: measure.to_owned(),
            item: item.to_owned(),
            denominator: denominator.to_string(),
            numerator: numerator.to_string(),
            share_pct: share(numerator, denominator),
            note,
        });
    }
}

/// Compute one figure. Local usage inputs exist only on hosts carrying
/// the private `local/` tree; numbers needing one are skipped with a
/// named missing-input note and recorded in `missing`.
pub fn compute(figure: &Figure, eval: &mut Eval, local: &LocalInputs) -> Result<Computed, String> {
    match figure.kind.as_str() {
        "coverage-funnel" => coverage_funnel(figure, eval, local),
        "composition-bar" => composition_bar(figure, eval),
        other => Err(format!(
            "figure {} has kind \"{other}\", which this tool does not compute",
            figure.id
        )),
    }
}

fn coverage_funnel(
    figure: &Figure,
    eval: &mut Eval,
    local: &LocalInputs,
) -> Result<Computed, String> {
    let mut computed = Computed::new();
    let dataset_present = figure
        .dataset
        .as_deref()
        .is_some_and(|id| local.datasets.contains_key(id));
    let missing_note = |what: &str| -> String {
        format!(
            "{what} needs local dataset {}, not present on this host",
            figure.dataset.clone().unwrap_or_default()
        )
    };
    if let Some(id) = &figure.dataset {
        if !dataset_present {
            computed.missing.push(id.clone());
        }
    }
    for (section_id, section) in &figure.sections {
        for (row_id, row) in &section.rows {
            let population = eval.population(&row.population)?;
            let endpoints: Vec<(String, String)> = population
                .iter()
                .map(|&index| {
                    let concept = eval.concept(index);
                    (concept.system.clone(), concept.code.clone())
                })
                .collect();
            // Codes of the population carrying a claim matching each
            // stage and tier selector. A selector naming the
            // `target-standard` leaf needs the local Athena release; a
            // run without one skips that stage or tier under a named
            // missing-input note.
            let standard_absent = !eval.has_standard();
            let note_athena_missing = |computed: &mut Computed| {
                if !computed.missing.iter().any(|m| m == "athena") {
                    computed.missing.push("athena".to_owned());
                }
            };
            let mut stage_hits: Vec<(String, BTreeSet<(String, String)>)> = Vec::new();
            for (stage_id, stage) in &figure.stages {
                if standard_absent && Eval::needs_standard(&stage.claims) {
                    note_athena_missing(&mut computed);
                    computed.skip(
                        section_id,
                        row_id,
                        "",
                        format!(
                            "stage {stage_id} needs the local Athena release, not present on this host (pass --athena DIR)"
                        ),
                    );
                    continue;
                }
                let mut hits = BTreeSet::new();
                for endpoint in &endpoints {
                    let edges: Vec<Claim> = eval.claims_of(endpoint).to_vec();
                    let mut matched = false;
                    for &edge in &edges {
                        if eval.eval_claim(&stage.claims, edge)? {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        hits.insert(endpoint.clone());
                    }
                }
                stage_hits.push((stage_id.clone(), hits));
            }
            let mut tier_hits: Vec<(String, BTreeSet<(String, String)>)> = Vec::new();
            for (tier_id, tier) in &figure.tiers {
                if standard_absent && Eval::needs_standard(&tier.claims) {
                    note_athena_missing(&mut computed);
                    computed.skip(
                        section_id,
                        row_id,
                        "",
                        format!(
                            "tier {tier_id} needs the local Athena release, not present on this host (pass --athena DIR)"
                        ),
                    );
                    continue;
                }
                let mut hits = BTreeSet::new();
                for endpoint in &endpoints {
                    let edges: Vec<Claim> = eval.claims_of(endpoint).to_vec();
                    let mut matched = false;
                    for &edge in &edges {
                        if eval.eval_claim(&tier.claims, edge)? {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        hits.insert(endpoint.clone());
                    }
                }
                tier_hits.push((tier_id.clone(), hits));
            }
            let all_stages: BTreeSet<&(String, String)> = stage_hits
                .iter()
                .flat_map(|(_, hits)| hits.iter())
                .collect();
            let usage: HashMap<&(String, String), Vec<&Usage>> = figure
                .dataset
                .as_deref()
                .map(|id| usage_of(local, id))
                .unwrap_or_default();
            for track in &figure.tracks {
                if track.weighting == "by-count" {
                    if dataset_present {
                        // Weighted numerators sum the dataset's recorded
                        // counts over the reaching codes.
                        let counts = figure.dataset.as_deref().map(|id| &local.datasets[id]);
                        let unit = counts
                            .and_then(|dataset| dataset.counts.get(track.count.as_deref()?))
                            .cloned()
                            .unwrap_or_default();
                        let total: u64 = usage
                            .values()
                            .flatten()
                            .filter(|record| {
                                endpoints.contains(&record.endpoint) && record.unit == unit
                            })
                            .map(|record| record.count)
                            .sum();
                        computed.push(
                            section_id,
                            row_id,
                            &track.weighting,
                            "denominator",
                            "events",
                            total as usize,
                            total as usize,
                            format!("unit={unit}"),
                        );
                        for (stage_id, hits) in &stage_hits {
                            let weighted: u64 = usage
                                .values()
                                .flatten()
                                .filter(|record| {
                                    hits.contains(&record.endpoint) && record.unit == unit
                                })
                                .map(|record| record.count)
                                .sum();
                            computed.push(
                                section_id,
                                row_id,
                                &track.weighting,
                                "stage",
                                stage_id,
                                total as usize,
                                weighted as usize,
                                String::new(),
                            );
                        }
                    } else {
                        computed.skip(
                            section_id,
                            row_id,
                            &track.weighting,
                            missing_note(&format!(
                                "by-count track (count={})",
                                track.count.clone().unwrap_or_default()
                            )),
                        );
                    }
                    continue;
                }
                let denominator = match track.denominator.as_deref() {
                    Some("release") | Some("defined") => {
                        let note = (track.denominator.as_deref() == Some("release"))
                            .then(|| {
                                "release denominator from the tree's definitions (no release source configured)".to_owned()
                            })
                            .unwrap_or_default();
                        computed.push(
                            section_id,
                            row_id,
                            &track.weighting,
                            "denominator",
                            track.denominator.as_deref().unwrap_or(""),
                            population.len(),
                            population.len(),
                            note,
                        );
                        population.len()
                    }
                    Some("present") | Some("visible") if dataset_present => {
                        // A code the dataset records at all is present;
                        // visible adds the bands unit's threshold.
                        let visible = |record: &Usage| match figure.bands.as_ref() {
                            Some(bands) => {
                                record.unit == bands.unit && record.count >= bands.threshold
                            }
                            None => true,
                        };
                        let count = usage
                            .keys()
                            .filter(|endpoint| {
                                endpoints.contains(*endpoint)
                                    && (track.denominator.as_deref() == Some("present")
                                        || usage.get(*endpoint).is_some_and(
                                            |records: &Vec<&Usage>| {
                                                records.iter().any(|record| visible(*record))
                                            },
                                        ))
                            })
                            .count();
                        computed.push(
                            section_id,
                            row_id,
                            &track.weighting,
                            "denominator",
                            track.denominator.as_deref().unwrap_or(""),
                            count,
                            count,
                            String::new(),
                        );
                        count
                    }
                    _ => {
                        computed.skip(
                            section_id,
                            row_id,
                            &track.weighting,
                            missing_note(&format!(
                                "denominator {}",
                                track.denominator.clone().unwrap_or_default()
                            )),
                        );
                        continue;
                    }
                };
                for (stage_id, hits) in &stage_hits {
                    computed.push(
                        section_id,
                        row_id,
                        &track.weighting,
                        "stage",
                        stage_id,
                        denominator,
                        hits.len(),
                        String::new(),
                    );
                }
                // The exclusive partition of the reached codes by the set
                // of stages each one hits: one row per non-empty stage
                // set, its item the hit stage ids in declaration order
                // joined by `+`. Any exclusive carving of the stages (a
                // tier by route, say) is a sum over these rows, so a
                // renderer derives its segments without re-reading the
                // corpus.
                let mut cells: BTreeMap<String, usize> = BTreeMap::new();
                for endpoint in &endpoints {
                    let signature: Vec<&str> = stage_hits
                        .iter()
                        .filter(|(_, hits)| hits.contains(endpoint))
                        .map(|(stage_id, _)| stage_id.as_str())
                        .collect();
                    if !signature.is_empty() {
                        *cells.entry(signature.join("+")).or_default() += 1;
                    }
                }
                for (signature, count) in &cells {
                    computed.push(
                        section_id,
                        row_id,
                        &track.weighting,
                        "partition",
                        signature,
                        denominator,
                        *count,
                        String::new(),
                    );
                }
                if let Some(segments) = tier_route_segments(&figure.stages, &cells) {
                    for (item, count) in segments {
                        computed.push(
                            section_id,
                            row_id,
                            &track.weighting,
                            "segment",
                            &item,
                            denominator,
                            count,
                            String::new(),
                        );
                    }
                }
                for (tier_id, hits) in &tier_hits {
                    let extra = hits
                        .iter()
                        .filter(|endpoint| !all_stages.contains(*endpoint))
                        .count();
                    computed.push(
                        section_id,
                        row_id,
                        &track.weighting,
                        "tier-extra",
                        tier_id,
                        denominator,
                        extra,
                        String::new(),
                    );
                }
            }
            if let Some(bands) = &figure.bands {
                if dataset_present {
                    // The endpoint's recorded count in the bands unit:
                    // the maximum its records state, zero when none.
                    let count_of = |endpoint: &(String, String)| -> u64 {
                        usage
                            .get(endpoint)
                            .map(|records: &Vec<&Usage>| {
                                records
                                    .iter()
                                    .filter(|record| record.unit == bands.unit)
                                    .map(|record| record.count)
                                    .max()
                                    .unwrap_or(0)
                            })
                            .unwrap_or(0)
                    };
                    let mut present = 0usize;
                    let mut below = 0usize;
                    let mut never = 0usize;
                    for endpoint in &endpoints {
                        let count = count_of(endpoint);
                        if count >= bands.threshold {
                            present += 1;
                        } else if count > 0 {
                            below += 1;
                        } else {
                            never += 1;
                        }
                    }
                    for (item, count) in [("present", present), ("below", below), ("never", never)]
                    {
                        computed.push(
                            section_id,
                            row_id,
                            "",
                            "band",
                            item,
                            population.len(),
                            count,
                            String::new(),
                        );
                    }
                } else {
                    computed.skip(section_id, row_id, "", missing_note("presence bands"));
                }
            }
        }
    }
    Ok(computed)
}

/// The confidence-tier ladder a coverage funnel may declare, strongest
/// first: (tier, the stage holding the tier's codes, the stage holding
/// its codes with a direct standard-vocabulary target). A code takes
/// the first tier whose stage it hits, so the tiers are exclusive.
const TIER_LADDER: [(&str, &str, &str); 3] = [
    ("high", "high-confidence", "high-direct"),
    ("medium", "medium-plus", "medium-plus-direct"),
    ("low", "target-standard", "direct"),
];

/// The exclusive tier-by-route segments of one row, summed from its
/// stage-set partition: `<tier>-direct` and `<tier>-hop` for each tier
/// of the ladder, zero counts included so every row has the same shape.
/// None when the figure does not declare every ladder stage.
fn tier_route_segments(
    stages: &[(String, crate::figures_model::Stage)],
    cells: &BTreeMap<String, usize>,
) -> Option<Vec<(String, usize)>> {
    let declared: BTreeSet<&str> = stages.iter().map(|(id, _)| id.as_str()).collect();
    if !TIER_LADDER
        .iter()
        .all(|(_, tier, direct)| declared.contains(tier) && declared.contains(direct))
    {
        return None;
    }
    let mut sums = vec![0usize; TIER_LADDER.len() * 2];
    for (signature, count) in cells {
        let hit: BTreeSet<&str> = signature.split('+').collect();
        if let Some(position) = TIER_LADDER.iter().position(|(_, tier, _)| hit.contains(tier)) {
            let direct = hit.contains(TIER_LADDER[position].2);
            sums[position * 2 + usize::from(!direct)] += count;
        }
    }
    Some(
        TIER_LADDER
            .iter()
            .enumerate()
            .flat_map(|(position, (tier, _, _))| {
                [
                    (format!("{tier}-direct"), sums[position * 2]),
                    (format!("{tier}-hop"), sums[position * 2 + 1]),
                ]
            })
            .collect(),
    )
}

/// Compute one composition-bar figure: per-segment counts over the claim
/// population, first matching segment wins, misses reported as `other`.
fn composition_bar(figure: &Figure, eval: &mut Eval) -> Result<Computed, String> {
    let mut computed = Computed::new();
    let count_sources = figure.unit.as_deref() == Some("sources");
    let mut population: Vec<usize> = Vec::new();
    for index in 0..eval.corpus().mappings.len() {
        let matched = match &figure.population {
            Some(selector) => eval.eval_claim(selector, Claim::Mapping(index))?,
            None => true,
        };
        if matched {
            population.push(index);
        }
    }
    let mut claimed = vec![false; population.len()];
    let mut segments: Vec<(&String, usize, BTreeSet<(String, String)>)> = Vec::new();
    for (segment_id, segment) in &figure.segments {
        let mut count = 0usize;
        let mut sources = BTreeSet::new();
        for (position, &index) in population.iter().enumerate() {
            if claimed[position] {
                continue;
            }
            if eval.eval_claim(&segment.claims, Claim::Mapping(index))? {
                claimed[position] = true;
                count += 1;
                sources.insert(eval.corpus().mappings[index].source.clone());
            }
        }
        segments.push((segment_id, count, sources));
    }
    let other_claims = claimed.iter().filter(|claimed| !**claimed).count();
    let mut other_sources = BTreeSet::new();
    for (position, &index) in population.iter().enumerate() {
        if !claimed[position] {
            other_sources.insert(eval.corpus().mappings[index].source.clone());
        }
    }
    let total = population.len();
    let total_sources: usize = population
        .iter()
        .map(|&index| eval.corpus().mappings[index].source.clone())
        .collect::<BTreeSet<_>>()
        .len();
    let denominator = if count_sources { total_sources } else { total };
    let size_of = |segment: &(&String, usize, BTreeSet<(String, String)>)| {
        if count_sources {
            segment.2.len()
        } else {
            segment.1
        }
    };
    for segment in &segments {
        computed.push(
            "",
            "",
            "",
            "segment",
            segment.0,
            denominator,
            size_of(segment),
            String::new(),
        );
    }
    let other_size = if count_sources {
        other_sources.len()
    } else {
        other_claims
    };
    computed.push(
        "",
        "",
        "",
        "segment",
        "other",
        denominator,
        other_size,
        String::new(),
    );
    if other_claims > 0 {
        computed.notes.push(format!(
            "{other_claims} claim(s) of the population match no segment (reported as other)"
        ));
    }
    for (brace_id, members) in &figure.braces {
        computed
            .notes
            .push(format!("brace {brace_id}: {}", members.join("+")));
    }
    Ok(computed)
}
