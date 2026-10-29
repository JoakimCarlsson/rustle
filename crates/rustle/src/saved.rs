//! Keeps values saved with `remember_saveable` in a small file per application.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use rustle_runtime::Saver;

/// A key-value file, one entry per line, kept in the user's state directory.
pub(crate) struct FileSaver {
    path: Option<PathBuf>,
    entries: RefCell<HashMap<String, String>>,
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn unescape(text: &str) -> String {
    let mut result = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some(other) => result.push(other),
            None => {}
        }
    }
    result
}

impl FileSaver {
    /// Opens the file for the running program, or works in memory if it cannot be placed.
    pub(crate) fn open() -> Self {
        let path = Self::locate();
        let mut entries = HashMap::new();
        if let Some(path) = &path
            && let Ok(text) = fs::read_to_string(path)
        {
            for line in text.lines() {
                if let Some((key, value)) = line.split_once('\t') {
                    entries.insert(unescape(key), unescape(value));
                }
            }
        }
        Self {
            path,
            entries: RefCell::new(entries),
        }
    }

    fn locate() -> Option<PathBuf> {
        let name = std::env::current_exe()
            .ok()?
            .file_stem()?
            .to_str()?
            .to_owned();
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })?;
        Some(state.join("rustle").join(format!("{name}.state")))
    }

    fn write(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let mut lines: Vec<String> = self
            .entries
            .borrow()
            .iter()
            .map(|(key, value)| format!("{}\t{}", escape(key), escape(value)))
            .collect();
        lines.sort();
        if let Some(directory) = path.parent() {
            let _ = fs::create_dir_all(directory);
        }
        let _ = fs::write(path, lines.join("\n"));
    }
}

impl Saver for FileSaver {
    fn load(&self, key: &str) -> Option<String> {
        self.entries.borrow().get(key).cloned()
    }

    fn store(&self, key: &str, value: &str) {
        let changed = self.entries.borrow().get(key).map(String::as_str) != Some(value);
        if changed {
            self.entries
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            self.write();
        }
    }
}
