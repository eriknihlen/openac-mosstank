//! What MossTank keeps of a session beside its rules (converted from the
//! C# plugin in `../src/AcDream.Plugins.MossTank`):
//!
//! - [`action_history`]: the bounded history of what the macro was doing
//!   (`ActionHistory`);
//! - [`kill_statistics`]: the optional per-session kill counters
//!   (`KillStatistics`);
//! - [`game_info`]: the check that keeps the player's game database current
//!   from openac-gamedata (`VtankGameInfoUpdater`), and the HTTP request it
//!   sends and how it reads the answer (`HttpVtankGameInfoTransport`).
//!
//! Nothing here calls the client: the shell hands each part the client's
//! clock, storage and events.

pub mod action_history;
pub mod clock;
pub mod game_info;
pub mod kill_statistics;
pub mod testing;

pub use clock::SessionClock;
