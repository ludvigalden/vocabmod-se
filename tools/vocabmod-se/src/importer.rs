//! The import verb: operator-local sources rendered into KDL fragments
//! under a never-committed landing zone. The committed corpus stays
//! exactly what publication allows; the imported layer restores the
//! designation text locally, one fragment tree per source and release,
//! and `search`, `index`, and `omop-export` admit it through
//! `--imports` for merged readings (committed values win on conflict:
//! a duplicate identity across layers with differing content is a
//! finding, never a silent win).
//!
//! The landing zone is `imports/` at the repository root (outside the
//! repository's tracked surface); a run refuses any other target unless
//! `--allow-committed` names a `local/` profile document instead — both
//! gitignored. No network access, no committed write path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vocab_proc::{WriteDocument, WriteNode, WriteValue};

use crate::{CLEAN, FINDINGS, Paths, USAGE, fail};

/// The v1 import formats. Each reads an operator-local source and
/// projects concepts (code, optional parent, designations) into KDL
/// fragments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Claml,
    PdfText,
    Tdl,
    Csv,
    Rf2,
}

impl Format {
    pub fn parse(spelling: &str) -> Option<Self> {
        Some(match spelling {
            "claml" => Self::Claml,
            "pdf-text" => Self::PdfText,
            "tdl" => Self::Tdl,
            "csv" => Self::Csv,
            "rf2" => Self::Rf2,
            _ => return None,
        })
    }

    fn spelling(self) -> &'static str {
        match self {
            Self::Claml => "claml",
            Self::PdfText => "pdf-text",
            Self::Tdl => "tdl",
            Self::Csv => "csv",
            Self::Rf2 => "rf2",
        }
    }
}

pub struct Options {
    pub format: Format,
    pub source: String,
    pub into: Option<PathBuf>,
    pub system: String,
    pub release: String,
    pub manifest: bool,
    pub dry_run: bool,
    pub allow_local: bool,
    pub aliases: Vec<(String, PathBuf)>,
}

/// One projected concept from a source row: code, optional parent code
/// (within the imported system), and (locale, text) designations.
struct RawConcept {
    code: String,
    parent: Option<String>,
    names: Vec<(String, String)>,
}

pub fn run(paths: &Paths, options: &Options) -> ExitCode {
    match import(paths, options) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::from(CLEAN)
        }
        Err(error) => fail(&error),
    }
}

fn import(paths: &Paths, options: &Options) -> Result<String, String> {
    // Resolve the source: `$alias/rest` resolves NAME through an
    // --alias NAME=DIR binding; anything else is a plain path.
    let (source_path, source_spelling) = resolve_source(&options.source, &options.aliases)?;

    // Guard the write target: the landing zone or, with
    // --allow-committed, a gitignored local/ document root.
    let landing = paths.root.join("imports");
    let local_root = paths.root.join("local");
    let into = options.into.clone().unwrap_or_else(|| {
        landing
            .join(source_id(&source_path))
            .join(sanitize_release(&options.release))
    });
    guard_target(paths, &into, &landing, &local_root, options.allow_local)?;

    // Parse.
    let concepts = match options.format {
        Format::Claml => parse_claml(&source_path)?,
        Format::PdfText => parse_pdf_text(&source_path)?,
        Format::Tdl => parse_tdl(&source_path)?,
        Format::Csv => parse_csv(&source_path)?,
        Format::Rf2 => parse_rf2(&source_path)?,
    };
    if concepts.is_empty() {
        return Err(format!(
            "source {} yielded no concepts; refusing to write an empty import",
            source_path.display()
        ));
    }

    let digest = digest_path(&source_path)?;
    if options.dry_run {
        return Ok(format!(
            "dry-run: {} {} -> {} ({} concept(s), no files written)",
            options.format.spelling(),
            source_spelling,
            into.display(),
            concepts.len()
        ));
    }

    // Emit.
    std::fs::create_dir_all(into.join("concepts"))
        .map_err(|e| format!("creating {}: {e}", into.join("concepts").display()))?;
    let mut names_total = 0usize;
    let mut files = 0usize;
    for (i, chunk) in concepts.chunks(500).enumerate() {
        let first = &chunk.first().unwrap().code;
        let last = &chunk.last().unwrap().code;
        let path = into
            .join("concepts")
            .join(format!("{:04}-{}.kdl", i, file_stem(first, last)));
        let text = render_fragment(options, chunk, i)?;
        std::fs::write(&path, text).map_err(|e| format!("writing {}: {e}", path.display()))?;
        names_total += chunk.iter().map(|c| c.names.len()).sum::<usize>();
        files += 1;
    }
    let definitions = render_definitions(options);
    std::fs::write(into.join("definitions.kdl"), definitions)
        .map_err(|e| format!("writing {}: {e}", into.join("definitions.kdl").display()))?;
    if options.manifest {
        let text = render_manifest(
            options,
            &source_path,
            &digest,
            concepts.len(),
            names_total,
            files,
        );
        std::fs::write(into.join("manifest.kdl"), text)
            .map_err(|e| format!("writing {}: {e}", into.join("manifest.kdl").display()))?;
    }
    Ok(format!(
        "imported {} concept(s), {names_total} designation(s) into {} file(s) at {}",
        concepts.len(),
        files,
        into.display()
    ))
}

/// `$NAME/rest` resolves through an --alias NAME=DIR binding; unknown
/// aliases are usage errors.
fn resolve_source(
    source: &str,
    aliases: &[(String, PathBuf)],
) -> Result<(PathBuf, String), String> {
    if let Some(rest) = source.strip_prefix('$') {
        let (name, rest) = rest.split_once('/').unwrap_or((rest, ""));
        let Some((_, dir)) = aliases.iter().find(|(n, _)| n == name) else {
            return Err(format!(
                "unknown source alias \"{name}\"; bind one with --alias {name}=<dir>"
            ));
        };
        return Ok((dir.join(rest), source.to_owned()));
    }
    Ok((PathBuf::from(source), source.to_owned()))
}

/// Only the landing zone (and, explicitly, a gitignored `local/`
/// document root) may receive imports.
fn guard_target(
    paths: &Paths,
    into: &Path,
    landing: &Path,
    local_root: &Path,
    allow_local: bool,
) -> Result<(), String> {
    // Resolve relative targets against the working directory so the
    // guard judges the real landing path.
    let into = if into.is_absolute() {
        into.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("resolving --into: {e}"))?
            .join(into)
    };
    fn inside(dir: &Path, base: &Path) -> bool {
        dir.starts_with(base)
    }
    // A directory outside the repository is the operator's own surface;
    // the guard protects the tracked tree, not the machine.
    if !inside(&into, &paths.root) {
        return Ok(());
    }
    if inside(&into, landing) {
        return Ok(());
    }
    if allow_local && inside(&into, local_root) {
        return Ok(());
    }
    Err(format!(
        "refusing --into {}: imports land only under {} (or, with --allow-committed, under {}); \
         the committed tree is never an import target",
        into.display(),
        landing.display(),
        local_root.display()
    ))
}

// ---------------------------------------------------------------------
// Rendering

fn render_definitions(options: &Options) -> String {
    let mut doc = WriteDocument::new();
    doc.comment(format!(
        "Imported layer for system {}: one operator-local source, one release.\n\
         Produced by the import verb; never committed. Its designations come\n\
         straight from the operator-local source named in manifest.kdl.",
        options.system
    ));
    doc.node(
        WriteNode::new("system")
            .arg(options.system.as_str())
            .prop("kind", WriteValue::identifier("classification"))
            .prop("distribution", WriteValue::identifier("local-only"))
            .child(
                WriteNode::new("name")
                    .arg("en")
                    .arg(format!("imported layer: {} {}", options.system, options.release)),
            ),
    );
    doc.node(
        WriteNode::new("release")
            .arg(options.release.as_str())
            .prop("system", options.system.as_str())
            .prop("label-source", WriteValue::identifier("registry")),
    );
    doc.node(context_block(options));
    doc.node(
        WriteNode::new("import")
            .arg("concepts/**/*.kdl")
            .prop("kind", WriteValue::identifier("fragment")),
    );
    doc.render()
}

/// The context block every imported document opens with.
fn context_block(options: &Options) -> WriteNode {
    WriteNode::new("context").child(
        WriteNode::new("system")
            .arg(options.system.as_str())
            .prop("release", options.release.as_str()),
    )
}

fn render_fragment(
    options: &Options,
    chunk: &[RawConcept],
    index: usize,
) -> Result<String, String> {
    let mut doc = WriteDocument::new();
    doc.comment(format!(
        "Imported layer: {} {}, fragment {} (codes {} .. {})",
        options.system,
        options.release,
        index,
        one_line(&chunk.first().unwrap().code),
        one_line(&chunk.last().unwrap().code)
    ));
    doc.node(context_block(options));
    for concept in chunk {
        let mut node = WriteNode::new("concept").arg(one_line(&concept.code));
        if let Some(parent) = &concept.parent {
            node = node.prop("parent", one_line(parent));
        }
        for (locale, text) in &concept.names {
            node = node.child(
                WriteNode::new("name")
                    .arg(one_line(locale))
                    .arg(one_line(text)),
            );
        }
        doc.node(node);
    }
    Ok(doc.render())
}

fn render_manifest(
    options: &Options,
    source: &Path,
    digest: &str,
    concepts: usize,
    names: usize,
    files: usize,
) -> String {
    let mut doc = WriteDocument::new();
    doc.comment("Import manifest: what this imported layer was built from.");
    doc.node(
        WriteNode::new("import-manifest")
            .arg(sanitize_release(&options.release))
            .prop("version", "1")
            .prop("system", options.system.as_str())
            .prop("release", sanitize_release(&options.release))
            .child(WriteNode::new("format").arg(options.format.spelling()))
            .child(WriteNode::new("source").arg(source.display().to_string()))
            .child(WriteNode::new("digest").arg(digest))
            .child(WriteNode::new("concepts").arg(concepts))
            .child(WriteNode::new("designations").arg(names))
            .child(WriteNode::new("fragments").arg(files)),
    );
    doc.render()
}

/// Source text folded onto one line: newlines and tabs become spaces so
/// a designation stays one line. Quoting is the KDL writer's.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|ch| if matches!(ch, '\n' | '\r' | '\t') { ' ' } else { ch })
        .collect()
}

fn file_stem(first: &str, last: &str) -> String {
    let sanitize = |s: &str| {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
    };
    format!("{}-{}", sanitize(first), sanitize(last))
}

fn sanitize_release(release: &str) -> String {
    release
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn source_id(path: &Path) -> String {
    let raw = path
        .file_stem()
        .or_else(|| path.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_owned());
    sanitize_release(&raw)
}

/// FNV-1a 64-bit over the file bytes (a directory hashes its files in
/// sorted relative-path order) — a plain change-detection digest, not a
/// cryptographic one.
fn digest_path(path: &Path) -> Result<String, String> {
    const PRIME: u64 = 0x100000001b3;
    fn feed(hash: u64, bytes: &[u8]) -> u64 {
        let mut hash = hash;
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    }
    let mut hash = 0xcbf29ce484222325;
    if path.is_dir() {
        let mut files: Vec<PathBuf> = walk_files(path)?;
        files.sort();
        for f in files {
            let rel = f.strip_prefix(path).unwrap_or(&f).display().to_string();
            hash = feed(hash, rel.as_bytes());
            hash = feed(hash, b"\0");
            let bytes = std::fs::read(&f).map_err(|e| format!("reading {}: {e}", f.display()))?;
            hash = feed(hash, &bytes);
        }
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        hash = feed(hash, &bytes);
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

fn walk_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path)?);
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------
// Parsers

/// ClaML (WHO ICD XML): `Class` codes with `SuperClass` parents and
/// `Rubric kind="preferred"` labels. Other rubric kinds are not
/// projected by v1.
fn parse_claml(path: &Path) -> Result<Vec<RawConcept>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut concepts = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut xml = XmlReader::new(&text);
    let mut current: Option<(String, Option<String>)> = None;
    let mut rubric_kind: Option<String> = None;
    let mut rubric_lang: Option<String> = None;
    let mut label_open = false;
    let mut label_text = String::new();
    while let Some(event) = xml.next()? {
        match event {
            XmlEvent::Start(tag, attrs) => match tag.as_str() {
                "Class" => {
                    let code = attrs
                        .iter()
                        .find(|(k, _)| k == "code")
                        .map(|(_, v)| v.clone());
                    if let Some(code) = code {
                        let known = concepts.contains_key(&code);
                        if !known {
                            order.push(code.clone());
                        }
                        current = Some((code, None));
                    }
                }
                "SuperClass" => {
                    if let Some((code, parent)) = current.as_mut() {
                        let _ = code;
                        let super_code = attrs
                            .iter()
                            .find(|(k, _)| k == "code")
                            .or_else(|| attrs.iter().find(|(k, _)| k == "coderef"))
                            .map(|(_, v)| v.clone());
                        *parent = super_code;
                    }
                }
                "Rubric" => {
                    rubric_kind = attrs
                        .iter()
                        .find(|(k, _)| k == "kind" || k == "name")
                        .map(|(_, v)| v.clone());
                    rubric_lang = None;
                    label_open = false;
                    label_text.clear();
                }
                "Label" => {
                    rubric_lang = attrs
                        .iter()
                        .find(|(k, _)| k == "xml:lang" || k == "lang")
                        .map(|(_, v)| v.clone());
                    label_open = true;
                    label_text.clear();
                }
                "Para" => {
                    if label_open && !label_text.is_empty() {
                        label_text.push(' ');
                    }
                }
                _ => {}
            },
            XmlEvent::Text(chunk) => {
                if label_open {
                    label_text.push_str(&chunk);
                }
            }
            XmlEvent::End(tag) => match tag.as_str() {
                "Label" => {
                    label_open = false;
                }
                "Rubric" => {
                    if rubric_kind.as_deref() == Some("preferred") {
                        if let Some((code, _)) = &current {
                            let text = decode_entities(label_text.trim());
                            if !text.is_empty() {
                                let locale = rubric_lang.clone().unwrap_or_else(|| "en".into());
                                let entry =
                                    concepts.entry(code.clone()).or_insert_with(|| RawConcept {
                                        code: code.clone(),
                                        parent: None,
                                        names: Vec::new(),
                                    });
                                entry.names.push((locale, text));
                            }
                        }
                    }
                    rubric_kind = None;
                    label_text.clear();
                }
                "Class" => {
                    if let Some((code, parent)) = current.take() {
                        let entry = concepts.entry(code.clone()).or_insert_with(|| RawConcept {
                            code: code.clone(),
                            parent: None,
                            names: Vec::new(),
                        });
                        entry.parent = parent;
                    }
                }
                _ => {}
            },
        }
    }
    Ok(order
        .into_iter()
        .filter_map(|c| concepts.remove(&c))
        .collect())
}

/// pdftotext -layout output of a classification chapter: a designation
/// line is a classification code followed by two or more spaces and the
/// title. Page furniture (bare numbers/roman numerals, form feeds) and
/// include/exclude annotation blocks are skipped.
fn parse_pdf_text(path: &Path) -> Result<Vec<RawConcept>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut out: Vec<RawConcept> = Vec::new();
    let mut seen = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.starts_with('\u{c}') {
            continue;
        }
        let trimmed = line.trim();
        // Page furniture: bare arabic or roman page numbers.
        if is_page_number(trimmed) {
            continue;
        }
        let Some((code_raw, title)) = split_code_line(trimmed) else {
            continue;
        };
        let code = code_raw.trim_end_matches(['†', '*', '§']).trim().to_owned();
        if code.is_empty() || title.is_empty() {
            continue;
        }
        // Annotation keywords start include/exclude blocks, whose lines
        // sometimes carry codes; treat the keyword-led line as noise.
        if title.starts_with("Utesluter")
            || title.starts_with("Inkluderar")
            || title.starts_with("Excludes")
            || title.starts_with("Includes")
        {
            continue;
        }
        let next = out.len();
        let idx = *seen.entry(code.clone()).or_insert(next);
        if idx == next {
            out.push(RawConcept {
                code: code.clone(),
                parent: None,
                names: Vec::new(),
            });
        }
        let entry = &mut out[idx];
        let text = decode_entities(title.trim()).to_owned();
        if !entry.names.iter().any(|(_, t)| t == &text) {
            entry.names.push(("sv".into(), text));
        }
    }
    Ok(out)
}

fn is_page_number(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_punctuation())
        || line
            .chars()
            .all(|c| c.is_ascii_digit() || "ivxlcdmIVXLCDM".contains(c))
}

fn split_code_line(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let head = &line[..i];
    if head.is_empty() || !head.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let code_ok = {
        let mut chars = head.chars();
        let first = chars.next()?;
        first.is_ascii_uppercase()
            && chars.next().is_some_and(|c| c.is_ascii_digit())
            && head[1..]
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_alphanumeric() || c == '.')
    };
    if !code_ok {
        return None;
    }
    let rest = line[i..].trim();
    // Titles follow their code with real column separation; single
    // spaces usually mean the token is a word, not a code.
    if line.len() <= i + 1 || !bytes[i].is_ascii_whitespace() {
        return None;
    }
    let gap = line[i..].len() - line[i..].trim_start().len();
    if gap < 2 {
        return None;
    }
    if rest.is_empty() {
        return None;
    }
    Some((head, rest))
}

/// TDL: `!columns` header plus `!row` records, tab-delimited. v1
/// projects the `id` column and the l10n `name` column, whose parts are
/// split on `|`, each part either plain (en) or `@<locale>: <text>`.
fn parse_tdl(path: &Path) -> Result<Vec<RawConcept>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut columns: Vec<String> = Vec::new();
    let mut id_col = None;
    let mut name_col = None;
    let mut parent_col = None;
    let mut out: Vec<RawConcept> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("!columns") {
            let rest = rest.trim_start();
            let rest = rest.strip_prefix(":schema").unwrap_or(rest);
            let parts: Vec<String> = rest
                .split('\t')
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty())
                .collect();
            if !parts.is_empty() && parts[0] != "schema" {
                columns = parts;
                id_col = columns.iter().position(|c| c == "id" || c == "kod");
                name_col = columns.iter().position(|c| c == "name");
                parent_col = columns.iter().position(|c| c == "parent");
            }
            continue;
        }
        if !line.starts_with("!row") {
            continue;
        }
        let (Some(id_col), Some(name_col)) = (id_col, name_col) else {
            return Err("tdl source carries no usable !columns header with id and name".into());
        };
        let rest = line.strip_prefix("!row").unwrap_or_default().trim_start();
        let cells: Vec<&str> = rest.split('\t').collect();
        let Some(code) = cells.get(id_col).map(|c| c.trim()) else {
            continue;
        };
        if code.is_empty() {
            continue;
        }
        let mut names = Vec::new();
        if let Some(raw) = cells.get(name_col) {
            for part in raw.split('|') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                // Each part is either plain (en) or "en text @<loc>: <text>".
                if let Some((before, after)) = part.split_once(" @") {
                    let before = before.trim();
                    if !before.is_empty() {
                        names.push(("en".into(), before.to_owned()));
                    }
                    match after.split_once(':') {
                        Some((locale, text)) => {
                            let locale = locale.trim().to_lowercase();
                            let text = text.trim();
                            if !text.is_empty() && !locale.is_empty() && locale.len() <= 8 {
                                names.push((locale, text.to_owned()));
                            } else if !text.is_empty() {
                                names.push(("en".into(), text.to_owned()));
                            }
                        }
                        None => names.push(("en".into(), after.trim().to_owned())),
                    }
                } else {
                    names.push(("en".into(), part.to_owned()));
                }
            }
        }
        let parent = parent_col
            .and_then(|c| cells.get(c))
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        out.push(RawConcept {
            code: code.to_owned(),
            parent,
            names,
        });
    }
    Ok(out)
}

/// CSV/TSV with a header: `code` (or `kod`), optional `parent`, and
/// designation columns `name_sv` / `name_en` (`sv` / `en` spellings
/// accepted). The delimiter is sniffed from the header line.
fn parse_csv(path: &Path) -> Result<Vec<RawConcept>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or_else(|| "csv source is empty".to_owned())?;
    let delimiter = sniff_delimiter(header);
    let columns: Vec<String> = header
        .split(delimiter)
        .map(|c| c.trim().trim_matches('"').to_lowercase().to_owned())
        .collect();
    let code_col = columns
        .iter()
        .position(|c| c == "code" || c == "kod" || c == "id")
        .ok_or_else(|| "csv source carries no code/kod/id column".to_owned())?;
    let parent_col = columns
        .iter()
        .position(|c| c == "parent" || c == "förälder");
    let name_cols: Vec<(String, usize)> = columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = c
                .strip_prefix("name_")
                .or_else(|| c.strip_prefix("name:"))
                .unwrap_or_else(|| {
                    if c.len() == 2 && c != "id" && c != "kod" {
                        c
                    } else {
                        ""
                    }
                });
            (!name.is_empty()).then(|| (name.to_owned(), i))
        })
        .collect();
    if name_cols.is_empty() {
        return Err("csv source carries no designation column (name_<locale> or <locale>)".into());
    }
    let mut out = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split(delimiter).collect();
        let Some(code) = cells.get(code_col).map(|c| c.trim().trim_matches('"')) else {
            continue;
        };
        if code.is_empty() {
            continue;
        }
        let names = name_cols
            .iter()
            .filter_map(|(locale, i)| {
                cells
                    .get(*i)
                    .map(|c| c.trim().trim_matches('"'))
                    .and_then(|text| (!text.is_empty()).then(|| (locale.clone(), text.to_owned())))
            })
            .collect();
        let parent = parent_col
            .and_then(|c| cells.get(c))
            .map(|c| c.trim().trim_matches('"'))
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        out.push(RawConcept {
            code: code.to_owned(),
            parent,
            names,
        });
    }
    Ok(out)
}

fn sniff_delimiter(header: &str) -> char {
    let counts = [
        (';', header.matches(';').count()),
        ('\t', header.matches('\t').count()),
        (',', header.matches(',').count()),
    ];
    counts
        .iter()
        .max_by_key(|(_, n)| *n)
        .and_then(|(c, n)| if *n > 0 { Some(*c) } else { None })
        .unwrap_or(',')
}

/// RF2 snapshot directory (unzipped): active concepts from
/// `sct2_Concept_Snapshot*.csv`, designations from active
/// `sct2_Description_Snapshot*.csv` rows (FSN loses its semantic tag).
fn parse_rf2(dir: &Path) -> Result<Vec<RawConcept>, String> {
    if !dir.is_dir() {
        return Err(format!(
            "rf2 source {} is not a directory; v1 reads an unzipped RF2 snapshot",
            dir.display()
        ));
    }
    let files = walk_files(dir)?;
    let concept_file = files
        .iter()
        .find(|f| {
            f.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sct2_Concept_Snapshot"))
        })
        .ok_or_else(|| "rf2 source carries no sct2_Concept_Snapshot file".to_owned())?;
    let description_file = files
        .iter()
        .find(|f| {
            f.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sct2_Description_Snapshot"))
        })
        .ok_or_else(|| "rf2 source carries no sct2_Description_Snapshot file".to_owned())?;

    const FSN: &str = "900000000000003001";
    let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut names: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    let text = std::fs::read_to_string(description_file)
        .map_err(|e| format!("reading {}: {e}", description_file.display()))?;
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() < 9 {
            continue;
        }
        // id, effectiveTime, active, moduleId, conceptId, languageCode,
        // typeId, term, caseSignificanceId
        if cells[2] != "1" {
            continue;
        }
        let concept = cells[4].to_owned();
        let locale = cells[5].to_lowercase();
        let type_id = cells[6];
        let mut term = cells[7].to_owned();
        if type_id == FSN {
            // FSN text ends with " (semantic tag)".
            if term.rfind(" (").is_some_and(|i| term.ends_with(')')) {
                term.truncate(term.rfind(" (").unwrap());
            }
        } else {
            continue; // v1: FSN as the designation; synonyms listed once
        }
        let list = names.entry(concept).or_default();
        if !list.iter().any(|(_, t)| t == &term) {
            list.push((locale, term));
        }
    }
    // Synonyms, kept once per (concept, locale, term) beyond the FSN.
    let text = std::fs::read_to_string(description_file)
        .map_err(|e| format!("reading {}: {e}", description_file.display()))?;
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() < 9 || cells[2] != "1" {
            continue;
        }
        if cells[6] != "900000000000013009" {
            continue;
        }
        let concept = cells[4].to_owned();
        let locale = cells[5].to_lowercase();
        let term = cells[7].to_owned();
        let list = names.entry(concept).or_default();
        if !list.iter().any(|(_, t)| t == &term) {
            list.push((locale, term));
        }
    }

    let text = std::fs::read_to_string(concept_file)
        .map_err(|e| format!("reading {}: {e}", concept_file.display()))?;
    let mut order: Vec<String> = Vec::new();
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() < 5 {
            continue;
        }
        // id, effectiveTime, active, moduleId, definitionStatusId
        if cells[2] != "1" {
            continue;
        }
        order.push(cells[0].to_owned());
        parents.entry(cells[0].to_owned()).or_default();
    }
    let _ = &mut parents;
    let mut out: Vec<RawConcept> = Vec::new();
    for code in order {
        let names = names.remove(&code).unwrap_or_default();
        out.push(RawConcept {
            code,
            parent: None,
            names,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------
// A minimal XML pull reader (the ClaML subset: nested elements, quoted
// attributes, entities, comments, a DOCTYPE line).

enum XmlEvent {
    Start(String, Vec<(String, String)>),
    End(String),
    Text(String),
}

struct XmlReader<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> XmlReader<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn next(&mut self) -> Result<Option<XmlEvent>, String> {
        while self.pos < self.text.len() {
            let rest = &self.text[self.pos..];
            if let Some(open) = rest.find('<') {
                if open > 0 {
                    let chunk = rest[..open].to_owned();
                    self.pos += open;
                    if !chunk.trim().is_empty() {
                        return Ok(Some(XmlEvent::Text(chunk)));
                    }
                }
                self.pos += 1;
                let rest = &self.text[self.pos..];
                if rest.starts_with("!--") {
                    let Some(end) = rest.find("-->") else {
                        return Err("unterminated XML comment".into());
                    };
                    self.pos += end + 3;
                    continue;
                }
                if rest.starts_with('!') || rest.starts_with('?') {
                    let Some(end) = rest.find('>') else {
                        return Err("unterminated XML declaration".into());
                    };
                    self.pos += end + 1;
                    continue;
                }
                let Some(end) = rest.find('>') else {
                    return Err("unterminated XML tag".into());
                };
                let tag_text = &rest[..end];
                self.pos += end + 1;
                if let Some(name) = tag_text.strip_prefix('/') {
                    return Ok(Some(XmlEvent::End(name.trim().to_owned())));
                }
                let self_closing = tag_text.ends_with('/');
                let tag_text = tag_text.strip_suffix('/').unwrap_or(tag_text);
                let mut parts = tag_text.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or_default().to_owned();
                let attrs = parse_attrs(parts.next().unwrap_or_default());
                if self_closing {
                    // Report as start+end so nesting trackers stay simple.
                    return Ok(Some(XmlEvent::Start(name.clone(), attrs)));
                }
                return Ok(Some(XmlEvent::Start(name, attrs)));
            }
            // Trailing text after the last element.
            let chunk = rest.trim().to_owned();
            self.pos = self.text.len();
            if !chunk.is_empty() {
                return Ok(Some(XmlEvent::Text(chunk)));
            }
        }
        Ok(None)
    }
}

fn parse_attrs(text: &str) -> Vec<(String, String)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // attribute name up to '='
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let name = text[name_start..i].to_owned();
        while i < bytes.len() && bytes[i] != b'=' && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            // Attribute without a value: skip it.
            continue;
        }
        i += 1; // '='
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let quote = bytes[i];
            i += 1;
            let value_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let value = text[value_start..i].to_owned();
            i += 1; // closing quote
            value
        } else {
            let value_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            text[value_start..i].to_owned()
        };
        out.push((name, value));
    }
    out
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        if end > 8 {
            out.push('&');
            rest = &rest[1..];
            continue;
        }
        match &rest[1..end] {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            entity if entity.starts_with("#x") || entity.starts_with("#X") => {
                if let Ok(code) = u32::from_str_radix(&entity[2..], 16) {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            entity if entity.starts_with('#') => {
                if let Ok(code) = entity[1..].parse::<u32>() {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            other => out.push_str(&format!("&{other};")),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}
