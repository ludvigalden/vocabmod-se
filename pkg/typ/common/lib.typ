// Shared report frame for vocabmod-se documents: design tokens,
// typography, and components. Import
// root-relatively from documents compiled at the repository root:
//
//   #import "/pkg/typ/common/lib.typ": descriptor-doc, ...
//
// Compilation runs with `--root <repo> --font-path <repo>/pkg/typ/assets/fonts`.
#import "src/tokens.typ": *
#import "src/typography.typ": *
#import "src/components.typ": *
#import "src/figures.typ": coverage-data, coverage-figure, coverage-caption

// One descriptor document: cover panel, band-opened chapters, and
// free-flowing body content in the kit house style. config: title,
// subtitle?, author?, logo? (pre-sized image content), logo-plate?
// (default true), meta ((label, value) pairs), contents? (default true:
// a linked table of contents on the cover), and the optional document
// palette accent/paper/tint.
#let descriptor-doc(config, body) = {
  set document(title: config.title, author: config.at("author", default: ()))
  show: house-style.with(
    accent: config.at("accent", default: none),
    paper: config.at("paper", default: none),
    tint: config.at("tint", default: none),
    h1-new-page: true,
  )
  show: foot-furniture
  cover(
    config.title,
    subtitle: config.at("subtitle", default: none),
    mode: "panel",
    logo: config.at("logo", default: none),
    logo-plate: config.at("logo-plate", default: true),
    details: if config.at("meta", default: ()) != () { kv-list(config.meta) },
    contents: config.at("contents", default: true),
  )
  show heading.where(level: 1): band-opener
  body
}
