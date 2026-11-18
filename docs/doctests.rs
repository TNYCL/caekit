//! Compiles the Rust examples of the repository documentation.
//!
//! Every Markdown page that holds a `rust` code block is listed below, which makes rustdoc
//! build its examples as doctests of this unpublished package:
//!
//! ```sh
//! cargo test --locked --doc -p caekit-docs
//! ```
//!
//! An example therefore cannot drift from the public API. Examples that read a file relative
//! to the repository root are fenced as `rust,no_run`: they are compiled, not run.
//! `scripts/check-docs.mjs` fails when a page with Rust examples is missing here. The README
//! of the `caekit` crate is that crate's own front page and is tested there.

#[cfg(doctest)]
mod pages {
    #[doc = include_str!("../README.md")]
    pub struct Readme;

    #[doc = include_str!("physics/multimaterial-heat-conduction.md")]
    pub struct MultimaterialHeatConduction;

    #[doc = include_str!("physics/thermoelastic-solid.md")]
    pub struct ThermoelasticSolid;
}
