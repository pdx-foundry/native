//! Bounded registry discovery from recorded executable inputs and historical startup traces.
//!
//! The method needs no config seeds, name list, content files, debugger, or game launch. It
//! enumerates exact template loader symbols, then reconstructs a bounded startup scheduling table
//! from raw instructions, literal strings and Mach-O chained fixups (offset-format 64-bit chained
//! pointers and addend64 imports). Same-image weak bindings stay static candidates: they cannot
//! establish current runtime interposition.
//!
//! Unknown calls invalidate volatile register values. Unsupported instructions, missing pointers,
//! and clobbered or missing table owners become explicit gaps. Candidates, scheduling witnesses
//! and relationships are separate, and each states its basis. Counts from this method never
//! establish that all registries were found.
mod ownership;
mod records;
mod replay;
mod scheduler;

pub use records::*;
pub use replay::{derive, replay};
pub use scheduler::{candidates, scheduler};

/// Discovery derivation revision, independent of live registry enumeration.
pub const METHOD: &str = "registry-discovery/v1";
/// Retained input contract.
pub const FORMAT: &str = "pdx-native-registry-discovery/v1";
