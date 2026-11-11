#import "tokens.typ": *

// Native renderings of the tool's figure data files (the figures verb
// writes them under assets/figures/dist/). Every number is read from the
// file with csv(); nothing here carries one. The figure is a real
// table set in real text, one row per system in reading order (system,
// covered, codes, %, bar), so selecting it copies as a table.

// Tier shades: one hue, light to dark (validated as an ordinal ramp
// against the white paper), the accent at the high end. The hatch is
// the route channel and survives grayscale and colour-vision loss.
#let fig-high = rgb("#17506d")
#let fig-medium = rgb("#4f86a6")
#let fig-low = rgb("#8fb3c9")
#let fig-track = rgb("#e6edf2")
#let fig-rule = 0.5pt + rgb("#c9d3db")
#let fig-gap = 1.5pt // the 2px surface gap at print scale
#let fig-bar-height = 7.5pt
#let fig-size = 9.5pt
#let fig-head-size = 8.5pt
#let fig-rule-pad = 6pt // breathing room between a rule and its rows

#let fig-hatch = tiling(size: (4pt, 4pt), {
  let s = 0.9pt + white
  place(line(start: (0pt, 4pt), end: (4pt, 0pt), stroke: s))
  place(line(start: (-1pt, 1pt), end: (1pt, -1pt), stroke: s))
  place(line(start: (3pt, 5pt), end: (5pt, 3pt), stroke: s))
})

// The segment order of the tiered bar, strongest first, as the figure
// data names them; odd positions are the one-hop routes.
#let coverage-segments = ("high-direct", "high-hop", "medium-direct", "medium-hop", "low-direct", "low-hop")
#let coverage-fills = (fig-high, fig-high, fig-medium, fig-medium, fig-low, fig-low)

// Thousands grouped with commas; a share in tenths of a percent,
// rounded half up in integer arithmetic (as the tool's SVG does).
#let grouped(n) = {
  let digits = str(n)
  let out = ""
  for (i, ch) in digits.clusters().enumerate() {
    if i > 0 and calc.rem(digits.len() - i, 3) == 0 { out += "," }
    out += ch
  }
  out
}
#let tenths(num, den) = {
  if den == 0 { return "" }
  let t = calc.quo(num * 2000 + den, 2 * den)
  str(calc.quo(t, 10)) + "." + str(calc.rem(t, 10))
}

// The coverage data of one figure file: shown rows in configuration
// order, omitted rows with their reason, and the totals.
#let coverage-data(path) = {
  let rows = csv(path, row-type: dictionary)
  let order = ()
  let by = (:)
  for r in rows {
    let key = r.section + "/" + r.row
    if key not in by {
      order.push(key)
      by.insert(key, (label: r.label, codes: 0, covered: 0, segments: (0,) * 6, display: none, reason: ""))
    }
    let e = by.at(key)
    if r.measure == "denominator" { e.codes = int(r.denominator) }
    if r.measure == "stage" and r.item == "target-standard" { e.covered = int(r.numerator) }
    if r.measure == "segment" and r.item in coverage-segments {
      e.segments.at(coverage-segments.position(s => s == r.item)) = int(r.numerator)
    }
    if r.measure == "display" {
      e.display = r.item
      e.reason = r.note
    }
    by.insert(key, e)
  }
  let all = order.map(k => by.at(k))
  let shown = all.filter(e => e.display == "shown")
  let omitted = all.filter(e => e.display == "omitted")
  let sum(list, f) = list.fold(0, (acc, e) => acc + f(e))
  let in-scope = all.filter(e => e.reason != "out-of-scope")
  (
    shown: shown,
    omitted: omitted,
    total: (
      label: "Total, " + str(shown.len()) + " systems",
      codes: sum(shown, e => e.codes),
      covered: sum(shown, e => e.covered),
      segments: range(6).map(i => sum(shown, e => e.segments.at(i))),
    ),
    in-scope: (codes: sum(in-scope, e => e.codes), covered: sum(in-scope, e => e.covered)),
  )
}

// One bar: the six tier-route parts and the uncovered rest, separated
// by the surface gap; a part under half a point draws nothing.
#let coverage-bar(e, width) = layout(size => {
  let w = size.width
  let parts = e.segments.enumerate().filter(((i, n)) => n > 0).map(((i, n)) => (i, n))
  let rest = e.codes - e.segments.sum()
  if rest > 0 { parts.push((6, rest)) }
  let parts = parts.filter(((i, n)) => w * n / e.codes >= 0.5pt)
  let usable = w - fig-gap * (parts.len() - 1)
  box(width: w, height: fig-bar-height, radius: 1.5pt, clip: true, stack(dir: ltr, spacing: fig-gap, ..parts.map(((i, n)) => {
    let pw = usable * n / e.codes
    let fill = if i < 6 { coverage-fills.at(i) } else { fig-track }
    box(width: pw, height: fig-bar-height, {
      place(rect(width: 100%, height: 100%, fill: fill))
      if i < 6 and calc.odd(i) { place(rect(width: 100%, height: 100%, fill: fig-hatch)) }
    })
  })))
})

#let fig-head(body) = text(font: (font-heading,), weight: weight-heading, tracking: display-tracking, size: fig-head-size, fill: ink-secondary, body)

#let legend-chip(fill, hatched: false) = box(width: 8pt, height: 8pt, radius: 1pt, clip: true, baseline: 0.5pt, {
  place(rect(width: 100%, height: 100%, fill: fill))
  if hatched { place(rect(width: 100%, height: 100%, fill: fig-hatch)) }
})

// The legend: the tier ladder, then the route encoding.
#let coverage-legend() = {
  set text(size: fig-size - 0.5pt, fill: ink-secondary)
  set par(justify: false)
  let item(chip, label) = box[#chip#h(4pt)#label]
  block(above: 0pt, below: fig-rule-pad + 2pt, {
    item(legend-chip(fig-high), [High tier])
    h(12pt)
    item(legend-chip(fig-medium), [Medium tier])
    h(12pt)
    item(legend-chip(fig-low), [Below medium or unassessed])
    h(12pt)
    item(legend-chip(fig-track), [No standard target])
    linebreak()
    item(legend-chip(fig-high, hatched: true), [Hatched: reaches the standard concept through one “Maps to” step; solid: direct standard target])
  })
}

// The full figure: legend over a table of the shown systems and their
// total, between recessive rules with room on both sides.
#let coverage-figure(data) = {
  let n = data.shown.len()
  let row(e, strong: false) = {
    let t(body) = if strong { text(weight: weight-medium, body) } else { body }
    (t(e.label), t(grouped(e.covered)), t(grouped(e.codes)), t(tenths(e.covered, e.codes)), coverage-bar(e, 100%))
  }
  // Header row is y = 0; systems 1..n; the total n + 1. Rows next to a
  // rule carry the extra pad, so no rule touches its rows.
  let pad-y = 2.6pt
  block(breakable: false, above: space.block-outer, below: 0pt, {
    coverage-legend()
    set text(size: fig-size, number-width: "tabular")
    set par(justify: false)
    table(
      columns: (1fr, auto, auto, auto, 46%),
      align: (left + horizon, right + horizon, right + horizon, right + horizon, left + horizon),
      stroke: none,
      inset: (x, y) => (
        left: if x == 0 { 0pt } else if x == 4 { 12pt } else { 10pt },
        right: 0pt,
        top: if y == 1 or y == n + 1 { fig-rule-pad } else if y == 0 { 0pt } else { pad-y },
        bottom: if y == 0 or y == n { fig-rule-pad } else { pad-y },
      ),
      table.header(fig-head[System], fig-head[Covered], fig-head[Codes], fig-head[%], fig-head[Coverage by tier]),
      table.hline(stroke: fig-rule),
      ..data.shown.map(e => row(e)).flatten(),
      table.hline(stroke: fig-rule),
      ..row(data.total, strong: true),
    )
  })
}

// Small counts spelled out, as prose sets them.
#let count-word(n) = {
  let words = ("no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten")
  if n < words.len() { words.at(n) } else { str(n) }
}

// The caption under the coverage figure, the figure's only caption:
// what the bar shows, and every omitted row with its coverage, all
// numbers read from the figure data. Set in the caption style one
// figure gap below the figure.
#let coverage-caption(data) = {
  let small = data.omitted.filter(e => e.reason == "small")
  let custom = data.omitted.filter(e => e.reason == "custom")
  let scope = data.omitted.filter(e => e.reason == "out-of-scope")
  let listed = small + custom
  let cov(e) = e.label + " (" + grouped(e.covered) + " of " + grouped(e.codes) + ")"
  let sum(list, f) = list.fold(0, (acc, e) => acc + f(e))
  block(breakable: false, above: space.figure-gap + 6pt, below: space.block-outer, width: 100%, {
    set par(justify: false, leading: 0.45em)
    text(size: caption-size + 0.5pt, fill: ink-secondary)[
      Coverage and confidence per Swedish system under the target-standard rule: bar length is the covered share, shade the confidence tier of the covering mappings (chapter 3), and the hatch marks coverage that reaches a standard concept only through one "Maps to" step.
      #if listed.len() > 0 [
        Not shown: #count-word(small.len()) systems below fifty codes#if custom.len() > 0 [ and the Swedish geography codes, custom concepts by nature]: #listed.map(cov).join("; "), together #grouped(sum(listed, e => e.covered)) of #grouped(sum(listed, e => e.codes)) codes covered.
      ]
      #for e in scope [
        #e.label (#grouped(e.codes) codes) are out of scope, reported as attributes rather than gaps.
      ]
    ]
  })
}
