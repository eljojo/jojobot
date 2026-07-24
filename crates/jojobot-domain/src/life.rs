//! Life layers — the ports for the services jojobot fronts (task/kanban, notes,
//! calendar, library). Each owner is fronted, never replaced. The concrete
//! anti-corruption adapters live in `jojobot-adapters`; the *ports* (traits)
//! will live here so the domain depends on abstractions, not clients.
//!
//! TODO: skeleton only — define the service ports (traits) here.
