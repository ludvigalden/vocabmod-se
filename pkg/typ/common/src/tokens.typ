// Design tokens for the vocabmod-se report templates (page grid, faces,
// type ladder, half-line grid).
// Palette roles, faces, the type ladder, the six-column page grid, and
// one vertical spacing table in half-line units. The body line B is
// the single rhythm unit: every baseline-to-baseline step between
// stacked blocks is B + k x B/2.

// Palette roles. Accent, paper, and tint are per-document parameters
// (house-style); the values here are only the kit defaults.
#let ink = rgb("#1a1a1a")
#let ink-secondary = rgb("#5b5b5b") // 6.35:1 on the default paper
#let rule = rgb("#7b7979") // non-text rules only
#let paper-default = rgb("#faf7f0")
#let tint-default = rgb("#e9efe7") // the summary page's ground
#let tint-neutral = rgb("#f4f3f1") // technical-information panels
#let accent-default = rgb("#24533f")

// Faces: Source Sans 3 is the body, Source Code Pro the mono face for
// code and for every heading role (cover title, chapter titles, H2).
#let font-body = "Source Sans 3"
#let font-mono = "Source Code Pro"
#let font-heading = font-mono

// Weights: nothing heavier than Medium.
#let weight-light = 300
#let weight-heading = 600 // mono headings at SemiBold: Regular read too thin
// Negative tracking pulls the wide monospace advance in on display sizes.
#let display-tracking = -0.06em
#let weight-mono = 400 // a monospace face reads lighter than its weight
#let weight-medium = 500

// Line breaking. Justified body on the wide measure. A tracking
// tolerance of 0.012 em (0.13 pt per glyph at body size, invisible)
// lets the paragraph builder spread a line's slack over its letters,
// so word spaces stay within 75-130% of natural and no rivers form.
// With that slack few lines need a hyphen; the raised cost keeps a
// break inside a word the last resort. A wider tracking band removes
// the remaining breaks but begins to show as letterspacing.
#let hyphenation-cost = 120%
#let justification-limits = (spacing: (min: 75%, max: 130%), tracking: (min: -0.012em, max: 0.012em))

// Type ladder and the half-line unit.
#let body-size = 11pt
#let B = 15pt // body line pitch
#let half-line = B / 2
// The cover keeps the display sizes (title 30, subtitle 18); in the
// document, chapter titles and H2 are set smaller so a page's heads do
// not outweigh its text. Heading baselines come from the line-box
// tokens below, not from these sizes, so every size keeps the grid.
#let title-size = 30pt
#let subtitle-size = 18pt
#let heading-one-size = 22pt
#let heading-two-size = 14pt
#let heading-three-size = body-size
#let quote-size = 14pt
#let label-size = 7.5pt
#let label-tracking = 0.04em
#let table-size = body-size // one table regime at body size
#let caption-size = 8pt
#let running-size = caption-size
// Mono identifiers sit inline in body lines, so the size only scales
// the glyphs; the line box keeps the body edges. Both faces share an
// x-height of 0.478 em, so 9.5 keeps mono at 0.86 of the body x-height.
#let mono-size = 9.5pt

// Line boxes run from 0.8 em above the baseline to a fixed depth below
// it at every size, so a block's bottom edge never depends on its size
// and adjacent-block spacing (which merges by max) stays on the grid.
#let edge-top = 0.8em
#let descent = 0.2 * body-size
#let leading-for(size, pitch) = pitch - 0.8 * size - descent
#let gap-before(step, size) = step - descent - 0.8 * size

#let body-leading = leading-for(body-size, B)
// An in-flow H1 steps 3 B from a paragraph and from an inset block
// alike, so its above must equal the block class (the larger value
// wins the merge); its line box top is lowered from 0.8 em to fit.
#let heading-one-top = 3 * B - descent - gap-before(2 * B, body-size)
#let heading-one-flow-leading = 2 * B - heading-one-top - descent
// A page-opening H2 must put its baseline on the grid: half a line
// below the first body baseline, so its line box top is that distance
// from the content top (16.3 pt, not 0.8 em = 14.4); the in-flow above
// is reduced by the same amount.
#let heading-two-top = 0.8 * body-size + half-line
#let heading-two-leading = 1.5 * B - heading-two-top - descent

// The spacing table: block above/below values, each derived from its
// baseline step. Heading-to-heading pairs resolve by the max merge of
// the following heading's above and the preceding one's below.
#let space = (
  par: gap-before(1.5 * B, body-size), // p -> p, 22.5
  list-item: gap-before(1.25 * B, body-size), // item -> item, 18.75
  list-above: gap-before(1.25 * B, body-size), // lead-in -> list, 18.75
  list-below: gap-before(1.5 * B, body-size), // list -> p, 22.5
  h1-above: 3 * B - descent - heading-one-top, // p -> H1 in flow, 45
  h1-below: gap-before(2 * B, body-size), // H1 -> p, 30
  h2-above: 3 * B - descent - heading-two-top, // p -> H2, 45
  h2-below: gap-before(1.5 * B, body-size), // H2 -> p, 22.5
  h3-above: gap-before(2 * B, body-size), // p -> H3, 30
  h3-below: gap-before(B, body-size), // H3 -> its text, 15
  block-outer: gap-before(2 * B, body-size), // p <-> table, figure, note
  figure-gap: B / 4, // figure -> caption
)

// Insets: one table regime (row pitch 1.5 B) and the note panel. An
// inset block's edges are moved onto its text (a negative wrapper
// inset for tables, an outset fill for notes), so its outer spacing is
// the plain block class and still merges correctly with a heading's.
#let table-inset-x = half-line
#let table-inset-y = (1.5 * B - 0.8 * body-size - descent) / 2
#let note-inset-x = B
#let note-inset-y = half-line
#let kv-inset-y = (B - 0.8 * body-size - descent) / 2 // rows on the 1 B pitch
#let list-indent-step = body-size
#let micro-seam = B / 4 // label-to-rule seam
#let seam-zero = 0pt

// A4 single-sided page on a six-column grid. The flow is narrowed to
// the body measure, ending inside column 4.
#let page-width = 595.28pt
#let page-margins = (left: 36pt, right: 36pt, top: 42pt, bottom: 48pt)
#let column-count = 6
#let grid-gutter = 12pt
#let grid-width = page-width - page-margins.left - page-margins.right
#let column-width = (grid-width - (column-count - 1) * grid-gutter) / column-count
#let span-width(n) = n * column-width + (n - 1) * grid-gutter
// The measure holds about 84 characters per line: wide enough that
// justified lines set with few hyphenation breaks.
#let measure-factor = 1.4 * 11 / 10.5
#let body-width = measure-factor * span-width(3)
#let flow-margins = (
  left: page-margins.left,
  right: page-width - page-margins.left - body-width,
  top: page-margins.top,
  bottom: page-margins.bottom,
)
// Level-one titles (H1, opener and cover titles) are set at the wide
// tier, so they wrap less than prose. A title that still wraps is
// narrowed to its balanced width: the search steps by title-balance-step
// and the found width keeps title-balance-slack against rounding.
#let title-width = span-width(5)
#let title-balance-step = 1pt
#let title-balance-slack = 0.5pt

// Strokes.
#let rule-hairline-stroke = 0.5pt + rule
#let accent-rule-thickness = 1pt

// Page kinds. The opener band spans the page width from the top edge.
// In the flow it is a block whose bottom edge sits on the text-edge
// grid, so the title's last baseline (4 half-lines above that edge) and
// the body resuming 4 B below the title both land on the grid. The band
// ends band-padding above the first line box after it. The panel cover
// drops its title by whole lines from the content top.
#let band-flow-height = 22.5 * half-line
#let band-title-inset = 4 * half-line
#let band-padding = 3 * half-line
#let band-height = page-margins.top + band-flow-height + space.h1-below - band-padding
#let cover-title-drop = 12 * B // leaves room for the cover contents
// Subtitle rhythm: d is the colour-band padding above the following
// text (3 half-lines, 22.5 pt). The subtitle sits 2 B + d below the
// title's baseline, the rule d below the subtitle's edge, and the next
// edge d + d/3 below the rule. The line edges around the rule add
// body-size, which puts the next baseline off the grid; the declared
// snap rounds that edge sum up to whole half-lines.
#let subtitle-rule-width = 24mm
#let subtitle-d = band-padding
#let subtitle-d-third = subtitle-d / 3
#let subtitle-grid-snap = calc.ceil(body-size / half-line) * half-line - body-size
#let subtitle-above = gap-before(2 * B + subtitle-d, subtitle-size)
#let subtitle-rule-gap = subtitle-d
#let subtitle-content-gap = subtitle-d + subtitle-d-third + subtitle-grid-snap
#let logo-plate-inset = half-line
// The foot sits one line below the bottom margin, outside the flow.
#let foot-gap = B
#let wordmark = "vocabmod-se"

// The per-document palette; house-style updates it, components read it
// under context.
#let palette = state("common-kit-palette", (accent: accent-default, tint: tint-default, paper: paper-default))
