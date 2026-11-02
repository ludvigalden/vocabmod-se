//! Figure rendering next to the data writer: the display policy of the
//! tiered coverage figure (which rows a rendering shows, published as
//! `display` rows of the figure data file so every renderer reads the
//! same policy) and its deterministic SVG, set as real text with an
//! embedded style that follows the reader's light or dark scheme. Only
//! the computed rows feed it; nothing here carries a number.

use std::collections::BTreeMap;

use crate::figures_compute::DataRow;
use crate::figures_model::Figure;

/// The display policy of one rendered figure: rows whose denominator
/// falls below `min_codes` are omitted as small, and the named rows are
/// omitted for the stated reason (`custom` concepts, or `out-of-scope`,
/// which also leaves the row out of the in-scope totals).
struct Policy {
    figure: &'static str,
    min_codes: usize,
    omit: &'static [(&'static str, &'static str)],
}

const POLICIES: &[Policy] = &[Policy {
    figure: "vocab-coverage-standard",
    min_codes: 50,
    omit: &[("geo-se", "custom"), ("tlv-vnr-dose-form", "out-of-scope")],
}];

/// The segment order of the tiered bar, strongest first, as the
/// computation names them.
const SEGMENTS: [&str; 6] = [
    "high-direct",
    "high-hop",
    "medium-direct",
    "medium-hop",
    "low-direct",
    "low-hop",
];

/// One row as a rendering reads it.
struct Shown {
    label: String,
    denominator: usize,
    covered: usize,
    segments: [usize; 6],
}

/// The `display` rows of a figure under its policy, one per configured
/// row: item `shown` or `omitted`, the note naming the omission reason
/// (`small`, `custom`, `out-of-scope`). None for a figure without one.
pub fn display_rows(figure: &Figure, rows: &[DataRow]) -> Option<Vec<DataRow>> {
    let policy = POLICIES.iter().find(|policy| policy.figure == figure.id)?;
    let mut out = Vec::new();
    for (section_id, section) in &figure.sections {
        for (row_id, row) in &section.rows {
            let denominator = rows
                .iter()
                .find(|data| {
                    &data.section == section_id && &data.row == row_id && data.measure == "denominator"
                })
                .and_then(|data| data.denominator.parse::<usize>().ok());
            let reason = policy
                .omit
                .iter()
                .find(|(id, _)| id == row_id)
                .map(|(_, reason)| (*reason).to_owned())
                .or_else(|| {
                    denominator
                        .is_some_and(|d| d < policy.min_codes)
                        .then(|| "small".to_owned())
                });
            out.push(DataRow {
                section: section_id.clone(),
                row: row_id.clone(),
                label: row.label.clone(),
                track: String::new(),
                measure: "display".to_owned(),
                item: if reason.is_some() { "omitted" } else { "shown" }.to_owned(),
                denominator: String::new(),
                numerator: String::new(),
                share_pct: String::new(),
                note: reason.unwrap_or_default(),
            });
        }
    }
    Some(out)
}

/// The shown rows in configuration order, from the data rows alone.
fn shown_rows(rows: &[DataRow]) -> Vec<Shown> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut by_row: BTreeMap<(String, String), Shown> = BTreeMap::new();
    for data in rows.iter().filter(|data| data.measure == "display" && data.item == "shown") {
        let key = (data.section.clone(), data.row.clone());
        order.push(key.clone());
        by_row.insert(
            key,
            Shown {
                label: data.label.clone(),
                denominator: 0,
                covered: 0,
                segments: [0; 6],
            },
        );
    }
    for data in rows {
        let Some(shown) = by_row.get_mut(&(data.section.clone(), data.row.clone())) else {
            continue;
        };
        let value = data.numerator.parse::<usize>().unwrap_or(0);
        match (data.measure.as_str(), data.item.as_str()) {
            ("denominator", _) => shown.denominator = value,
            ("stage", "target-standard") => shown.covered = value,
            ("segment", item) => {
                if let Some(position) = SEGMENTS.iter().position(|s| *s == item) {
                    shown.segments[position] = value;
                }
            }
            _ => {}
        }
    }
    order
        .into_iter()
        .filter_map(|key| by_row.remove(&key))
        .collect()
}

/// Thousands separated by commas.
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A share in tenths of a percent, rounded half up, in integers.
fn tenths(numerator: usize, denominator: usize) -> String {
    if denominator == 0 {
        return String::new();
    }
    let tenths = (numerator * 2000 + denominator) / (2 * denominator);
    format!("{}.{}", tenths / 10, tenths % 10)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Two decimals, trailing zeros kept: stable bytes across platforms.
fn px(value: f64) -> String {
    format!("{value:.2}")
}

// Geometry, in px of the viewBox.
const WIDTH: f64 = 760.0;
const NAME_X: f64 = 0.0;
const COVERED_R: f64 = 262.0;
const CODES_R: f64 = 326.0;
const PCT_R: f64 = 374.0;
const BAR_X: f64 = 400.0;
const BAR_W: f64 = 360.0;
const ROW_H: f64 = 24.0;
const BAR_H: f64 = 12.0;
const GAP: f64 = 2.0;
const RULE_PAD: f64 = 9.0;

/// The style block: palette roles as custom properties, one light and
/// one dark set (validated against #ffffff and #0d1117), so ink, track,
/// tier shades, rules, and hatch all switch with the reader's scheme.
const STYLE: &str = r#"<style>
svg { --ink: #1f2328; --ink-2: #59636e; --rule: #d1d9e0; --surface: #ffffff;
  --high: #17506d; --medium: #4f86a6; --low: #8fb3c9; --track: #e6edf2; }
@media (prefers-color-scheme: dark) {
  svg { --ink: #e6edf3; --ink-2: #b1bac4; --rule: #3d444d; --surface: #0d1117;
    --high: #b5d4e6; --medium: #6f9fbd; --low: #3b6a88; --track: #1d2731; }
}
text { font-family: "Source Sans 3", "Source Sans Pro", system-ui, -apple-system, "Segoe UI", Helvetica, Arial, sans-serif;
  font-size: 13px; fill: var(--ink); font-variant-numeric: tabular-nums; }
.head { font-family: "Source Code Pro", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 11.5px; font-weight: 600; letter-spacing: -0.06em; fill: var(--ink-2); }
.legend { fill: var(--ink-2); }
.total { font-weight: 600; }
.rule { stroke: var(--rule); stroke-width: 1; }
.high { fill: var(--high); } .medium { fill: var(--medium); } .low { fill: var(--low); }
.track { fill: var(--track); }
.hatch-line { stroke: var(--surface); stroke-width: 1.4; }
</style>"#;

/// The tiered coverage figure as SVG: legend, one row per shown system
/// (label, covered, codes, share, bar) and a total row, between
/// recessive rules with breathing room. Deterministic for equal input.
pub fn coverage_svg(rows: &[DataRow]) -> String {
    let shown = shown_rows(rows);
    let tier_class = ["high", "high", "medium", "medium", "low", "low"];
    let mut body = String::new();

    // Legend: the tier ladder, then the route encoding.
    let mut y = 14.0;
    let mut x = 0.0;
    for (class, label, width) in [
        ("high", "High tier", 74.0),
        ("medium", "Medium tier", 92.0),
        ("low", "Below medium or unassessed", 186.0),
        ("track", "No standard target", 140.0),
    ] {
        body.push_str(&format!(
            "<rect class=\"{class}\" x=\"{}\" y=\"{}\" width=\"12\" height=\"12\" rx=\"2\"/>\
             <text class=\"legend\" x=\"{}\" y=\"{}\">{label}</text>\n",
            px(x),
            px(y - 10.5),
            px(x + 18.0),
            px(y),
        ));
        x += width;
    }
    y += 22.0;
    body.push_str(&format!(
        "<rect class=\"high\" x=\"0.00\" y=\"{}\" width=\"12\" height=\"12\" rx=\"2\"/>\
         <rect fill=\"url(#hatch)\" x=\"0.00\" y=\"{}\" width=\"12\" height=\"12\" rx=\"2\"/>\
         <text class=\"legend\" x=\"18.00\" y=\"{}\">Hatched: reaches the standard concept through one “Maps to” step; solid: direct standard target</text>\n",
        px(y - 10.5),
        px(y - 10.5),
        px(y),
    ));

    // Column heads over the first rule.
    y += 34.0;
    for (anchor, x, label) in [
        ("start", NAME_X, "System"),
        ("end", COVERED_R, "Covered"),
        ("end", CODES_R, "Codes"),
        ("end", PCT_R, "%"),
        ("start", BAR_X, "Coverage by tier"),
    ] {
        body.push_str(&format!(
            "<text class=\"head\" x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\">{label}</text>\n",
            px(x),
            px(y)
        ));
    }
    y += RULE_PAD - 2.0;
    body.push_str(&format!(
        "<line class=\"rule\" x1=\"0\" x2=\"{}\" y1=\"{}\" y2=\"{}\"/>\n",
        px(WIDTH),
        px(y + 0.5),
        px(y + 0.5)
    ));
    y += RULE_PAD;

    let mut totals = Shown {
        label: format!("Total, {} systems", shown.len()),
        denominator: 0,
        covered: 0,
        segments: [0; 6],
    };
    let mut bar_row = |body: &mut String, index: usize, row: &Shown, y: f64, total: bool| {
        let baseline = y + ROW_H / 2.0 + 4.5;
        let class = if total { " class=\"total\"" } else { "" };
        body.push_str(&format!(
            "<g><text{class} x=\"{}\" y=\"{}\">{}</text>\n",
            px(NAME_X),
            px(baseline),
            escape(&row.label)
        ));
        // Segments in tier order with a 2px surface gap between drawn
        // parts; a part under 1px draws nothing and takes no gap.
        let bar_y = y + (ROW_H - BAR_H) / 2.0;
        body.push_str(&format!(
            "<clipPath id=\"bar-{index}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\"/></clipPath>\
             <g clip-path=\"url(#bar-{index})\">",
            px(BAR_X),
            px(bar_y),
            px(BAR_W),
            px(BAR_H)
        ));
        let denominator = row.denominator.max(1) as f64;
        let mut parts: Vec<(usize, f64)> = row
            .segments
            .iter()
            .enumerate()
            .map(|(position, &count)| (position, BAR_W * count as f64 / denominator))
            .filter(|(_, width)| *width >= 1.0)
            .collect();
        let rest = BAR_W * (row.denominator.saturating_sub(row.segments.iter().sum())) as f64 / denominator;
        if rest >= 1.0 {
            parts.push((SEGMENTS.len(), rest));
        }
        let gaps = GAP * parts.len().saturating_sub(1) as f64;
        let scale = (BAR_W - gaps) / BAR_W;
        let mut cursor = BAR_X;
        for (position, width) in &parts {
            let width = width * scale;
            let class = tier_class.get(*position).copied().unwrap_or("track");
            body.push_str(&format!(
                "<rect class=\"{class}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                px(cursor),
                px(bar_y),
                px(width),
                px(BAR_H)
            ));
            if *position < SEGMENTS.len() && position % 2 == 1 {
                body.push_str(&format!(
                    "<rect fill=\"url(#hatch)\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                    px(cursor),
                    px(bar_y),
                    px(width),
                    px(BAR_H)
                ));
            }
            cursor += width + GAP;
        }
        body.push_str("</g>\n");
        for (x, value) in [
            (COVERED_R, grouped(row.covered)),
            (CODES_R, grouped(row.denominator)),
            (PCT_R, tenths(row.covered, row.denominator)),
        ] {
            body.push_str(&format!(
                "<text{class} x=\"{}\" y=\"{}\" text-anchor=\"end\">{value}</text>\n",
                px(x),
                px(baseline)
            ));
        }
        body.push_str("</g>\n");
    };
    for (index, row) in shown.iter().enumerate() {
        bar_row(&mut body, index, row, y, false);
        y += ROW_H;
        totals.denominator += row.denominator;
        totals.covered += row.covered;
        for (sum, part) in totals.segments.iter_mut().zip(row.segments) {
            *sum += part;
        }
    }
    y += RULE_PAD;
    body.push_str(&format!(
        "<line class=\"rule\" x1=\"0\" x2=\"{}\" y1=\"{}\" y2=\"{}\"/>\n",
        px(WIDTH),
        px(y + 0.5),
        px(y + 0.5)
    ));
    y += RULE_PAD;
    bar_row(&mut body, shown.len(), &totals, y, true);
    y += ROW_H + 4.0;

    let height = y.ceil();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" \
         role=\"img\" aria-label=\"Coverage and confidence of the vocabmod-se mappings, by system\">\n\
         {STYLE}\n\
         <defs><pattern id=\"hatch\" patternUnits=\"userSpaceOnUse\" width=\"5\" height=\"5\" patternTransform=\"rotate(45)\">\
         <line class=\"hatch-line\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"5\"/></pattern></defs>\n\
         {body}</svg>\n",
        w = WIDTH as usize,
        h = height as usize,
    )
}
