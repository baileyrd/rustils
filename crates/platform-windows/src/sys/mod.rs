//! Safe wrappers over the ffi layer. All `unsafe` in this crate lives in
//! this module tree, one documented invariant per block.

pub mod errmap;
pub mod fileio;
pub mod handle;
pub mod nt;
