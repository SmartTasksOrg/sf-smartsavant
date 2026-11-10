//! SmartSavant for Rust: the design-council engine of spec/SEMANTICS.md, with the gates, the schema
//! checks, the stub / scripted / replay adapters, the exporters, the SmartSignal importer and the audit.
//! The Python reference (src/sf_smartsavant) is the detailed reference; the shared vectors in
//! spec/conformance test this crate (`cargo run --release --bin conformance DIR`).
//!
//! ```no_run
//! use smartsavant::{Council, Options, StubAdapter};
//! let mut council = Council::new("Learn reusable automations", StubAdapter, Options::default()).unwrap();
//! let res = council.run();
//! println!("{}", smartsavant::exporters::to_spec_md(&res));
//! ```
pub mod adapters;
pub mod audit;
pub mod conformance;
pub mod engine;
pub mod exporters;
pub mod gates;
pub mod importer;
pub mod json;
pub mod model;
pub mod py;
pub mod schema;
pub mod sha256;

pub use adapters::{
    Adapter, AdapterError, Context, MergeRequest, Prior, ReplayAdapter, ScriptedAdapter, StubAdapter, ViewRequest,
};
pub use engine::{health, Council, Health, Options, Phase};
pub use json::{Object, Value};
pub use model::{CouncilResult, CouncilRound, DecisionNode, JurorMerge, StakeholderView, W5H1};
