use std::path::PathBuf;

pub(crate) fn default_ssh_dir() -> Option<PathBuf> {
    local_home_dir().map(|home| home.join(".ssh"))
}

pub(crate) fn local_home_dir() -> Option<PathBuf> {
    // Prefer the platform profile directory so Windows does not inherit an
    // unrelated HOME value from shells or build environments.
    dirs::home_dir().or_else(std::env::home_dir)
}
