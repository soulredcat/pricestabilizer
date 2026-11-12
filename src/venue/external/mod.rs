//! External venues: a connector process, written in any language for any
//! chain, speaks JSON lines on its stdin and stdout. No chain code lives here.

pub mod paper_server;
pub mod process_venue;
pub mod protocol;
