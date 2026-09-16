//! Notes live as plain Markdown files with a small frontmatter header, one file
//! per note, in a shared data directory. The desktop app, the MCP server and the
//! CLI all read and write the same files, so Claude Code / Codex can work with
//! notes whether or not the app is running.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

pub const COLORS: &[&str] = &["yellow", "pink", "green", "blue", "purple", "orange", "gray"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub color: String,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct NotePatch {
    pub body: Option<String>,
    pub color: Option<String>,
    pub pinned: Option<bool>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    Ambiguous(String),
    Invalid(String),
    Io(io::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound(id) => write!(f, "no note matches '{id}'"),
            StoreError::Ambiguous(id) => write!(f, "'{id}' matches more than one note; use more of the id"),
            StoreError::Invalid(msg) => write!(f, "{msg}"),
            StoreError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

/// Data directory shared by every entry point. `STICKIES_HOME` overrides it,
/// which is also how tests and portable installs point at another location.
pub fn default_root() -> PathBuf {
    if let Some(home) = std::env::var_os("STICKIES_HOME") {
        return PathBuf::from(home);
    }
    dirs::data_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("stickies")
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Store { root: root.into() };
        fs::create_dir_all(store.notes_dir())?;
        fs::create_dir_all(store.trash_dir())?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self> {
        Self::open(default_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn notes_dir(&self) -> PathBuf {
        self.root.join("notes")
    }

    fn trash_dir(&self) -> PathBuf {
        self.root.join("trash")
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.notes_dir().join(format!("{id}.md"))
    }

    /// All notes, most recently updated first.
    pub fn list(&self) -> Result<Vec<Note>> {
        read_notes(&self.notes_dir())
    }

    /// Deleted notes, most recently updated first.
    pub fn list_trash(&self) -> Result<Vec<Note>> {
        read_notes(&self.trash_dir())
    }

    /// Moves a note back out of the trash.
    pub fn restore(&self, id: &str) -> Result<Note> {
        let id = id.trim();
        if !is_safe_id(id) {
            return Err(StoreError::Invalid(format!("invalid note id '{id}'")));
        }
        let from = self.trash_dir().join(format!("{id}.md"));
        if !from.exists() {
            return Err(StoreError::NotFound(id.into()));
        }
        fs::rename(from, self.path_for(id))?;
        self.resolve(id)
    }

    /// Look a note up by full id, unique id prefix, or exact (case-insensitive) title.
    pub fn resolve(&self, key: &str) -> Result<Note> {
        let key = key.trim();
        if key.is_empty() {
            return Err(StoreError::Invalid("note id is required".into()));
        }
        let path = self.path_for(key);
        if is_safe_id(key) && path.exists() {
            let text = fs::read_to_string(&path)?;
            return Ok(parse(key, &text));
        }
        let notes = self.list()?;
        let lower = key.to_lowercase();
        let by_prefix: Vec<&Note> = notes.iter().filter(|n| n.id.to_lowercase().starts_with(&lower)).collect();
        match by_prefix.len() {
            1 => return Ok(by_prefix[0].clone()),
            n if n > 1 => return Err(StoreError::Ambiguous(key.into())),
            _ => {}
        }
        // Exact title first, then a unique title prefix ("groc" -> "Groceries").
        for prefix_ok in [false, true] {
            let found: Vec<&Note> = notes
                .iter()
                .filter(|n| {
                    let title = n.title.to_lowercase();
                    title == lower || (prefix_ok && title.starts_with(&lower))
                })
                .collect();
            match found.len() {
                0 => continue,
                1 => return Ok(found[0].clone()),
                _ => return Err(StoreError::Ambiguous(key.into())),
            }
        }
        Err(StoreError::NotFound(key.into()))
    }

    pub fn create(&self, body: &str, color: Option<&str>, tags: Vec<String>) -> Result<Note> {
        let now = now();
        let color = match color {
            Some(c) => validate_color(c)?,
            None => "yellow".to_string(),
        };
        let note = Note {
            id: ulid::Ulid::new().to_string().to_lowercase(),
            title: title_of(body),
            body: body.to_string(),
            color,
            pinned: false,
            tags,
            created: now.clone(),
            updated: now,
        };
        self.write(&note)?;
        Ok(note)
    }

    pub fn update(&self, key: &str, patch: NotePatch) -> Result<Note> {
        let mut note = self.resolve(key)?;
        if let Some(body) = patch.body {
            note.title = title_of(&body);
            note.body = body;
        }
        if let Some(color) = patch.color {
            note.color = validate_color(&color)?;
        }
        if let Some(pinned) = patch.pinned {
            note.pinned = pinned;
        }
        if let Some(tags) = patch.tags {
            note.tags = tags;
        }
        note.updated = now();
        self.write(&note)?;
        Ok(note)
    }

    pub fn append(&self, key: &str, text: &str) -> Result<Note> {
        let note = self.resolve(key)?;
        let mut body = note.body.clone();
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str(text);
        self.update(&note.id, NotePatch { body: Some(body), ..Default::default() })
    }

    /// Moves the note to the trash folder rather than deleting it outright.
    pub fn delete(&self, key: &str) -> Result<Note> {
        let note = self.resolve(key)?;
        fs::rename(self.path_for(&note.id), self.trash_dir().join(format!("{}.md", note.id)))?;
        Ok(note)
    }

    pub fn search(&self, query: &str) -> Result<Vec<Note>> {
        let q = query.to_lowercase();
        Ok(self
            .list()?
            .into_iter()
            .filter(|n| n.body.to_lowercase().contains(&q) || n.tags.iter().any(|t| t.to_lowercase() == q))
            .collect())
    }

    fn write(&self, note: &Note) -> Result<()> {
        if !is_safe_id(&note.id) {
            return Err(StoreError::Invalid(format!("invalid note id '{}'", note.id)));
        }
        let path = self.path_for(&note.id);
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, serialize(note))?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

/// Every `.md` file in a folder, newest first. A half-written or hand-edited
/// file is skipped rather than hiding every other note.
fn read_notes(dir: &Path) -> Result<Vec<Note>> {
    let mut notes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            notes.push(parse(stem, &text));
        }
    }
    notes.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(notes)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn validate_color(color: &str) -> Result<String> {
    let c = color.trim().to_lowercase();
    if COLORS.contains(&c.as_str()) {
        Ok(c)
    } else {
        Err(StoreError::Invalid(format!("unknown color '{color}' (use one of: {})", COLORS.join(", "))))
    }
}

/// First non-empty line, without Markdown heading/list/checkbox markers.
pub fn title_of(body: &str) -> String {
    let line = body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line = line.trim_start_matches('#').trim_start();
    let line = line
        .strip_prefix("- [ ] ")
        .or_else(|| line.strip_prefix("- [x] "))
        .or_else(|| line.strip_prefix("- "))
        .unwrap_or(line);
    let title: String = line.chars().take(60).collect();
    if title.is_empty() { "Untitled".into() } else { title }
}

fn serialize(note: &Note) -> String {
    format!(
        "---\nid: {}\ncolor: {}\npinned: {}\ntags: {}\ncreated: {}\nupdated: {}\n---\n{}",
        note.id,
        note.color,
        note.pinned,
        note.tags.join(", "),
        note.created,
        note.updated,
        note.body
    )
}

fn parse(file_stem: &str, text: &str) -> Note {
    let mut meta = BTreeMap::new();
    let mut body = text;
    if let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    meta.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            body = rest[end + 4..].strip_prefix("\r\n").or_else(|| rest[end + 4..].strip_prefix('\n')).unwrap_or(&rest[end + 4..]);
        }
    }
    let get = |k: &str| meta.get(k).cloned().unwrap_or_default();
    let updated = get("updated");
    Note {
        id: file_stem.to_string(),
        title: title_of(body),
        body: body.to_string(),
        color: validate_color(&get("color")).unwrap_or_else(|_| "yellow".into()),
        pinned: get("pinned") == "true",
        tags: get("tags").split(',').map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect(),
        created: meta.get("created").cloned().unwrap_or_else(|| updated.clone()),
        updated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> Store {
        let dir = std::env::temp_dir().join(format!("stickies-test-{}", ulid::Ulid::new()));
        Store::open(dir).unwrap()
    }

    #[test]
    fn create_read_update_roundtrip() {
        let s = temp_store();
        let n = s.create("# Groceries\n- [ ] milk", Some("pink"), vec!["home".into()]).unwrap();
        assert_eq!(n.title, "Groceries");
        let got = s.resolve(&n.id).unwrap();
        assert_eq!(got, n);

        let updated = s
            .update(&n.id[..8], NotePatch { body: Some("Errands\n---\nnot frontmatter".into()), pinned: Some(true), ..Default::default() })
            .unwrap();
        assert_eq!(updated.title, "Errands");
        assert!(updated.pinned);
        assert_eq!(s.resolve(&n.id).unwrap().body, "Errands\n---\nnot frontmatter");
    }

    #[test]
    fn append_search_delete() {
        let s = temp_store();
        let n = s.create("Ideas", None, vec![]).unwrap();
        s.append(&n.id, "- ship the MCP server").unwrap();
        assert_eq!(s.resolve("ideas").unwrap().body, "Ideas\n- ship the MCP server");
        assert_eq!(s.resolve("ide").unwrap().id, n.id);
        s.create("Ideation", None, vec![]).unwrap();
        assert!(matches!(s.resolve("ide"), Err(StoreError::Ambiguous(_))));
        assert_eq!(s.resolve("ideas").unwrap().id, n.id);
        assert_eq!(s.search("mcp").unwrap().len(), 1);
        s.delete(&n.id).unwrap();
        assert!(matches!(s.resolve(&n.id), Err(StoreError::NotFound(_))));
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn deleted_notes_can_be_listed_and_restored() {
        let s = temp_store();
        let n = s.create("Draft", None, vec![]).unwrap();
        s.delete(&n.id).unwrap();
        assert_eq!(s.list_trash().unwrap().len(), 1);
        assert_eq!(s.restore(&n.id).unwrap().title, "Draft");
        assert!(s.list_trash().unwrap().is_empty());
        assert_eq!(s.list().unwrap().len(), 1);
        assert!(s.restore("../../etc/passwd").is_err());
        assert!(matches!(s.restore(&n.id), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn rejects_bad_color_and_unsafe_ids() {
        let s = temp_store();
        assert!(matches!(s.create("x", Some("chartreuse"), vec![]), Err(StoreError::Invalid(_))));
        assert!(s.resolve("../../etc/passwd").is_err());
    }
}
