mod directories;

pub use crate::config::constants::DROPTUBE_DIRECTORY;
pub use directories::prepare_droptube_directory;

/// holds functions that print display to the user through the Command Line Interface
pub mod display;

/// module thats holds functionality related to scanning and parsing video files in a directory
pub mod scanner;

/// contains functions related to interacting with tailwind
pub mod tailwind;

pub mod thumbnails;

/// Builds the LAN URL displayed by both the command-line and terminal interfaces.
#[must_use]
pub fn local_url_of(local_ip_addr: &str, port: u16) -> String {
    format!("http://{}:{}", local_ip_addr, port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_url_contains_the_address_and_port() {
        assert_eq!(local_url_of("192.168.1.4", 8081), "http://192.168.1.4:8081");
    }
}
