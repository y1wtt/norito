//! Ports: contracts only. No GUI types cross this boundary; OS paths enter
//! only through `Paths` and attachment import, and leave only as display text.
//!
//! - `types`: opaque handles and the values exchanged with adapters.
//! - `error`: the typed failure every port returns.
//! - `ports`: one trait per OS service.
//! - `names`: validation of user-entered entry names.
mod error;
mod names;
mod ports;
mod types;

pub use error::*;
pub use names::*;
pub use ports::*;
pub use types::*;
