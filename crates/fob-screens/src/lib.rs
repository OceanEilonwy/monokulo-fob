//! What each screen shows: the wallet's records and the clock in, finished
//! rows and words out. Pure functions: nothing here touches the node, a file
//! or a background task, so both apps draw the same thing.

pub mod activity;
pub mod backup;
pub mod catch_up;
pub mod clock;
pub mod contacts;
pub mod manual;
pub mod error;
pub mod not_ready;
pub mod profile;
pub mod requests;
pub mod split;
