//! MossTank's UB tab, converted from the C# plugin: the features brought
//! over from UtilityBelt (MIT; see `rust/NOTICE.md`).
//!
//! - [`chat`]: the shapes of the lines the UtilityBelt side prints
//!   (`UbChat`).
//! - [`help`]: every `/ub` command's usage line (`UbCommandHelp`).
//! - [`search`]: finding the object a typed name stands for
//!   (`UbObjectSearch`).
//! - [`catalog`]: the UB settings catalogue as the tab reaches it.
//! - [`alias`]: the alias list and its chat interceptor (`AliasTable`,
//!   `AliasController`).
//!
//! The client comes in through [`host::UbHost`] (the meta engine's
//! [`mosstank_meta::host::MetaHost`] and the calls only the UB commands
//! make); the expression runtime through [`expressions::UbExpressions`].

pub mod alias;
pub mod catalog;
pub mod chat;
pub mod expressions;
pub mod help;
pub mod host;
pub mod search;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
