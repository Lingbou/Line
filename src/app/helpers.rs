use std::path::Path;

use crate::config::{Profile, profile_names_equal};

use super::DEFAULT_PORT;

pub(super) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(super) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(super) fn prev_word_boundary(text: &str, char_cursor: usize) -> usize {
    if char_cursor == 0 {
        return 0;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut i = char_cursor.min(chars.len());
    while i > 0 && !is_word_char(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1]) {
        i -= 1;
    }
    i
}

pub(super) fn next_word_boundary(text: &str, char_cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if char_cursor >= len {
        return len;
    }
    let mut i = char_cursor;
    while i < len && is_word_char(chars[i]) {
        i += 1;
    }
    while i < len && !is_word_char(chars[i]) {
        i += 1;
    }
    i
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

pub(super) fn remove_char_range(text: &mut String, start_char: usize, end_char: usize) {
    if start_char >= end_char {
        return;
    }
    let start_byte = text
        .char_indices()
        .nth(start_char)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let end_byte = text
        .char_indices()
        .nth(end_char)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.replace_range(start_byte..end_byte, "");
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
    let mut text = host.trim();
    if let Some(without_ssh) = text.strip_prefix("ssh ") {
        text = without_ssh.trim();
    }
    let parts: Vec<&str> = text.split_whitespace().collect();
    let mut remaining_parts = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i] == "-p" && i + 1 < parts.len() && parts[i + 1].parse::<u16>().is_ok() {
            *port = parts[i + 1].to_owned();
            i += 2;
        } else if let Some(suffix) = parts[i].strip_prefix("-p")
            && suffix.parse::<u16>().is_ok()
        {
            *port = suffix.to_owned();
            i += 1;
        } else {
            remaining_parts.push(parts[i]);
            i += 1;
        }
    }
    let combined = remaining_parts.join(" ");
    let mut host_candidate = combined.as_str();

    if let Some(at) = host_candidate.rfind('@') {
        if username.is_empty() || username == "root" {
            *username = host_candidate[..at].to_owned();
        }
        host_candidate = &host_candidate[at + 1..];
    }

    if host_candidate.starts_with('[') {
        if let Some(close) = host_candidate.find(']') {
            let address = host_candidate[1..close].to_owned();
            if host_candidate.as_bytes().get(close + 1) == Some(&b':')
                && *port == DEFAULT_PORT.to_string()
            {
                let suffix = &host_candidate[close + 2..];
                if !suffix.is_empty() {
                    *port = suffix.to_owned();
                }
            }
            *host = address;
            return;
        }
    } else if *port == DEFAULT_PORT.to_string()
        && host_candidate.matches(':').count() == 1
        && let Some(colon) = host_candidate.rfind(':')
    {
        let suffix = &host_candidate[colon + 1..];
        if suffix.parse::<u16>().is_ok() {
            *port = suffix.to_owned();
            *host = host_candidate[..colon].to_owned();
            return;
        }
    }
    *host = host_candidate.to_owned();
}
