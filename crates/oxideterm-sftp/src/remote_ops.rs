use thiserror::Error;

use crate::{normalize_remote_path, shell_quote};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RemoteCopyPlanError {
    #[error("source and destination are the same path")]
    SamePath,
    #[error("a directory cannot be copied into itself")]
    DirectoryIntoItself,
}

/// Builds the explicit remote-side copy command used by the SFTP file browser.
///
/// SFTP itself has no portable recursive-copy request, so the connected SSH
/// transport executes one safely quoted `cp` command without staging data on
/// the local machine.
pub fn plan_remote_copy_command(
    source_path: &str,
    destination_path: &str,
    source_is_directory: bool,
) -> Result<String, RemoteCopyPlanError> {
    let source = normalize_remote_path(source_path);
    let destination = normalize_remote_path(destination_path);
    if source == destination {
        return Err(RemoteCopyPlanError::SamePath);
    }
    if source_is_directory {
        let source_prefix = format!("{}/", source.trim_end_matches('/'));
        if destination.starts_with(&source_prefix) {
            return Err(RemoteCopyPlanError::DirectoryIntoItself);
        }
    }
    Ok(format!(
        "cp -a -- {} {}",
        shell_quote(&source),
        shell_quote(&destination)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_copy_quotes_paths_as_shell_data() {
        let command =
            plan_remote_copy_command("/srv/a file's.txt", "/srv/dest/a file's.txt", false)
                .expect("different file paths can be copied");

        assert_eq!(
            command,
            "cp -a -- '/srv/a file'\\''s.txt' '/srv/dest/a file'\\''s.txt'"
        );
    }

    #[test]
    fn remote_copy_rejects_copying_directory_into_itself() {
        assert_eq!(
            plan_remote_copy_command("/srv/tree", "/srv/tree/child/tree", true),
            Err(RemoteCopyPlanError::DirectoryIntoItself)
        );
    }

    #[test]
    fn remote_copy_rejects_same_path_after_normalization() {
        assert_eq!(
            plan_remote_copy_command("srv/file", "/srv/file", false),
            Err(RemoteCopyPlanError::SamePath)
        );
    }
}
