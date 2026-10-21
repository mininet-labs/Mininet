//! Typed source-graph traversal without materializing a checkout or exporting Git.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceNodeKind {
    Commit,
    Tree,
    File,
}

/// Validate one source object's shape and return its typed dependencies.
/// Integrity/provenance, graph completeness, traversal bounds and owner sharing
/// permission remain the caller's responsibility. No branch, merge or release changes.
pub fn source_dependencies(
    object: &Object,
    expected: SourceNodeKind,
) -> Result<Vec<(ObjectId, SourceNodeKind)>> {
    let Payload::Public(_) = &object.payload else {
        return Err(ForgeError::BadObject);
    };
    match expected {
        SourceNodeKind::Commit => {
            validated_commit_tree(object)?;
            Ok(object
                .links
                .iter()
                .map(|l| {
                    (
                        l.target.clone(),
                        if l.rel == "tree" {
                            SourceNodeKind::Tree
                        } else {
                            SourceNodeKind::Commit
                        },
                    )
                })
                .collect())
        }
        SourceNodeKind::Tree => {
            let entries = read_tree(object)?;
            let mut declared: Vec<_> = object.links.iter().map(|l| l.target.as_str()).collect();
            let mut actual: Vec<_> = entries.iter().map(|e| e.target.as_str()).collect();
            declared.sort_unstable();
            actual.sort_unstable();
            if object.links.iter().any(|l| l.rel != "entry") || declared != actual {
                return Err(ForgeError::BadObject);
            }
            Ok(entries
                .into_iter()
                .map(|e| {
                    (
                        e.target,
                        if e.is_dir {
                            SourceNodeKind::Tree
                        } else {
                            SourceNodeKind::File
                        },
                    )
                })
                .collect())
        }
        SourceNodeKind::File => {
            if !object.links.is_empty()
                || (object.object_type != ObjectType::Custom(FILE_TYPE.into())
                    && object.object_type != ObjectType::Custom(EXECUTABLE_FILE_TYPE.into()))
            {
                return Err(ForgeError::BadObject);
            }
            Ok(Vec::new())
        }
    }
}
