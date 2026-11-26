//! Offline mode validation for SynEvid CLI
//!
//! This module enforces that --offline flag is only valid for commands
//! in the OFFLINE_COMMANDS allowlist.

use crate::OFFLINE_COMMANDS;

/// Validates that a command supports offline mode
/// Returns Ok(()) if valid, Err with message if not
pub fn validate_offline_mode(command_id: &str, offline: bool) -> Result<(), String> {
    if !offline {
        return Ok(());
    }
    
    if !supports_offline(command_id) {
        return Err(format!(
            "command '{}' does not support --offline mode. Offline commands: {}",
            command_id,
            OFFLINE_COMMANDS.join(", ")
        ));
    }
    
    Ok(())
}

/// Check if a command supports offline mode
pub fn supports_offline(command_id: &str) -> bool {
    OFFLINE_COMMANDS.contains(&command_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_offline_validation() {
        // These should pass
        assert!(validate_offline_mode("impact", true).is_ok());
        assert!(validate_offline_mode("verify", true).is_ok());
        assert!(validate_offline_mode("baseline", true).is_ok());
        assert!(validate_offline_mode("tui", true).is_ok());
        
        // These should fail
        assert!(validate_offline_mode("attach", true).is_err());
        assert!(validate_offline_mode("status", true).is_err());
        assert!(validate_offline_mode("events", true).is_err());
        assert!(validate_offline_mode("analytics", true).is_err());
    }
    
    #[test]
    fn test_offline_false_always_ok() {
        assert!(validate_offline_mode("impact", false).is_ok());
        assert!(validate_offline_mode("attach", false).is_ok());
        assert!(validate_offline_mode("analytics", false).is_ok());
    }
}