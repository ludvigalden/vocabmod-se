//! The figure-configuration model: one committed `figure` declaration,
//! read through the generated processor, in the shapes the computation
//! consumes. References keep their written spellings; the corpus resolves
//! them.

use crate::corpus::endpoint_of;

/// One selector: a boolean tree over concepts (a row population) or over
/// claims (a stage, tier, segment, or the composition population). The
/// combinators are faithful to the schema: `not` is NOR over its members.
pub enum Selector {
    All(Vec<Selector>),
    Any(Vec<Selector>),
    Not(Vec<Selector>),
    System(String),
    /// The concept and its descendants: (system, code).
    Under((String, String)),
    CodePrefix(Vec<String>),
    /// Qualified category spellings: (system, category).
    Class(Vec<(String, String)>),
    MemberOf(String),
    Collection(Vec<String>),
    Target {
        system: String,
        chain: bool,
    },
    /// Presence alone: the claim's target resolves in the local Athena
    /// release to an OMOP-standard concept. Needs the Athena release; a
    /// run without one fails the leaf closed.
    TargetStandard,
    Relationship(Vec<String>),
    Preference(String),
    MinConfidence {
        score: String,
        scheme: String,
    },
    Method(Vec<String>),
    Imported {
        table: String,
        labels: Vec<String>,
    },
    TargetMemberOf(String),
}

pub struct Track {
    pub weighting: String,
    pub denominator: Option<String>,
    pub count: Option<String>,
}

pub struct Stage {
    pub claims: Selector,
}

pub struct Tier {
    pub claims: Selector,
}

pub struct Bands {
    pub threshold: u64,
    pub unit: String,
}

pub struct Row {
    pub target: String,
    pub label: String,
    /// The row's free-text note, when the configuration carries one.
    pub note: Option<String>,
    pub population: Selector,
}

pub struct Section {
    pub rows: Vec<(String, Row)>,
}

pub struct Segment {
    pub claims: Selector,
}

pub struct Figure {
    pub id: String,
    pub kind: String,
    pub dataset: Option<String>,
    pub unit: Option<String>,
    pub locale: String,
    /// The declared output stem (its .csv lands beside it) and formats.
    pub output: String,
    pub output_formats: Vec<String>,
    pub tracks: Vec<Track>,
    pub stages: Vec<(String, Stage)>,
    pub tiers: Vec<(String, Tier)>,
    pub bands: Option<Bands>,
    pub sections: Vec<(String, Section)>,
    pub population: Option<Selector>,
    pub segments: Vec<(String, Segment)>,
    pub braces: Vec<(String, Vec<String>)>,
}

/// One `title`-like text list in the figure's own locale, falling back
/// to the first text when the locale names none.
pub fn localized(texts: &[(String, String)], locale: &str) -> String {
    texts
        .iter()
        .find(|(text_locale, _)| text_locale == locale)
        .or_else(|| texts.first())
        .map(|(_, text)| text.clone())
        .unwrap_or_default()
}

/// The computation's model of one figure declaration, read from its
/// generated type. Enum members keep their schema spellings and
/// references their written spellings; the corpus resolves them.
pub fn from_typed(figure: &vocab_proc::types::Figure) -> Result<Figure, String> {
    let id = figure.id.clone();
    let locale = figure
        .locale
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "en".to_owned());
    let texts = |texts: &[vocab_proc::types::Text]| -> Vec<(String, String)> {
        texts
            .iter()
            .map(|text| (text.locale.to_string(), text.text.clone()))
            .collect()
    };
    let sections = figure
        .sections
        .iter()
        .map(|(section_id, section)| {
            let rows = section
                .rows
                .iter()
                .map(|(row_id, row)| {
                    (
                        row_id.clone(),
                        Row {
                            target: row.target.written().to_owned(),
                            label: localized(&texts(&row.label), &locale),
                            note: (!row.note.is_empty())
                                .then(|| localized(&texts(&row.note), &locale)),
                            population: selector(&row.population),
                        },
                    )
                })
                .collect();
            (section_id.clone(), Section { rows })
        })
        .collect();
    Ok(Figure {
        id,
        kind: figure.kind.member().to_owned(),
        dataset: figure.dataset.as_ref().map(ToString::to_string),
        unit: figure.unit.map(|unit| unit.member().to_owned()),
        output: figure.output.path.clone(),
        output_formats: figure
            .output
            .formats
            .iter()
            .map(|format| format.member().to_owned())
            .collect(),
        locale,
        tracks: figure
            .tracks
            .iter()
            .map(|track| Track {
                weighting: track.weighting.member().to_owned(),
                denominator: track.denominator.map(|d| d.member().to_owned()),
                count: track.count.clone(),
            })
            .collect(),
        stages: figure
            .stages
            .iter()
            .map(|(id, stage)| (id.clone(), Stage::new(selector(&stage.claims))))
            .collect(),
        tiers: figure
            .tiers
            .iter()
            .map(|(id, tier)| (id.clone(), Tier::new(selector(&tier.claims))))
            .collect(),
        bands: figure.bands.as_ref().map(|bands| Bands {
            threshold: bands.threshold,
            unit: bands.unit.member().to_owned(),
        }),
        sections,
        population: figure.population.as_ref().map(selector),
        segments: figure
            .segments
            .iter()
            .map(|(id, segment)| (id.clone(), Segment::new(selector(&segment.claims))))
            .collect(),
        braces: figure
            .braces
            .iter()
            .map(|(id, brace)| (id.clone(), brace.members.clone()))
            .collect(),
    })
}

impl Stage {
    fn new(claims: Selector) -> Self {
        Self { claims }
    }
}

impl Tier {
    fn new(claims: Selector) -> Self {
        Self { claims }
    }
}

impl Segment {
    fn new(claims: Selector) -> Self {
        Self { claims }
    }
}

/// One generated selector as the computation's selector.
fn selector(typed: &vocab_proc::types::Selector) -> Selector {
    use vocab_proc::types::Selector as Typed;
    let members = |members: &[Typed]| members.iter().map(selector).collect();
    match typed {
        Typed::All(all) => Selector::All(members(&all.members)),
        Typed::Any(any) => Selector::Any(members(&any.members)),
        Typed::Not(not) => Selector::Not(members(&not.members)),
        Typed::System(system) => Selector::System(system.system.written().to_owned()),
        Typed::Under(under) => Selector::Under(
            endpoint_of(&under.concept).unwrap_or_default(),
        ),
        Typed::CodePrefix(prefix) => Selector::CodePrefix(prefix.prefixes.clone()),
        Typed::Class(class) => Selector::Class(
            class.classes.iter().filter_map(|c| endpoint_of(c)).collect(),
        ),
        Typed::MemberOf(set) => Selector::MemberOf(set.set.written().to_owned()),
        Typed::Collection(collection) => Selector::Collection(
            collection
                .collections
                .iter()
                .map(|c| c.written().to_owned())
                .collect(),
        ),
        Typed::Target(target) => Selector::Target {
            system: target.system.written().to_owned(),
            chain: target.chain == Some(true),
        },
        Typed::TargetStandard(_) => Selector::TargetStandard,
        Typed::Relationship(relationship) => Selector::Relationship(
            relationship
                .relationships
                .iter()
                .map(|r| r.member().to_owned())
                .collect(),
        ),
        Typed::Preference(preference) => {
            Selector::Preference(preference.preference.member().to_owned())
        }
        Typed::MinConfidence(confidence) => Selector::MinConfidence {
            score: confidence.score.as_str().to_owned(),
            scheme: confidence.scheme.written().to_owned(),
        },
        Typed::Method(method) => Selector::Method(method.methods.clone()),
        Typed::Imported(imported) => Selector::Imported {
            table: imported.table.written().to_owned(),
            labels: imported.labels.clone(),
        },
        Typed::TargetMemberOf(set) => Selector::TargetMemberOf(set.set.written().to_owned()),
    }
}
