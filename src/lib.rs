//! `tracing-subscriber` layer for logging to the systemd journal
//!
//! Provides a `Layer` implementation for use with `tracing-subscriber` that can be configured.
//! Shows all spans and arguments.

//
mod formatting;

///The main module for the SystemdLayer and its implementations
mod systemd_layer;
pub use systemd_layer::SystemdLayer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
