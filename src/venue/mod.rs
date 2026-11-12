//! Venues execute orders. The engine only talks to the [`adapter::Venue`]
//! trait; which chain or market sits behind it is configuration.

pub mod adapter;
pub mod external;
pub mod order;
pub mod simulated;
pub mod venue_factory;
