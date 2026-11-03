//! trammel 4.0. Standard library only.

pub mod benchstat;
pub mod canon;
pub mod cli;
pub mod config;
pub mod contract;
pub mod doctor;
pub mod export;
pub mod goal;
pub mod harness;
pub mod hash;
pub mod implicit;
pub mod mcp;
pub mod migrate;
pub mod planner;
pub mod json;
pub mod lang;
pub mod root;
pub mod score;
pub mod similarity;
pub mod strategies;
pub mod store;
pub mod syntax;
pub mod tools;
pub mod topo;
pub mod walk;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_VERSION: &str = "4.0.0";

