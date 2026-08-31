use std::path::Path;

use crate::config::{Profile, profile_names_equal};

use super::DEFAULT_PORT;

pub(super) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(super) fn insert_char(text: &mut String, char_index: usize, character: char) {
    let byte_index = text
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.insert(byte_index, character);
}

pub(super) fn insert_str(text: &mut String, char_index: usize, value: &str) {
    let byte_index = text
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.insert_str(byte_index, value);
}

pub(super) fn remove_char(text: &mut String, char_index: usize) {
    let Some(start) = text.char_indices().nth(char_index).map(|(index, _)| index) else {
        return;
    };
    let end = text
        .char_indices()
        .nth(char_index + 1)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.replace_range(start..end, "");
}

pub(super) fn next_default_name(profiles: &[Profile]) -> String {
    (1..)
        .map(|number| format!("Connection {number}"))
        .find(|candidate| {
            profiles
                .iter()
                .all(|profile| !profile_names_equal(&profile.name, candidate))
        })
        .expect("an unused numeric default connection name always exists")
}

pub(super) fn parse_endpoint_shorthand(
    host: &mut String,
    username: &mut String,
    port: &mut String,
) {
    if let Some(at) = host.rfind('@') {
        if username.is_empty() {
            *username = host[..at].to_owned();
        }
        *host = host[at + 1..].to_owned();
    }
    if host.starts_with('[') {
        if let Some(close) = host.find(']') {
            let address = host[1..close].to_owned();
            if host.as_bytes().get(close + 1) == Some(&b':')
                && port == DEFAULT_PORT.to_string().as_str()
            {
                let suffix = &host[close + 2..];
                if !suffix.is_empty() {
                    *port = suffix.to_owned();
                }
            }
            *host = address;
        }
    } else if port == DEFAULT_PORT.to_string().as_str() {
        // Only split a single-colon host:port; an unbracketed IPv6 address is
        // left untouched and can still be entered in the Host field.
        if host.matches(':').count() == 1
            && let Some(colon) = host.rfind(':')
        {
            let suffix = &host[colon + 1..];
            if suffix.parse::<u16>().is_ok() {
                *port = suffix.to_owned();
                host.truncate(colon);
            }
        }
    }
}
