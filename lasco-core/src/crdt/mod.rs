//! CRDT protocol, canonical state, and durable local replica storage.

mod persistence;
mod state;
#[cfg(test)]
mod tests;

pub use persistence::*;
pub use state::*;
