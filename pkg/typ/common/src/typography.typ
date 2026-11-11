#import "tokens.typ": *

// Palette accessors for components called from content.
#let accent-fill(body) = context text(fill: palette.get().accent, body)

// Heading setting shared by every title role: the mono face at
// Regular, set tight. Heading lines never hyphenate: a broken word in a
// title is an awkward break, not a fit.
#let display(size, body, fill: auto) = text(
  font: (font-heading,),
  weight: weight-heading,
  tracking: display-tracking,
  size: size,
  hyphenate: false,
  ..if fill != auto { (fill: fill) },
  body,
)

// Keep-with-next guard: the heading block carries 3 B of reserved space
// below its text and cannot break, so a heading that would land within
// the last 3 B above the bottom margin moves to the next page; the
// negative space after it returns the reservation to the following
// text. A position-based decision is not used: the break would move
// the position it was decided on, and the layout would not converge.
#let guarded-heading(above, below, body) = {
  block(above: above, below: below, sticky: true, breakable: false, {
    body
    v(3 * B)
  })
  v(-3 * B)
}

// A level-one title at the wide tier. A title wider than the tier is
// set at the narrowest width that keeps its line count (a binary
// search between the even split w / n and the tier), so its lines come
// out even instead of a full line over an orphan; manual line breaks
// in the content are kept, and a title they keep within the tier is
// left as written.
#let balanced-title(body) = context {
  let natural = measure(body).width
  if natural <= title-width {
    block(width: title-width, body)
  } else {
    let height = measure(block(width: title-width, body)).height
    let lo = natural / calc.ceil(natural / title-width)
    let hi = title-width
    while hi - lo > title-balance-step {
      let mid = (lo + hi) / 2
      if measure(block(width: mid, body)).height > height { lo = mid } else { hi = mid }
    }
    block(width: calc.min(hi + title-balance-slack, title-width), body)
  }
}

// Level-one title text on the H1 line box: heading face, line box top
// lowered so a page-opening and an in-flow title both land on the grid.
#let title-text(size, body, fill: auto) = {
  set text(top-edge: heading-one-top)
  set par(leading: heading-one-flow-leading, justify: false)
  balanced-title(display(size, body, fill: fill))
}

// Level-1 heading in the flow. Exposed so page kinds that keep an H1
// in the flow (the summary page) can restore it under a template's
// opener rule.
#let heading-one-flow(it, new-page: false) = {
  if new-page { pagebreak(weak: true) }
  block(above: space.h1-above, below: space.h1-below, sticky: true, title-text(heading-one-size, it.body))
}

// Document-level typography: Source Sans 3 Light body on the half-line
// grid, Source Code Pro heads, justified on the wide measure, tabular
// numerals. Hyphenation stays on only as a costly last resort; the
// justification limits (tokens.typ) carry the slack instead. Runts
// (a last line of one short word) cost double. h1-new-page starts
// every level-1 heading on a new page (weak, so a heading that already
// opens a page adds none).
#let document-typography(doc, h1-new-page: true) = {
  set text(
    font: (font-body,),
    size: body-size,
    weight: weight-light,
    fill: ink,
    lang: "en",
    hyphenate: true,
    costs: (hyphenation: hyphenation-cost, runt: 200%),
    number-width: "tabular",
    top-edge: edge-top,
    bottom-edge: -descent,
  )
  set par(
    justify: true,
    leading: body-leading,
    spacing: space.par,
    justification-limits: justification-limits,
  )
  // Narrow table cells set ragged and unhyphenated: justification needs
  // the full measure, and a cell wraps at a word instead.
  show table: set par(justify: false)
  show table: set text(hyphenate: false)
  set list(indent: list-indent-step, spacing: space.list-item)
  set enum(indent: list-indent-step, spacing: space.list-item)
  show list: set block(above: space.list-above, below: space.list-below)
  show enum: set block(above: space.list-above, below: space.list-below)
  set figure(gap: space.figure-gap)
  // Figures and captions start on the content line.
  show figure: set align(left)
  show figure.caption: set align(left)
  // Typst's embedded default raw face is not vendored; every raw run
  // (backticks and raw()) uses the vendored mono face.
  show raw: set text(font: (font-mono,), weight: weight-mono, size: mono-size)
  // Absolute, not relative: strong on a Light body would otherwise land
  // at 600. The guards hold every heavier literal to Medium.
  show strong: it => text(weight: weight-medium, it.body)
  show text.where(weight: "semibold"): set text(weight: weight-medium)
  show text.where(weight: 600): set text(weight: weight-medium)
  show text.where(weight: "bold"): set text(weight: weight-medium)
  show text.where(weight: 700): set text(weight: weight-medium)
  show heading: it => {
    if it.level == 1 {
      heading-one-flow(it, new-page: h1-new-page)
    } else if it.level == 2 {
      guarded-heading(space.h2-above, space.h2-below, {
        set text(top-edge: heading-two-top)
        set par(leading: heading-two-leading, justify: false)
        display(heading-two-size, it.body)
      })
    } else {
      guarded-heading(space.h3-above, space.h3-below, text(weight: weight-medium, it.body))
    }
  }
  // Accent carries a link as its underline stroke; the glyphs stay ink.
  show link: it => context underline(stroke: 0.5pt + palette.get().accent, it)
  doc
}

// The document wrapper: per-document accent, paper, and tint, then the
// typography. none keeps the kit default for that role.
#let house-style(doc, accent: none, paper: none, tint: none, h1-new-page: true) = {
  palette.update(p => (
    accent: if accent != none { accent } else { p.accent },
    tint: if tint != none { tint } else { p.tint },
    paper: if paper != none { paper } else { p.paper },
  ))
  set page(
    paper: "a4",
    margin: flow-margins,
    fill: if paper != none { paper } else { paper-default },
  )
  show: document-typography.with(h1-new-page: h1-new-page)
  doc
}

// A document title on the H1 line box: the title size on the cover;
// chapter openers pass the level-one size.
// A document title on the H1 line box: the title size on the cover;
// chapter openers pass the level-one size.
#let doc-title(body, fill: auto, size: title-size) = block(above: seam-zero, below: seam-zero, title-text(size, body, fill: fill))
#let caption(body) = text(size: caption-size, fill: ink-secondary, body)

// Full six-column span from inside the narrowed flow: the block is
// wider than the flow and extends into columns 4-6. A widened block
// keeps its own outer spacing; spacing on the inner block would be
// swallowed at the wrapper's edge.
#let wide(body) = block(
  width: grid-width,
  above: if body.func() == block { body.at("above", default: auto) } else { auto },
  below: if body.func() == block { body.at("below", default: auto) } else { auto },
  body,
)
