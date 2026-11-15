//! pulley-core: everything Pulley computes.
//!
//! Parsing (SVG path data, Illustrator captures, Figma node data), geometry and color
//! conversion, validation of untrusted input against hard limits, the canonical
//! `pulley-scene` format, the bridge protocol and the After Effects apply plan all live
//! here. The crate does no I/O and contains no unsafe code, so the same logic runs
//! natively inside the bridge and as WebAssembly inside every client.
#![forbid(unsafe_code)]

pub mod aeplan;
pub mod error;
pub mod ffx;
pub mod figma;
pub mod geom;
pub mod illustrator;
pub mod limits;
pub mod protocol;
pub mod scene;
pub mod shapes;
pub mod svg;
pub mod svgpath;
pub mod validate;

pub use error::{Error, ErrorCode};

/// Version of this crate, embedded in scenes and protocol messages.
pub const PULLEY_VERSION: &str = env!("CARGO_PKG_VERSION");
