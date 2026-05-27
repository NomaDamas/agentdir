pub mod backend;
pub mod catalog;
pub mod error;
pub mod manifest;
pub mod materializer;
pub mod reconciler;
pub mod reflink;
pub mod snapshot;
pub mod types;
pub mod watcher;
pub mod workspace;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_version_not_empty() {
        assert!(!version().is_empty());
    }
}
