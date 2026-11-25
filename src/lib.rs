//! Code shared by PC Tube (the YouTube window) and PC Tube Descargas (the download manager).
#![recursion_limit = "256"] // config::defaults() is one big json! literal

pub mod config;
pub mod dmclient;
pub mod dmserver;
pub mod engine;
pub mod http;
