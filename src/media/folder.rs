use std::path::{Path, PathBuf};

/// Supported image file extensions for automatic filtering.
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tga", "exr", "ico",
];

/// An entry in a [`Folder`]: the filename stem and the full canonical path.
#[derive(Debug, Clone)]
pub struct FolderEntry {
    /// The filename *with* extension, e.g. `"sunset.jpg"`.
    pub name: String,
    /// Full path on disk.
    pub path: PathBuf,
}

/// A `Folder` value holds all image files found inside a directory.
/// Non-image files are silently filtered out at construction time.
///
/// ```drive
/// photos = folder("./assets/photos/");
/// for img in photos { ... }
/// img = photos["sunset.jpg"];
/// ```
#[derive(Debug, Clone)]
pub struct Folder {
    /// The original directory path given by the user.
    pub dir: PathBuf,
    /// Sorted list of image entries.
    entries: Vec<FolderEntry>,
}

#[derive(Debug)]
pub enum FolderError {
    /// The path does not exist or is not a directory.
    NotADirectory(PathBuf),
    /// OS I/O error while reading the directory.
    Io(std::io::Error),
}

impl std::fmt::Display for FolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FolderError::NotADirectory(p) => {
                write!(f, "'{}' is not a directory", p.display())
            }
            FolderError::Io(e) => write!(f, "folder I/O error: {e}"),
        }
    }
}

impl From<std::io::Error> for FolderError {
    fn from(e: std::io::Error) -> Self {
        FolderError::Io(e)
    }
}

impl Folder {
    /// Open a directory and collect all image files inside it (non-recursive).
    /// Files whose extension is not in [`IMAGE_EXTENSIONS`] are silently skipped.
    pub fn open<P: AsRef<Path>>(dir: P) -> Result<Self, FolderError> {
        let dir = dir.as_ref().to_path_buf();
        if !dir.is_dir() {
            return Err(FolderError::NotADirectory(dir));
        }

        let mut entries: Vec<FolderEntry> = std::fs::read_dir(&dir)?
            .filter_map(|res| res.ok())
            .filter(|e| e.path().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .map(|e| FolderEntry {
                name: e
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
                path: e.path(),
            })
            .collect();

        // Sort by name so iteration order is deterministic
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Folder { dir, entries })
    }

    /// All image entries in alphabetical order.
    pub fn entries(&self) -> &[FolderEntry] {
        &self.entries
    }

    /// Look up an entry by its filename (e.g. `"sunset.jpg"`).
    pub fn get(&self, name: &str) -> Option<&FolderEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Number of image files in this folder.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the folder contains no image files.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
