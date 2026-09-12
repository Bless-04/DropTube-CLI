mod directories;

pub use directories::{DROPTUBE_DIRECTORY, prepare_droptube_directory};

/// holds functions that print display to the user through the Command Line Interface
pub mod display;

/// module thats holds functionality related to scanning and parsing video files in a directory
pub mod scanner;

/// contains functions related to interacting with tailwind
pub mod tailwind;

pub mod thumbnails;
