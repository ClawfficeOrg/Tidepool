//! One module per tab. Each tab owns its state, its keys and its client calls;
//! the shell in [`crate::app`] only pumps them.
//!
//! Hosts and presence lives in [`hosts`]; triggers and runs in [`runs`].

pub mod hosts;
pub mod runs;
