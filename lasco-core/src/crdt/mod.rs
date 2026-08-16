//! CRDT protocol, materialized state, and durable local storage.

mod persistence;
mod state;
#[cfg(test)]
mod tests;

pub use persistence::*;
pub use state::*;
