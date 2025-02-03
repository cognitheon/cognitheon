#![warn(clippy::all, rust_2018_idioms)]

pub mod app;
pub mod canvas;
pub mod colors;
pub mod geometry;
pub mod globals;
pub mod graph;
pub mod particle;
pub mod ui;
pub use app::TemplateApp;
