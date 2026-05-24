pub mod error;
pub mod backend;
pub mod types;

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
