use anyhow::Context;
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: PathBuf) -> anyhow::Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("Could not open workspace {}", root.display()))?;
        if !root.is_dir() {
            anyhow::bail!("Workspace is not a directory: {}", root.display());
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = self.root.file_name().and_then(|name| name.to_str()) {
            name.to_string()
        } else {
            self.root.to_string_lossy().to_string()
        }
    }

    pub fn resolve_existing(&self, rel: &str) -> anyhow::Result<PathBuf> {
        let rel = clean_relative_path(rel)?;
        let path = self.root.join(rel).canonicalize()?;
        ensure_inside(&self.root, &path)?;
        Ok(path)
    }

    pub fn resolve_for_write(&self, rel: &str) -> anyhow::Result<PathBuf> {
        let rel = clean_relative_path(rel)?;
        let path = self.root.join(rel);
        let mut existing_ancestor = path.parent().unwrap_or(&self.root);
        while !existing_ancestor.exists() {
            existing_ancestor = existing_ancestor
                .parent()
                .with_context(|| format!("No existing ancestor for {}", path.display()))?;
        }
        let existing_ancestor = existing_ancestor.canonicalize()?;
        ensure_inside(&self.root, &existing_ancestor)?;
        Ok(path)
    }

    pub fn ui_file_rows(&self, limit: usize) -> Vec<String> {
        let mut rows = Vec::new();
        for entry in WalkDir::new(&self.root)
            .max_depth(5)
            .into_iter()
            .filter_entry(|entry| !is_ignored_entry(entry))
            .filter_map(Result::ok)
        {
            if entry.path() == self.root {
                continue;
            }
            if rows.len() >= limit {
                rows.push("...".to_string());
                break;
            }
            let Ok(rel) = entry.path().strip_prefix(&self.root) else {
                continue;
            };
            let suffix = if entry.file_type().is_dir() { "/" } else { "" };
            rows.push(format!(
                "{}{}",
                rel.to_string_lossy().replace('\\', "/"),
                suffix
            ));
        }
        rows
    }

    pub fn read_text(&self, rel: &str, max_bytes: usize) -> anyhow::Result<String> {
        let path = self.resolve_existing(rel)?;
        if path.is_dir() {
            anyhow::bail!("Directory selected");
        }

        let bytes = fs::read(&path)?;
        if bytes.len() > max_bytes {
            anyhow::bail!(
                "File is too large to edit safely: {} bytes, limit is {} bytes",
                bytes.len(),
                max_bytes
            );
        }

        String::from_utf8(bytes)
            .with_context(|| format!("File is not valid UTF-8: {}", path.display()))
    }

    pub fn write_text(&self, rel: &str, content: &str) -> anyhow::Result<()> {
        let path = self.resolve_for_write(rel)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    pub fn rename_entry(&self, rel: &str, new_name: &str) -> anyhow::Result<String> {
        let new_name = clean_file_name(new_name)?;
        let source = self.resolve_existing(rel)?;
        if source == self.root {
            anyhow::bail!("Workspace root cannot be renamed from here");
        }
        let was_dir = source.is_dir();
        let parent = source
            .parent()
            .with_context(|| format!("No parent for {}", source.display()))?;
        ensure_inside(&self.root, parent)?;
        let target = parent.join(new_name);
        ensure_new_path_inside(&self.root, &target)?;
        if target.exists() {
            anyhow::bail!("Target already exists: {}", target.display());
        }

        fs::rename(&source, &target)?;
        relative_ui_path(&self.root, &target, was_dir)
    }

    pub fn duplicate_entry(&self, rel: &str) -> anyhow::Result<String> {
        let source = self.resolve_existing(rel)?;
        if source == self.root {
            anyhow::bail!("Workspace root cannot be duplicated from here");
        }
        let is_dir = source.is_dir();
        let parent = source
            .parent()
            .with_context(|| format!("No parent for {}", source.display()))?;
        ensure_inside(&self.root, parent)?;
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("Invalid file name: {}", source.display()))?;
        let target = unique_copy_path(parent, file_name)?;
        ensure_new_path_inside(&self.root, &target)?;

        if is_dir {
            copy_dir_all(&source, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }

        relative_ui_path(&self.root, &target, is_dir)
    }

    pub fn delete_entry(&self, rel: &str) -> anyhow::Result<()> {
        let target = self.resolve_existing(rel)?;
        if target == self.root {
            anyhow::bail!("Workspace root cannot be deleted from here");
        }

        if target.is_dir() {
            fs::remove_dir_all(target)?;
        } else {
            fs::remove_file(target)?;
        }
        Ok(())
    }

    pub fn move_entry_to_dir(&self, rel: &str, target_dir_rel: &str) -> anyhow::Result<String> {
        let source = self.resolve_existing(rel)?;
        if source == self.root {
            anyhow::bail!("Workspace root cannot be moved from here");
        }
        let is_dir = source.is_dir();
        let target_dir = self.resolve_existing(target_dir_rel)?;
        if !target_dir.is_dir() {
            anyhow::bail!("Drop target is not a directory: {}", target_dir.display());
        }
        if is_dir && target_dir.starts_with(&source) {
            anyhow::bail!("Cannot move a directory into itself");
        }

        let file_name = source
            .file_name()
            .with_context(|| format!("Invalid file name: {}", source.display()))?;
        let target = target_dir.join(file_name);
        ensure_new_path_inside(&self.root, &target)?;
        if target == source {
            return relative_ui_path(&self.root, &target, is_dir);
        }
        if target.exists() {
            anyhow::bail!("Target already exists: {}", target.display());
        }

        fs::rename(&source, &target)?;
        relative_ui_path(&self.root, &target, is_dir)
    }
}

fn clean_relative_path(rel: &str) -> anyhow::Result<PathBuf> {
    let rel = if rel.trim().is_empty() {
        "."
    } else {
        rel.trim()
    };
    let path = Path::new(rel);
    if path.is_absolute() {
        anyhow::bail!("Absolute paths are not allowed");
    }

    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => cleaned.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("Path traversal is not allowed");
            }
        }
    }

    if cleaned.as_os_str().is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(cleaned)
    }
}

fn ensure_inside(root: &Path, path: &Path) -> anyhow::Result<()> {
    if !path.starts_with(root) {
        anyhow::bail!("Path is outside workspace: {}", path.display());
    }
    Ok(())
}

fn ensure_new_path_inside(root: &Path, path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("No parent for {}", path.display()))?;
    let parent = parent.canonicalize()?;
    ensure_inside(root, &parent)
}

fn clean_file_name(name: &str) -> anyhow::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("Name cannot be empty");
    }
    if name == "." || name == ".." {
        anyhow::bail!("Invalid file name");
    }
    if name.contains('/') || name.contains('\\') {
        anyhow::bail!("Name must not contain path separators");
    }
    if name
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        anyhow::bail!("Name contains characters that are not valid on Windows");
    }
    Ok(name)
}

fn relative_ui_path(root: &Path, path: &Path, is_dir: bool) -> anyhow::Result<String> {
    let rel = path
        .strip_prefix(root)
        .with_context(|| format!("Path is outside workspace: {}", path.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(if is_dir { format!("{rel}/") } else { rel })
}

fn unique_copy_path(parent: &Path, file_name: &str) -> anyhow::Result<PathBuf> {
    for index in 1..=10_000 {
        let candidate_name = copy_candidate_name(file_name, index);
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("Could not create a unique copy name for {file_name}")
}

fn copy_candidate_name(file_name: &str, index: usize) -> String {
    let suffix = if index == 1 {
        " copy".to_string()
    } else {
        format!(" copy {index}")
    };
    let path = Path::new(file_name);
    match (
        path.file_stem().and_then(|stem| stem.to_str()),
        path.extension().and_then(|extension| extension.to_str()),
    ) {
        (Some(stem), Some(extension)) if !stem.is_empty() => {
            format!("{stem}{suffix}.{extension}")
        }
        _ => format!("{file_name}{suffix}"),
    }
}

fn copy_dir_all(source: &Path, target: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_source = entry.path();
        let entry_target = target.join(entry.file_name());
        if entry_source.is_dir() {
            copy_dir_all(&entry_source, &entry_target)?;
        } else {
            fs::copy(&entry_source, &entry_target)?;
        }
    }
    Ok(())
}

pub fn is_ignored_entry(entry: &DirEntry) -> bool {
    let Some(name) = entry.file_name().to_str() else {
        return false;
    };
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".next"
            | "dist"
            | "build"
            | ".venv"
            | "__pycache__"
            | ".cargo"
            | ".rustup"
            | "rustup-init.exe"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_absolute_paths() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let err = workspace
            .resolve_existing("C:/Windows/System32")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Absolute paths"));
    }

    #[test]
    fn rejects_parent_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        let err = workspace
            .resolve_for_write("../x.txt")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Path traversal"));
    }

    #[test]
    fn reads_and_writes_text_inside_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        workspace
            .write_text("src/main.rs", "fn main() {}\n")
            .unwrap();
        assert_eq!(
            workspace.read_text("src/main.rs", 1024).unwrap(),
            "fn main() {}\n"
        );
        assert!(fs::metadata(temp.path().join("src/main.rs")).is_ok());
    }

    #[test]
    fn renames_duplicates_moves_and_deletes_entries_inside_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();

        workspace
            .write_text("src/main.rs", "fn main() {}\n")
            .unwrap();
        fs::create_dir_all(temp.path().join("other")).unwrap();

        let renamed = workspace.rename_entry("src/main.rs", "lib.rs").unwrap();
        assert_eq!(renamed, "src/lib.rs");
        assert!(temp.path().join("src/lib.rs").is_file());

        let copied = workspace.duplicate_entry("src/lib.rs").unwrap();
        assert_eq!(copied, "src/lib copy.rs");
        assert!(temp.path().join("src/lib copy.rs").is_file());

        let moved = workspace
            .move_entry_to_dir("src/lib copy.rs", "other")
            .unwrap();
        assert_eq!(moved, "other/lib copy.rs");
        assert!(temp.path().join("other/lib copy.rs").is_file());

        workspace.delete_entry("other/lib copy.rs").unwrap();
        assert!(!temp.path().join("other/lib copy.rs").exists());
    }

    #[test]
    fn rejects_unsafe_file_manager_names() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::new(temp.path().to_path_buf()).unwrap();
        workspace.write_text("note.txt", "hello").unwrap();

        let err = workspace
            .rename_entry("note.txt", "../escape.txt")
            .unwrap_err()
            .to_string();
        assert!(err.contains("separators"));

        let err = workspace
            .rename_entry("note.txt", "bad:name.txt")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Windows"));
    }
}
