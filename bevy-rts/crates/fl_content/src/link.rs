//! The link pass context: cross-reference lookups and path-aware error reporting.

use std::path::Path;

use crate::db::ContentDb;
use crate::error::ContentError;
use crate::kinds::ContentKind;

/// Context handed to [`DefComponent::validate`](crate::DefComponent::validate) and
/// [`ContentKind::link`] during the link pass. Errors are attributed to the item's file and
/// prefixed with the current scope.
pub struct LinkCtx<'a> {
    db: &'a ContentDb,
    file: &'a Path,
    scope: String,
    errors: &'a mut Vec<ContentError>,
}

impl<'a> LinkCtx<'a> {
    /// A context looking things up in `db` and reporting into `errors` for `file`, scoped to
    /// `scope`: an item id (`"base.red"`) or a component (`"base.de.pz3/components.Hull"`).
    pub(crate) fn new(
        db: &'a ContentDb,
        file: &'a Path,
        scope: impl Into<String>,
        errors: &'a mut Vec<ContentError>,
    ) -> Self {
        LinkCtx {
            db,
            file,
            scope: scope.into(),
            errors,
        }
    }

    /// Report an error at `path`, relative to the current scope (`""` for the scope itself).
    ///
    /// The path joins an item scope with `/` (`base.red/roster[0]`) and a component scope
    /// with `.` (`base.de.pz3/components.Hull.hp`).
    pub fn error(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        let path = path.into();
        let full = if self.scope.is_empty() {
            path
        } else if path.is_empty() {
            self.scope.clone()
        } else if self.scope.contains('/') {
            format!("{}.{path}", self.scope)
        } else {
            format!("{}/{path}", self.scope)
        };
        self.errors.push(ContentError {
            file: self.file.to_path_buf(),
            line_col: None,
            path: full,
            message: msg.into(),
        });
    }

    /// Whether a unit with this id exists, concrete or abstract. A unit whose `inherits`
    /// chain failed counts as existing: its own error is already reported.
    pub fn unit_exists(&self, id: &str) -> bool {
        self.db.unit_id(id).is_some()
            || self.db.unit_is_abstract(id)
            || self.db.unit_is_unresolved(id)
    }

    /// Whether `id` is an abstract unit template.
    pub fn unit_is_abstract(&self, id: &str) -> bool {
        self.db.unit_is_abstract(id)
    }

    /// The loaded item of kind `K` with this id; `None` if it does not exist or `K` is not
    /// registered.
    pub fn kind_get<K: ContentKind>(&self, id: &str) -> Option<&K> {
        self.db.kind_table::<K>()?.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::DefRegistry;

    #[test]
    fn error_paths_are_scoped() {
        let db = ContentDb::empty(DefRegistry::default());
        let mut errors = Vec::new();
        let file = Path::new("u.unit.ron");
        let mut ctx = LinkCtx::new(&db, file, "a.b/components.Hull", &mut errors);
        ctx.error("hp", "must be positive");
        ctx.error("", "whole component");
        let mut item = LinkCtx::new(&db, file, "a.red", &mut errors);
        item.error("roster[0]", "missing");
        let mut root = LinkCtx::new(&db, file, "", &mut errors);
        root.error("x", "root");
        let paths: Vec<&str> = errors.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "a.b/components.Hull.hp",
                "a.b/components.Hull",
                "a.red/roster[0]",
                "x"
            ]
        );
        assert_eq!(errors[0].message, "must be positive");
        assert_eq!(errors[0].file, file);
    }
}
