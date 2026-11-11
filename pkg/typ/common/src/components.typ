#import "tokens.typ": *
#import "typography.typ": wide, doc-title, title-text, heading-one-flow, display

// An inset block measured from its text: the negative wrapper inset
// pulls the block's edges onto the first and last text lines, so the
// outer spacing is the plain block class and merges by max with a
// neighbouring heading's spacing without adding the inset on top.
#let text-edged(outer, inset-y, body) = block(
  width: 100%,
  above: outer,
  below: outer,
  inset: (y: -inset-y),
  body,
)

// Pale callout with a run-in label: the document tint for summaries and
// notes, neutral for technical information. The label sits one plain
// line above the body; outer spacing is the shared block class,
// measured from the text (the inset is not counted). A note never
// splits across pages.
#let callout(label, body, neutral: false) = text-edged(space.block-outer, note-inset-y, context block(
  width: 100%,
  breakable: false,
  fill: if neutral { tint-neutral } else { palette.get().tint },
  inset: (x: note-inset-x, y: note-inset-y),
  {
    block(above: seam-zero, below: seam-zero, text(weight: weight-medium, label))
    block(above: space.h3-below, below: seam-zero, body)
  },
))

// Definition-style rows for short metadata: label in secondary ink,
// value in main ink. entries is an array of (label, value) pairs.
#let kv-list(entries) = text-edged(space.block-outer, kv-inset-y, table(
  columns: (auto, 1fr),
  align: left,
  stroke: none,
  column-gutter: B,
  row-gutter: seam-zero,
  inset: (x: seam-zero, y: kv-inset-y),
  ..entries.map(((label, value)) => (
    text(size: body-size, fill: ink-secondary, label),
    value,
  )).flatten(),
))

// Data table in the one table regime: body size, rows on a 1.5 B
// pitch, Medium header labels without fill over one hairline, no
// vertical rules. columns is the column spec, header an array of
// strings, rows an array of row arrays; aligns is a per-column
// alignment array. Fractional columns give the table its measure.
#let data-table(columns: none, header: none, rows: none, aligns: auto) = {
  if columns == none or header == none or rows == none {
    panic("data-table requires columns, header, and rows")
  }
  text-edged(space.block-outer, table-inset-y, text(size: table-size, table(
    columns: columns,
    align: if aligns == auto { left } else { aligns },
    stroke: none,
    inset: (x: table-inset-x, y: table-inset-y),
    table.header(..header.map(h => text(weight: weight-medium, h))),
    table.hline(stroke: rule-hairline-stroke),
    ..rows.flatten(),
  )))
}

// A corpus record shown verbatim: raw text on the neutral technical
// panel, mono through the raw show rule, breakable so a longer record
// can cross a page.
#let record-panel(body) = block(
  width: 100%,
  above: space.block-outer,
  below: space.block-outer,
  breakable: true,
  fill: tint-neutral,
  inset: (x: note-inset-x, y: note-inset-y),
  if type(body) == str { raw(body, block: true) } else { body },
)

// ---- Page kinds and furniture ----

// Marks the page it lands on as a page kind; readable with typst query
// after render, so page-map gates know the deliberate kinds without
// hard-coded page numbers.
#let page-kind(name) = context [#metadata((
  kind: name,
  page: here().page(),
  y: here().position().y.pt(),
)) <page-kind>]

// Foot line: the wordmark and the folio one line below the bottom
// margin, outside the flow; no running head. A cover sets the same
// wordmark without the folio.
#let foot-line(fill: ink-secondary, folio: true) = {
  v(foot-gap - 0.8 * caption-size)
  wide(grid(
    columns: (1fr, auto),
    text(size: caption-size, weight: weight-medium, fill: fill, wordmark),
    if folio { text(size: caption-size, fill: fill, context counter(page).display()) },
  ))
}
#let foot-furniture(doc) = {
  set page(footer: foot-line(), footer-descent: seam-zero)
  doc
}

// Cover subtitle, rule and details below the title.
#let subtitle-rule(subtitle, fill: ink-secondary, rule-paint: auto) = context {
  let paint = if rule-paint == auto { palette.get().accent } else { rule-paint }
  if subtitle != none {
    block(above: subtitle-above, below: seam-zero, text(size: subtitle-size, fill: fill, hyphenate: false, par(justify: false, subtitle)))
  }
  block(above: subtitle-rule-gap, below: subtitle-content-gap, line(length: subtitle-rule-width, stroke: accent-rule-thickness + paint))
}

#let cover-tail(subtitle, details, ink-sub, rule-paint) = {
  subtitle-rule(subtitle, fill: ink-sub, rule-paint: rule-paint)
  if details != none { block(above: subtitle-content-gap, below: seam-zero, details) }
}

// Contents on the cover: one linked entry per level-1 heading with its
// page number, under a small mono label. Entries are PDF links.
#let cover-contents(fill) = block(above: 2 * B, below: seam-zero, {
  block(above: seam-zero, below: half-line, text(font: (font-heading,), size: caption-size, tracking: label-tracking, fill: fill, upper[Contents]))
  set par(justify: false)
  show outline.entry: it => link(it.element.location(), block(above: seam-zero, below: seam-zero, inset: (y: kv-inset-y), grid(
    columns: (1fr, auto),
    column-gutter: B,
    it.body(),
    it.page(),
  )))
  outline(title: none, depth: 1)
})

// Cover. compact: a paper page with the title, subtitle, a short
// accent rule and the details block; a logo sits top-right on the grid
// edge. panel: a full-bleed accent page with the title in paper
// colour, the details, and the foot-line wordmark in paper colour. The
// logo sits on a paper plate; logo-plate: false places the caller's
// logo content bare. Both end the page.
#let cover(title, subtitle: none, mode: "compact", logo: none, logo-plate: true, details: none, contents: false) = {
  if mode == "panel" {
    context {
      let p = palette.get()
      page(fill: p.accent, footer: foot-line(fill: p.paper, folio: false), footer-descent: seam-zero, {
        page-kind("cover")
        set text(fill: p.paper)
        show text.where(fill: ink-secondary): set text(fill: p.paper)
        if logo != none {
          place(top + left, if logo-plate { block(fill: p.paper, inset: logo-plate-inset, logo) } else { logo })
        }
        v(cover-title-drop, weak: false)
        doc-title(title, fill: p.paper)
        cover-tail(subtitle, details, p.paper, p.paper)
        if contents { cover-contents(p.paper) }
      })
    }
  } else {
    page-kind("cover")
    if logo != none {
      place(top + left, block(width: grid-width, align(right, logo)))
    }
    doc-title(title)
    cover-tail(subtitle, details, ink-secondary, auto)
    pagebreak(weak: true)
  }
}

// The opener heading block shared by the level-1 opener: its bottom
// edge sits on the text-edge grid, so the title's last baseline (4
// half-lines above it) and the body resuming 4 B below the title land
// on the grid. With band, the accent band is drawn from the page's top
// edge behind it and the title is set in paper colour; without, the
// title is ink. title maps the title colour to the title content.
#let opener-block(title, band) = context {
  let p = palette.get()
  block(width: 100%, height: band-flow-height, above: seam-zero, below: space.h1-below, breakable: false, {
    if band {
      place(top + left, dx: -page-margins.left, dy: -page-margins.top, rect(width: page-width, height: band-height, fill: p.accent))
    }
    align(bottom, block(above: seam-zero, below: seam-zero, inset: (bottom: band-title-inset), title(if band { p.paper } else { ink })))
  })
}

// Chapter opener on the band: a new page with the level-1 heading on
// the accent band. Apply as a show rule on level-1 headings.
#let band-opener(it) = {
  pagebreak(weak: true)
  page-kind("chapter")
  opener-block(fill => doc-title(it.body, fill: fill, size: heading-one-size), true)
}

// Summary page: the full-bleed summary kind in the document tint with
// dark ink. Its level-1 heading stays in the flow on the tint page (it
// is the document's own opener).
#let summary-page(body) = context page(fill: palette.get().tint, {
  page-kind("summary")
  show heading.where(level: 1): it => heading-one-flow(it)
  body
})
