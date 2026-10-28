//! Network adapter inventory, selection and subnet models.
mod adapter;
mod cache;
mod id;
mod lease;
mod windows;

pub use adapter::*;
pub use cache::*;
pub use id::*;
pub use lease::*;
pub use opclook_contract::net::{NetError, SubnetHosts, SubnetSpec};
pub use windows::inventory_adapters;

#[cfg(test)]
mod tests;
