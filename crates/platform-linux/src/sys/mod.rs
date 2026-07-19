//! Safe wrappers over the ffi layer. All `unsafe` in this crate lives in
//! this module tree; every `unsafe` block carries a `// SAFETY:` invariant
//! comment (enforced by `clippy::undocumented_unsafe_blocks` at the
//! workspace level).

pub mod fdio;
