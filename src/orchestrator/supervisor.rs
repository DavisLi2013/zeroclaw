use anyhow::ensure;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ButlerDirectory {
    active: HashMap<String, String>,
}

impl ButlerDirectory {
    pub fn register(&mut self, user_id: &str, butler_id: &str) -> anyhow::Result<()> {
        ensure!(
            !self.active.contains_key(user_id),
            "user already has an active butler"
        );
        self.active.insert(user_id.to_string(), butler_id.to_string());
        Ok(())
    }

    pub fn lookup(&self, user_id: &str) -> Option<&str> {
        self.active.get(user_id).map(String::as_str)
    }

    pub fn release(&mut self, user_id: &str) -> Option<String> {
        self.active.remove(user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn butler_directory_enforces_one_active_butler_per_user() {
        let mut directory = ButlerDirectory::default();
        assert!(directory.register("user-a", "butler-1").is_ok());
        assert!(directory.register("user-a", "butler-2").is_err());
    }

    #[test]
    fn butler_directory_can_release_and_reassign_user() {
        let mut directory = ButlerDirectory::default();
        directory.register("user-a", "butler-1").unwrap();
        assert_eq!(directory.lookup("user-a"), Some("butler-1"));

        let released = directory.release("user-a");
        assert_eq!(released.as_deref(), Some("butler-1"));

        assert!(directory.register("user-a", "butler-2").is_ok());
        assert_eq!(directory.lookup("user-a"), Some("butler-2"));
    }
}
