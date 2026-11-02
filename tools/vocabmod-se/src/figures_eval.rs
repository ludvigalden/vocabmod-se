//! Selector evaluation over the corpus: every selector leaf resolved
//! against concepts, mapping claims, omop bindings, value sets, and
//! parent links. `not` is NOR over its members; unknown values never
//! match; a leaf evaluated over the wrong population fails closed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::corpus::Corpus;
use crate::figures_compute::Standardness;
use crate::figures_model::Selector;

/// One committed claim on a code: a mapping assertion or an omop
/// binding — the two claim shapes the coverage definition counts.
#[derive(Clone, Copy, PartialEq)]
pub enum Claim {
    Mapping(usize),
    Binding(usize),
}

/// One local usage observation, from a `usage` record.
pub struct Usage {
    pub dataset: String,
    pub endpoint: (String, String),
    pub count: u64,
    pub unit: String,
}

/// One declared local dataset: its id and the count names it declares.
pub struct Dataset {
    pub id: String,
    pub counts: BTreeMap<String, String>,
}

/// The local inputs a figure reading may admit; empty when the host has
/// no `local/` tree.
#[derive(Default)]
pub struct LocalInputs {
    pub datasets: BTreeMap<String, Dataset>,
    pub usage: Vec<Usage>,
}

/// The usage records of one dataset, keyed by concept endpoint.
pub fn usage_of<'a>(
    local: &'a LocalInputs,
    dataset: &str,
) -> HashMap<&'a (String, String), Vec<&'a Usage>> {
    let mut map: HashMap<&'a (String, String), Vec<&'a Usage>> = HashMap::new();
    for record in &local.usage {
        if record.dataset == dataset {
            map.entry(&record.endpoint).or_default().push(record);
        }
    }
    map
}

/// Exact percent string from integer counts: two decimals, integer math.
pub fn share(numerator: usize, denominator: usize) -> String {
    if denominator == 0 {
        return String::new();
    }
    let basis_points = (numerator * 10000) / denominator;
    format!("{}.{:02}", basis_points / 100, basis_points % 100)
}

/// Everything the selectors resolve against, built once per run.
pub struct Eval<'a> {
    corpus: &'a Corpus,
    concept_index: HashMap<(String, String), usize>,
    /// Concepts carrying an omop binding, by endpoint.
    bound: HashSet<(String, String)>,
    /// Concept endpoint to the claims whose source it is, mappings and
    /// omop bindings alike.
    pub outbound: HashMap<(String, String), Vec<Claim>>,
    children: BTreeMap<(String, String), Vec<(String, String)>>,
    descendants: HashMap<(String, String), BTreeSet<(String, String)>>,
    reach: HashMap<((String, String), bool, String), bool>,
    /// The local-Athena standardness resolver behind a `target-standard`
    /// leaf; absent when the host carries no Athena release.
    standard: Option<&'a Standardness>,
}

impl<'a> Eval<'a> {
    pub fn new(corpus: &'a Corpus, standard: Option<&'a Standardness>) -> Self {
        let concept_index = corpus
            .concepts
            .iter()
            .enumerate()
            .map(|(index, concept)| ((concept.system.clone(), concept.code.clone()), index))
            .collect();
        let bound = corpus
            .bindings
            .iter()
            .map(|binding| (binding.system.clone(), binding.code.clone()))
            .collect();
        let mut outbound: HashMap<(String, String), Vec<Claim>> = HashMap::new();
        for (index, mapping) in corpus.mappings.iter().enumerate() {
            outbound
                .entry(mapping.source.clone())
                .or_default()
                .push(Claim::Mapping(index));
        }
        for (index, binding) in corpus.bindings.iter().enumerate() {
            outbound
                .entry((binding.system.clone(), binding.code.clone()))
                .or_default()
                .push(Claim::Binding(index));
        }
        let mut children: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
        for concept in &corpus.concepts {
            let child = (concept.system.clone(), concept.code.clone());
            for parent in &concept.parents {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(child.clone());
            }
        }
        Self {
            corpus,
            concept_index,
            bound,
            outbound,
            children,
            descendants: HashMap::new(),
            reach: HashMap::new(),
            standard,
        }
    }

    /// Bind the local-Athena standardness resolver (the `target-standard`
    /// leaf); absent when the host carries no Athena release, in which
    /// case the leaf fails closed.
    pub fn with_standard(mut self, standard: Option<&'a Standardness>) -> Self {
        self.standard = standard;
        self
    }

    /// Whether any selector in the tree needs the local Athena release.
    pub fn needs_standard(selector: &Selector) -> bool {
        match selector {
            Selector::TargetStandard => true,
            Selector::All(members) | Selector::Any(members) | Selector::Not(members) => {
                members.iter().any(Self::needs_standard)
            }
            _ => false,
        }
    }

    /// Whether the resolver is bound.
    pub fn has_standard(&self) -> bool {
        self.standard.is_some()
    }

    pub fn corpus(&self) -> &Corpus {
        self.corpus
    }

    pub fn concept(&self, index: usize) -> &crate::corpus::ConceptData {
        &self.corpus.concepts[index]
    }

    /// The system family a selector spelling names: a system the corpus
    /// declares names itself; anything else fails closed, naming the
    /// spelling.
    fn family(&self, system: &str) -> Result<BTreeSet<String>, String> {
        if self.corpus.systems.contains(system) {
            return Ok(BTreeSet::from([system.to_owned()]));
        }
        Err(format!(
            "a selector names system \"{system}\", which the corpus does not declare"
        ))
    }

    fn direct_hit(&self, endpoint: &(String, String), family: &str) -> bool {
        family.split('+').any(|system| endpoint.0 == system) || self.bound.contains(endpoint)
    }

    /// Whether a claim target reaches one of `family`: directly (a
    /// target endpoint in the family, or a target concept carrying an
    /// omop binding), or through further mappings when `chain`. Families
    /// are keyed by their '+-joined signature, which cannot collide with
    /// a system spelling because ids never contain '+'.
    fn reaches(&mut self, target: &(String, String), chain: bool, family: &str) -> bool {
        let key = (target.clone(), chain, family.to_owned());
        if let Some(cached) = self.reach.get(&key) {
            return *cached;
        }
        self.reach.insert(key.clone(), false);
        let mut result = self.direct_hit(target, family);
        if !result && chain && self.concept_index.contains_key(target) {
            let edges = self.outbound.get(target).cloned().unwrap_or_default();
            for edge in edges {
                // Chaining walks mappings; an omop binding is a terminal
                // identifier assertion and never chains further.
                let Claim::Mapping(index) = edge else {
                    continue;
                };
                let endpoint = self.corpus.mappings[index].target.clone();
                if self.direct_hit(&endpoint, family) || self.reaches(&endpoint, true, family) {
                    result = true;
                    break;
                }
            }
        }
        self.reach.insert(key, result);
        result
    }

    fn target_family_signature(&self, system: &str) -> Result<String, String> {
        Ok(self
            .family(system)?
            .into_iter()
            .collect::<Vec<_>>()
            .join("+"))
    }

    fn descendants_of(&mut self, root: &(String, String)) -> BTreeSet<(String, String)> {
        if let Some(cached) = self.descendants.get(root) {
            return cached.clone();
        }
        let mut seen = BTreeSet::from([root.clone()]);
        let mut queue = vec![root.clone()];
        while let Some(current) = queue.pop() {
            if let Some(children) = self.children.get(&current) {
                for child in children {
                    if seen.insert(child.clone()) {
                        queue.push(child.clone());
                    }
                }
            }
        }
        self.descendants.insert(root.clone(), seen.clone());
        seen
    }

    /// Evaluate a selector over one concept (by corpus index).
    pub fn eval_concept(&mut self, selector: &Selector, index: usize) -> Result<bool, String> {
        let endpoint = {
            let concept = self.concept(index);
            (concept.system.clone(), concept.code.clone())
        };
        Ok(match selector {
            Selector::All(members) => {
                for member in members {
                    if !self.eval_concept(member, index)? {
                        return Ok(false);
                    }
                }
                true
            }
            Selector::Any(members) => {
                for member in members {
                    if self.eval_concept(member, index)? {
                        return Ok(true);
                    }
                }
                false
            }
            Selector::Not(members) => {
                for member in members {
                    if self.eval_concept(member, index)? {
                        return Ok(false);
                    }
                }
                true
            }
            Selector::System(system) => self.concept(index).system == *system,
            Selector::Under(root) => self.descendants_of(root).contains(&endpoint),
            Selector::CodePrefix(prefixes) => prefixes
                .iter()
                .any(|prefix| self.concept(index).code.starts_with(prefix.as_str())),
            Selector::Class(classes) => {
                let (spelling, system) = match (
                    &self.concept(index).class,
                    &self.concept(index).class_system,
                ) {
                    (Some(spelling), Some(system)) => (spelling.clone(), system.clone()),
                    _ => return Ok(false),
                };
                classes
                    .iter()
                    .any(|(class_system, class)| *class_system == system && *class == spelling)
            }
            Selector::MemberOf(set) => {
                let members = self.corpus.value_sets.get(set).ok_or_else(|| {
                    format!(
                        "a selector names value set \"{set}\", which the corpus does not declare"
                    )
                })?;
                members.contains(&endpoint)
            }
            other => {
                return Err(format!(
                    "selector leaf {} does not apply to a concept population",
                    leaf_name(other)
                ));
            }
        })
    }

    /// Evaluate a selector over one committed claim — a mapping
    /// assertion or an omop binding. Leaves defined only over mappings
    /// fail closed on a binding; only `target-standard` (the coverage
    /// definition's leaf) and the combinators bind on both.
    pub fn eval_claim(&mut self, selector: &Selector, claim: Claim) -> Result<bool, String> {
        let mapping = |index: usize| -> &crate::corpus::MappingData {
            &self.corpus.mappings[index]
        };
        Ok(match claim {
            Claim::Mapping(index) => match selector {
                Selector::All(members) => {
                    for member in members {
                        if !self.eval_claim(member, claim)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                Selector::Any(members) => {
                    for member in members {
                        if self.eval_claim(member, claim)? {
                            return Ok(true);
                        }
                    }
                    false
                }
                Selector::Not(members) => {
                    for member in members {
                        if self.eval_claim(member, claim)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                Selector::Collection(collections) => {
                    collections.contains(&mapping(index).collection)
                }
                Selector::Target { system, chain } => {
                    let family = self.target_family_signature(system)?;
                    let target = mapping(index).target.clone();
                    self.reaches(&target, *chain, &family)
                }
                Selector::TargetStandard => match self.standard {
                    Some(resolver) => {
                        let target = mapping(index).target.clone();
                        resolver.covered(&target.0, &target.1)
                    }
                    None => false,
                },
                Selector::Relationship(relationships) => {
                    relationships.contains(&mapping(index).relationship)
                }
                Selector::Preference(preference) => {
                    mapping(index).preference.as_deref() == Some(preference.as_str())
                }
                Selector::MinConfidence { score, scheme } => {
                    match mapping(index).confidence.clone() {
                        Some((claim_score, claim_scheme)) => {
                            claim_scheme == *scheme && decimal_at_least(&claim_score, score)
                        }
                        None => false,
                    }
                }
                Selector::Method(methods) => mapping(index)
                    .method
                    .as_deref()
                    .is_some_and(|method| methods.iter().any(|candidate| candidate == method)),
                Selector::Imported { table, labels } => mapping(index)
                    .imported
                    .iter()
                    .any(|(claim_table, label)| claim_table == table && labels.contains(label)),
                Selector::TargetMemberOf(set) => {
                    let members = self.corpus.value_sets.get(set).ok_or_else(|| {
                        format!(
                            "a selector names value set \"{set}\", which the corpus does not declare"
                        )
                    })?;
                    let target = mapping(index).target.clone();
                    members.contains(&target)
                }
                other => {
                    return Err(format!(
                        "selector leaf {} does not apply to a claim population",
                        leaf_name(other)
                    ));
                }
            },
            Claim::Binding(index) => match selector {
                Selector::All(members) => {
                    for member in members {
                        if !self.eval_claim(member, claim)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                Selector::Any(members) => {
                    for member in members {
                        if self.eval_claim(member, claim)? {
                            return Ok(true);
                        }
                    }
                    false
                }
                Selector::Not(members) => {
                    for member in members {
                        if self.eval_claim(member, claim)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                Selector::TargetStandard => match self.standard {
                    Some(resolver) => {
                        let endpoint = {
                            let binding = &self.corpus.bindings[index];
                            (binding.omop_system.clone(), binding.omop_code.clone())
                        };
                        resolver.covered(&endpoint.0, &endpoint.1)
                    }
                    None => false,
                },
                // A leaf defined only over mappings fails closed on a
                // binding.
                _ => false,
            },
        })
    }

    /// The concepts a row's population selector selects, in corpus order.
    pub fn population(&mut self, selector: &Selector) -> Result<Vec<usize>, String> {
        let mut selected = Vec::new();
        for index in 0..self.corpus.concepts.len() {
            if self.eval_concept(selector, index)? {
                selected.push(index);
            }
        }
        Ok(selected)
    }

    /// The claims on one endpoint — mappings and omop bindings, in
    /// corpus order.
    pub fn claims_of(&self, endpoint: &(String, String)) -> &[Claim] {
        self.outbound
            .get(endpoint)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

fn leaf_name(selector: &Selector) -> &'static str {
    match selector {
        Selector::All(_) => "all",
        Selector::Any(_) => "any",
        Selector::Not(_) => "not",
        Selector::System(_) => "system",
        Selector::Under(_) => "under",
        Selector::CodePrefix(_) => "code-prefix",
        Selector::Class(_) => "class",
        Selector::MemberOf(_) => "member-of",
        Selector::Collection(_) => "collection",
        Selector::Target { .. } => "target",
        Selector::TargetStandard => "target-standard",
        Selector::Relationship(_) => "relationship",
        Selector::Preference(_) => "preference",
        Selector::MinConfidence { .. } => "min-confidence",
        Selector::Method(_) => "method",
        Selector::Imported { .. } => "imported",
        Selector::TargetMemberOf(_) => "target-member-of",
    }
}

/// Exact decimal comparison `a >= b` on written decimal strings, by digit
/// comparison; never through binary floating point.
pub(crate) fn decimal_at_least(a: &str, b: &str) -> bool {
    let (a_negative, a_integer, a_fraction) = decimal_parts(a);
    let (b_negative, b_integer, b_fraction) = decimal_parts(b);
    let compare = a_integer.len().cmp(&b_integer.len()).then_with(|| {
        let width = a_fraction.len().max(b_fraction.len());
        let pad = |fraction: &str| format!("{fraction:0<width$}");
        a_integer
            .cmp(b_integer)
            .then(pad(a_fraction).cmp(&pad(b_fraction)))
    });
    use std::cmp::Ordering as O;
    match (a_negative, b_negative) {
        (false, false) => compare != O::Less,
        (true, true) => compare != O::Greater,
        (true, false) => false,
        (false, true) => true,
    }
}

/// (negative, integer digits without leading zeros, fraction digits
/// without trailing zeros) of a written decimal string.
fn decimal_parts(text: &str) -> (bool, &str, &str) {
    let negative = text.starts_with('-');
    let bare = text.trim_start_matches(['-', '+']);
    let (integer, fraction) = match bare.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (bare, ""),
    };
    (
        negative,
        integer.trim_start_matches('0'),
        fraction.trim_end_matches('0'),
    )
}
