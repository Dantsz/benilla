//! The 1.12.1 (build 5875) world-protocol vocabulary, free of any transport: opcode numbers and
//! names, little-endian wire helpers, packed GUIDs and GUID high parts. `benilla-protocol` builds
//! the client on it and re-exports it at its old paths; it builds for `wasm32-unknown-unknown`.

pub mod guid;
pub mod opcode;
mod opcode_names;
pub mod wire;

pub use opcode_names::opcode_name;
