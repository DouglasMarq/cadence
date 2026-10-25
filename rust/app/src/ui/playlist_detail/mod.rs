mod actions;
mod binding;
mod deps;
mod detail;
mod editing;
mod loading;
mod model;
mod pages;
mod playing;
mod rows;
mod selecting;
mod targets;
#[cfg(test)]
mod tests;
mod view;
#[cfg(test)]
mod view_tests;

pub use loading::refresh_if_showing;
pub use view::install;
