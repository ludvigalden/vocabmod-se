//! The committed census artifact (vocab/_registry/census.kdl),
//! projected during the corpus reading. The census-system and
//! census-collection children bind into the census declaration's own
//! `systems`/`collections` fields, so the projection reads them off the
//! generated `Census` type. The index cross-checks every count it can
//! against these rows, reading them through the same typed reading as
//! the counts it audits.

use std::collections::BTreeMap;


#[derive(Default)]
pub struct Census {
    pub systems: Vec<CensusSystem>,
    pub collections: Vec<CensusCollection>,
}

pub struct CensusSystem {
    pub id: String,
    pub concepts: u64,
    pub inactive: u64,
    pub retired: u64,
    pub named: u64,
    pub sv: u64,
    pub en: u64,
}

pub struct CensusCollection {
    pub id: String,
    pub source: String,
    pub target: String,
    pub mappings: u64,
    pub bindings: u64,
    pub assessed: u64,
    pub kinds: BTreeMap<String, u64>,
    pub tiers: BTreeMap<String, u64>,
}

/// Project the census declaration's bound rows, in document order.
pub fn project(census: &mut Census, declared: &vocab_proc::types::Census) {
    for row in &declared.systems {
        census.systems.push(CensusSystem {
            id: row.system.to_string(),
            concepts: row.concepts,
            inactive: row.inactive,
            retired: row.retired,
            named: row.named,
            sv: row.sv,
            en: row.en,
        });
    }
    for row in &declared.collections {
        let mut kinds = BTreeMap::new();
        for kind in &row.kinds {
            *kinds.entry(kind.kind.member().to_owned()).or_default() += kind.count;
        }
        let mut tiers = BTreeMap::new();
        for tier in &row.tiers {
            *tiers.entry(tier.tier.clone()).or_default() += tier.count;
        }
        census.collections.push(CensusCollection {
            id: row.id.clone(),
            source: row.source.to_string(),
            target: row.target.to_string(),
            mappings: row.mappings,
            bindings: row.bindings,
            assessed: row.assessed,
            kinds,
            tiers,
        });
    }
}
