//! OS credential-store access for API keys.
//!
//! Keys live in the platform credential vault (Windows Credential Manager,
//! macOS Keychain, Linux Secret Service) under service name `meetily` with
//! accounts like `api-key/openai`. The SQLite `settings` columns are kept
//! only as a legacy fallback: `SettingsRepository` migrates any plaintext
//! key it finds into the vault and clears the column afterwards.
//!
//! Failure policy: if the vault is unavailable (headless Linux, locked
//! keychain, ...), callers fall back to the legacy database storage and log
//! a warning — losing the encryption benefit is preferable to losing the
//! user's keys.

use log::{info, warn};

const SERVICE: &str = "meetily";

/// Account namespaced by key kind and provider, e.g. `api-key/openai`,
/// `transcript-api-key/groq`.
pub fn account(kind: &str, provider: &str) -> String {
    format!("{}/{}", kind, provider)
}

fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| format!("keyring entry: {}", e))
}

/// Stores `secret` in the OS credential store.
pub fn set_secret(account: &str, secret: &str) -> Result<(), String> {
    entry(account)?.set_password(secret).map_err(|e| format!("keyring set: {}", e))
}

/// Reads `secret` from the OS credential store. `Ok(None)` means the entry
/// does not exist; `Err` means the store itself is unavailable.
pub fn get_secret(account: &str) -> Result<Option<String>, String> {
    match entry(account)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("keyring get: {}", e)),
    }
}

/// Deletes `secret` from the OS credential store. Deleting a missing entry
/// is success.
pub fn delete_secret(account: &str) -> Result<(), String> {
    match entry(account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keyring delete: {}", e)),
    }
}

/// Best-effort migration of a legacy plaintext key into the vault.
/// Returns true when the key now lives in the vault (and the caller should
/// clear its plaintext copy).
pub fn migrate_secret(account: &str, plaintext: &str, provider: &str) -> bool {
    match set_secret(account, plaintext) {
        Ok(()) => {
            info!("migrated {} API key into OS credential store", provider);
            true
        }
        Err(e) => {
            warn!(
                "could not store {} API key in OS credential store ({}); keeping plaintext fallback",
                provider, e
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_namespacing() {
        assert_eq!(account("api-key", "openai"), "api-key/openai");
        assert_eq!(
            account("transcript-api-key", "groq"),
            "transcript-api-key/groq"
        );
    }
}
