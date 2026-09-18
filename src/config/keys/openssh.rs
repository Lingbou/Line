use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use base64::Engine;
use sha2::{Digest, Sha256};

use super::storage::write_private_file;
use super::{KeyError, KeyResult};

pub(super) fn canonical_pair_matches(
    root: &Path,
    private_path: &Path,
    public_path: &Path,
    expected_identity: &str,
) -> bool {
    let Ok(private) = fs::read(private_path) else {
        return false;
    };
    let Ok(derived) = derive_public_key(root, &private) else {
        return false;
    };
    if canonical_public_identity(&derived).ok().as_deref() != Some(expected_identity) {
        return false;
    }
    let Ok(public) = fs::read_to_string(public_path) else {
        return false;
    };
    canonical_public_identity(&public).ok().as_deref() == Some(expected_identity)
}

pub(super) fn validate_private_bytes(bytes: &[u8]) -> KeyResult<()> {
    if bytes.is_empty() || bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(KeyError::EmptyPrivateKey);
    }
    let text = String::from_utf8_lossy(bytes);
    if !text.contains("PRIVATE KEY") {
        return Err(KeyError::InvalidPrivateKey);
    }
    if text.contains("ENCRYPTED") || text.contains("Proc-Type: 4,ENCRYPTED") {
        return Err(KeyError::EncryptedPrivateKey);
    }
    if let Some(cipher) = openssh_cipher_name(&text)
        && cipher != "none"
    {
        return Err(KeyError::EncryptedPrivateKey);
    }
    Ok(())
}

fn openssh_cipher_name(text: &str) -> Option<String> {
    if !text.contains("BEGIN OPENSSH PRIVATE KEY") {
        return None;
    }
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(body)
        .ok()?;
    let magic = b"openssh-key-v1\0";
    if !decoded.starts_with(magic) {
        return None;
    }
    let mut cursor = magic.len();
    read_ssh_string(&decoded, &mut cursor).map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn read_ssh_string<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    if bytes.len().saturating_sub(*cursor) < 4 {
        return None;
    }
    let length = u32::from_be_bytes(bytes[*cursor..*cursor + 4].try_into().ok()?) as usize;
    *cursor += 4;
    let end = (*cursor).checked_add(length)?;
    if end > bytes.len() {
        return None;
    }
    let value = &bytes[*cursor..end];
    *cursor = end;
    Some(value)
}

pub(super) fn derive_public_key(root: &Path, private: &[u8]) -> KeyResult<String> {
    let temporary = root.join(format!(".keygen-{}", super::unique_suffix()));
    write_private_file(&temporary, private)?;
    let result = (|| {
        let output = Command::new("ssh-keygen")
            .args(["-y", "-P", "", "-f"])
            .arg(&temporary)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| KeyError::Io {
                path: std::path::PathBuf::from("ssh-keygen"),
                source,
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr)
                .trim()
                .to_lowercase();
            if detail.contains("passphrase") || detail.contains("encrypted") {
                return Err(KeyError::EncryptedPrivateKey);
            }
            return Err(KeyError::InvalidPrivateKey);
        }
        let public = String::from_utf8(output.stdout).map_err(|_| KeyError::InvalidPublicKey)?;
        canonical_public_identity(&public)?;
        Ok(public.trim().to_owned())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

pub(super) fn canonical_public_identity(public: &str) -> KeyResult<String> {
    let known_types = [
        "ssh-rsa",
        "ssh-dss",
        "ssh-ed25519",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "sk-ssh-ed25519@openssh.com",
        "sk-ecdsa-sha2-nistp256@openssh.com",
        "rsa-sha2-256",
        "rsa-sha2-512",
    ];
    for line in public.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        for index in 0..fields.len().saturating_sub(1) {
            if known_types.contains(&fields[index])
                && base64::engine::general_purpose::STANDARD
                    .decode(fields[index + 1])
                    .is_ok()
            {
                return Ok(format!("{} {}", fields[index], fields[index + 1]));
            }
        }
    }
    Err(KeyError::InvalidPublicKey)
}

pub(super) fn fingerprint_for_identity(identity: &str) -> String {
    Sha256::digest(identity.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn fingerprint_for_public(public: &str) -> KeyResult<String> {
    Ok(fingerprint_for_identity(&canonical_public_identity(
        public,
    )?))
}
