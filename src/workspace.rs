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
}
