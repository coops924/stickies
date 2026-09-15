//! API keys for the direct-API fallback, kept in the OS credential store
//! (macOS Keychain, Windows Credential Manager, Secret Service on Linux).

const SERVICE: &str = "io.github.coops924.stickies";
pub const ANTHROPIC: &str = "anthropic-api-key";
pub const OPENAI: &str = "openai-api-key";

pub fn get(name: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, name)
        .and_then(|e| e.get_password())
        .ok()
        .filter(|k| !k.is_empty())
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())?;
    if value.trim().is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(value.trim()).map_err(|e| format!("couldn't save to the system keychain: {e}"))
    }
}
