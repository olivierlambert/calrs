// Temporary library shim so examples (e.g. `examples/ews_smoke.rs`) can
// exercise internal modules. Not exported in published crates.
#![allow(dead_code)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod caldav;
pub mod ews;
pub mod providers;
pub mod utils;
