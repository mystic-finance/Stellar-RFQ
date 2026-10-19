#![no_std]
//! Math and cross-contract interfaces shared by every Octarine contract. Type
//! library only — never deployed. Contracts call each other through the
//! clients generated here, never by linking each other's crates, which would
//! export one contract's entry points from another's wasm.

pub mod interfaces;
pub mod math;
pub mod schedule;
pub mod timelock;
pub mod types;
