#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::option_if_let_else)]

pub mod cli;
pub mod context;
pub mod db;
pub mod integrity;
pub mod models;
pub mod pipeline;
pub mod reference;
pub mod rules;
pub mod tools;
pub mod util;
