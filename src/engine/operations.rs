//! Live engine operations: what a supervised game session establishes.
//!
//! The debugger worker writes what it sees to an event stream. The supervisor reads that stream
//! once, when the game is paused, and reduces it to answers with the rules in these modules. The
//! caller receives the reduced answers; it never reads the stream.
pub(crate) mod event_stream;
pub(crate) mod fixture;
pub(crate) mod loaded_modifiers;
pub(crate) mod registry_items;
