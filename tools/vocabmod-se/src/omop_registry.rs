//! The OMOP constants registry: vocab/_registry/omop/omop.kdl, bound under
//! the schema's omop profile and read through the generated processor,
//! so the spellings stay authored data versioned with the corpus. A
//! diagnostic, an unknown table, or a broken cross-row rule fails
//! closed: the layer cannot silently rot.

use std::collections::BTreeMap;
use std::path::Path;

use vocab_proc::{Item, TypedItem};

#[derive(Debug, Default)]
pub struct OmopRegistry {
    pub vocabulary_id: BTreeMap<String, String>,
    pub domain_id: BTreeMap<String, String>,
    pub relationship_id: BTreeMap<String, String>,
    pub invalid_reason: BTreeMap<String, String>,
    pub language_concept_id: BTreeMap<String, String>,
    /// target_vocabulary_id per standard endpoint system; the key set is
    /// exactly the systems whose endpoint codes are OMOP concept ids.
    pub target_vocabulary: BTreeMap<String, String>,
    /// One RELATIONSHIP.csv row per authored spelling, in document order.
    pub relationships: Vec<RelationshipDef>,
    pub validity_start: String,
    pub validity_end: String,
    pub version: String,
    /// One export-time id band per system, as authored.
    pub id_bands: BTreeMap<String, (u64, u64)>,
}

/// One authored RELATIONSHIP.csv row: the spelling, its display name,
/// hierarchy flags, and the spelling the inverse row carries.
#[derive(Debug)]
pub struct RelationshipDef {
    pub relationship_id: String,
    pub relationship_name: String,
    pub hierarchical: bool,
    pub defines_ancestry: bool,
    pub inverse: String,
}

/// Read the registry document through the generated processor. A
/// diagnostic, a duplicate table or key, an unrecognized table, or a
/// broken cross-row rule is an error naming the construct.
pub fn read(schema: &Path, root: &Path, document: &Path) -> Result<OmopRegistry, String> {
    let definition = crate::corpus::compile(schema, root)?;
    let reader = crate::corpus::typed_reader(&definition, root)?;
    let mut items = Vec::new();
    let mut diagnostics = Vec::new();
    let failed = reader
        .read(&[document], &mut |item| match item {
            TypedItem::Declaration { value, .. } | TypedItem::Record { value, .. } => {
                items.push(value);
            }
            TypedItem::Diagnostic(record) => diagnostics.push(format!(
                "{} — {}",
                record.code,
                record.message.as_deref().unwrap_or("")
            )),
            _ => {}
        })
        .map_err(|error| format!("registry read error: {error}"))?;
    if !diagnostics.is_empty() {
        return Err(format!("registry diagnostics: {}", diagnostics.join("; ")));
    }
    if failed {
        return Err("registry reported error diagnostics without surfacing them".to_owned());
    }
    build(items)
}

/// Assemble the registry from its typed items, in document order.
fn build(items: Vec<Item>) -> Result<OmopRegistry, String> {
    let mut registry = OmopRegistry::default();
    let mut tables: BTreeMap<&str, &mut BTreeMap<String, String>> = BTreeMap::from([
        ("omop-vocabulary-id", &mut registry.vocabulary_id),
        ("omop-domain-id", &mut registry.domain_id),
        ("omop-relationship-id", &mut registry.relationship_id),
        ("omop-invalid-reason", &mut registry.invalid_reason),
        (
            "omop-language-concept-id",
            &mut registry.language_concept_id,
        ),
        ("omop-target-vocabulary", &mut registry.target_vocabulary),
    ]);
    let mut version: Option<String> = None;
    let mut validity: Option<(String, String)> = None;
    let mut relationships: Vec<RelationshipDef> = Vec::new();
    let mut id_bands: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for item in items {
        match item {
            Item::TranslationTable(table) => {
                let name = table.id.as_str();
                let map = tables
                    .get_mut(name)
                    .ok_or_else(|| format!("unknown translation-table \"{name}\""))?;
                if !map.is_empty() {
                    return Err(format!("translation-table \"{name}\" declared twice"));
                }
                for (key, entry) in table.entries.iter() {
                    let value = entry
                        .value
                        .clone()
                        .ok_or_else(|| format!("an entry of \"{name}\" without exactly two values"))?;
                    map.insert(key.clone(), value);
                }
            }
            Item::OmopValidity(row) => {
                validity = Some((row.start.to_string(), row.end.to_string()));
            }
            Item::OmopRegistry(row) => {
                if version.is_some() {
                    return Err("omop-registry declared twice".to_owned());
                }
                version = Some(row.version.to_string());
            }
            Item::OmopRelationship(row) => {
                if relationships.iter().any(|def| def.relationship_id == row.id) {
                    return Err(format!("omop-relationship \"{}\" declared twice", row.id));
                }
                relationships.push(RelationshipDef {
                    relationship_id: row.id,
                    relationship_name: row.name,
                    hierarchical: row.hierarchical,
                    defines_ancestry: row.defines_ancestry,
                    inverse: row.inverse,
                });
            }
            Item::OmopIdBand(band) => {
                let system = band.system.to_string();
                if band.first >= band.last {
                    return Err(format!("omop-id-band \"{system}\" is empty"));
                }
                if id_bands
                    .insert(system.clone(), (band.first, band.last))
                    .is_some()
                {
                    return Err(format!("omop-id-band \"{system}\" declared twice"));
                }
            }
            _ => return Err("the registry carries a node outside the omop profile".to_owned()),
        }
    }
    drop(tables);
    let Some((start, end)) = validity else {
        return Err("registry carries no omop-validity defaults".to_owned());
    };
    let Some(version) = version else {
        return Err("registry carries no omop-registry version".to_owned());
    };
    registry.validity_start = start;
    registry.validity_end = end;
    registry.version = version;
    registry.relationships = relationships;
    registry.id_bands = id_bands;
    // Every spelling a mapping kind can emit needs a definition row, and
    // every inverse named must itself be a defined spelling.
    for spelling in registry.relationship_id.values() {
        if !registry
            .relationships
            .iter()
            .any(|def| &def.relationship_id == spelling)
        {
            return Err(format!(
                "relationship spelling \"{spelling}\" has no omop-relationship definition"
            ));
        }
    }
    for def in &registry.relationships {
        if !registry
            .relationships
            .iter()
            .any(|other| other.relationship_id == def.inverse)
        {
            return Err(format!(
                "omop-relationship \"{}\" names undefined inverse \"{}\"",
                def.relationship_id, def.inverse
            ));
        }
    }
    Ok(registry)
}

/// The spelling the inverse direction of `spelling` carries.
pub fn inverse_of<'a>(registry: &'a OmopRegistry, spelling: &str) -> Option<&'a str> {
    registry
        .relationships
        .iter()
        .find(|def| def.relationship_id == spelling)
        .map(|def| def.inverse.as_str())
}
