//! API Key 安全存储：macOS Keychain / Windows Credential Manager（keyring crate，FR-6.3）。
//! 应用内任何界面、日志、错误上报均不回显完整 Key（仅显示后 4 位）。

const SERVICE: &str = "com.wordpick.app";

pub fn store_key(provider: &str, key: &str) -> Result<(), keyring::Error> {
    let entry = keyring::Entry::new(SERVICE, provider)?;
    entry.set_password(key)
}

pub fn read_key(provider: &str) -> Result<String, keyring::Error> {
    let entry = keyring::Entry::new(SERVICE, provider)?;
    entry.get_password()
}

/// Key 掩码：仅显示后 4 位（FR-6.3）
pub fn mask_key(key: &str) -> String {
    if key.len() <= 4 {
        return "****".to_string();
    }
    format!("****{}", &key[key.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_key() {
        assert_eq!(mask_key("sk-abcdef123456"), "****3456");
        assert_eq!(mask_key("abcd"), "****");
        assert_eq!(mask_key("ab"), "****");
    }
}
