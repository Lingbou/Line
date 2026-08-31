use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn expand_tilde(value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_owned());
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(rest))
            .ok_or_else(|| "HOME is not set".to_owned());
    }
    Ok(PathBuf::from(value))
}

pub(super) fn complete_path(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Type or paste a private-key path first".to_owned());
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let current = std::env::current_dir().map_err(|error| error.to_string())?;
    let expanded = expand_tilde(value)?;
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        current.join(expanded)
    };

    if absolute.is_file() {
        return Ok(value.to_owned());
    }
    if absolute.is_dir() && !value.ends_with('/') {
        return Ok(format!("{value}/"));
    }

    let (directory, fragment) = if value.ends_with('/') {
        (absolute.as_path(), "")
    } else {
        (
            absolute.parent().unwrap_or(&current),
            absolute
                .file_name()
                .and_then(|part| part.to_str())
                .unwrap_or_default(),
        )
    };
    let mut matches = fs::read_dir(directory)
        .map_err(|error| format!("Cannot read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.starts_with(fragment).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.0.cmp(&right.0));
    if matches.is_empty() {
        return Err("No matching path".to_owned());
    }

    let completed_name = common_prefix(matches.iter().map(|(name, _)| name.as_str()));
    let completed = directory.join(&completed_name);
    let mut display = display_completed_path(&completed, value, home.as_deref(), &current);
    if matches.len() == 1 && matches[0].1.is_dir() {
        display.push('/');
    }
    if completed_name == fragment && matches.len() > 1 {
        let preview = matches
            .iter()
            .take(4)
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("{} matches: {preview}", matches.len()));
    }
    Ok(display)
}

pub(super) fn common_prefix<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let mut values = values;
    let Some(first) = values.next() else {
        return String::new();
    };
    let mut prefix = first.to_owned();
    for value in values {
        let bytes = prefix
            .char_indices()
            .zip(value.chars())
            .take_while(|((_, left), right)| *left == *right)
            .map(|((index, character), _)| index + character.len_utf8())
            .last()
            .unwrap_or(0);
        prefix.truncate(bytes);
    }
    prefix
}

fn display_completed_path(
    completed: &Path,
    original: &str,
    home: Option<&Path>,
    current: &Path,
) -> String {
    if original.starts_with('~')
        && let Some(home) = home
        && let Ok(relative) = completed.strip_prefix(home)
    {
        return if relative.as_os_str().is_empty() {
            "~".to_owned()
        } else {
            format!("~/{}", relative.display())
        };
    }
    if !Path::new(original).is_absolute()
        && let Ok(relative) = completed.strip_prefix(current)
    {
        return relative.display().to_string();
    }
    completed.display().to_string()
}
