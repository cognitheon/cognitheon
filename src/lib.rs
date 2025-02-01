#![warn(clippy::all, rust_2018_idioms)]

pub mod app;
pub mod global;
pub mod canvas;
pub mod colors;
pub mod graph;
pub mod ui;
pub mod geometry;
pub use app::TemplateApp;
