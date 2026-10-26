//! Content errors that carry the file, location and field path.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::tree::Node;

/// One content error, pointing at a file, an optional position and a field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentError {
    pub file: PathBuf,
    /// 1-based `(line, column)` of the value the error is about, when known.
    pub line_col: Option<(usize, usize)>,
    /// Field path such as `"base.de.pz3/components.Hull"`; empty when unknown.
    pub path: String,
    pub message: String,
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())?;
        if let Some((line, col)) = self.line_col {
            write!(f, ":{line}:{col}")?;
        }
        if !self.path.is_empty() {
            write!(f, ": {}", self.path)?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ContentError {}

/// A collection of content errors; loading reports all of them at once.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContentErrors(pub Vec<ContentError>);

impl fmt::Display for ContentErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{err}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ContentErrors {}

impl From<ContentError> for ContentErrors {
    fn from(err: ContentError) -> Self {
        ContentErrors(vec![err])
    }
}

impl ContentError {
    /// An error about `file` as a whole, or at `path` inside it, without a position.
    pub(crate) fn at(file: &Path, path: impl Into<String>, message: impl Into<String>) -> Self {
        ContentError {
            file: file.to_path_buf(),
            line_col: None,
            path: path.into(),
            message: message.into(),
        }
    }

    /// An error at `path` inside `file`, at the position where `node` starts.
    pub(crate) fn at_node(
        file: &Path,
        path: impl Into<String>,
        node: &Node,
        message: impl Into<String>,
    ) -> Self {
        ContentError {
            file: file.to_path_buf(),
            line_col: Some((node.span.line as usize, node.span.col as usize)),
            path: path.into(),
            message: message.into(),
        }
    }

    /// A RON parse error in `file`, with its position.
    pub(crate) fn parse(file: &Path, err: &ron::error::SpannedError) -> Self {
        ContentError {
            file: file.to_path_buf(),
            line_col: Some((err.span.start.line, err.span.start.col)),
            path: String::new(),
            message: err.code.to_string(),
        }
    }
}

/// Read `file` as UTF-8 text.
pub(crate) fn read_text(file: &Path) -> Result<String, ContentError> {
    let bytes =
        std::fs::read(file).map_err(|e| ContentError::at(file, "", format!("cannot read: {e}")))?;
    String::from_utf8(bytes)
        .map_err(|e| ContentError::at(file, "", format!("not valid UTF-8: {e}")))
}
