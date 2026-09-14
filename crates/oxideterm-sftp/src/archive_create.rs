use crate::{ArchiveKind, archive_kind, shell_quote};

/// Programs are fixed by format; never derive executable names from user input.
pub fn archive_tools(kind: ArchiveKind, extract: bool) -> &'static [&'static str] {
    match kind {
        ArchiveKind::Zip if extract => &["unzip"],
        ArchiveKind::Zip => &["zip"],
        ArchiveKind::SevenZip => &["7z"],
        ArchiveKind::Rar => &["unrar"],
        ArchiveKind::Tar => &["tar"],
        ArchiveKind::TarGzip => &["tar", "gzip"],
        ArchiveKind::TarBzip2 => &["tar", "bzip2"],
        ArchiveKind::TarXz => &["tar", "xz"],
        ArchiveKind::TarZstd => &["tar", "zstd"],
    }
}

/// Selected paths are single siblings. Relative ./ prefixes prevent option injection.
pub fn plan_archive_creation(
    directory: &str,
    names: &[String],
    destination: &str,
) -> Result<(ArchiveKind, String), &'static str> {
    if !directory.starts_with('/')
        || !destination.starts_with('/')
        || names.is_empty()
        || directory.contains('\0')
        || destination.contains('\0')
        || destination.split('/').any(|part| part == "." || part == "..")
    {
        return Err("invalid archive paths");
    }
    for name in names {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
            return Err("invalid archive source");
        }
        let source = format!("{}/{}", directory.trim_end_matches('/'), name);
        if destination == source || destination.starts_with(&format!("{source}/")) {
            return Err("archive output is inside its source");
        }
    }
    let kind = archive_kind(destination).ok_or("unsupported archive format")?;
    let destination = shell_quote(destination);
    let sources = names
        .iter()
        .map(|name| shell_quote(&format!("./{name}")))
        .collect::<Vec<_>>()
        .join(" ");
    let command = match kind {
        ArchiveKind::Tar => format!("tar -cf {destination} -- {sources}"),
        ArchiveKind::TarGzip => format!("tar -czf {destination} -- {sources}"),
        ArchiveKind::TarBzip2 => format!("tar -cjf {destination} -- {sources}"),
        ArchiveKind::TarXz => format!("tar -cJf {destination} -- {sources}"),
        ArchiveKind::TarZstd => format!("tar --zstd -cf {destination} -- {sources}"),
        ArchiveKind::Zip => format!("zip -r {destination} {sources}"),
        ArchiveKind::SevenZip => format!("7z a -t7z {destination} -- {sources} < /dev/null"),
        ArchiveKind::Rar => return Err("RAR creation is not supported"),
    };
    // Never silently update an existing archive or replace an existing symbolic link.
    Ok((
        kind,
        format!(
            "cd {} || exit 74; if [ -e {destination} ] || [ -L {destination} ]; then exit 73; fi; {command}",
            shell_quote(directory)
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn tar_roundtrip_keeps_selected_names_and_does_not_replace_existing_output() {
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
        }
        let temp = Temp(std::env::temp_dir().join(format!("archive test'{}", uuid::Uuid::new_v4())));
        std::fs::create_dir_all(temp.0.join("source dir")).unwrap();
        std::fs::write(temp.0.join("source dir/a'b.txt"), b"payload").unwrap();
        std::fs::write(temp.0.join("-option.txt"), b"second").unwrap();
        let output = temp.0.join("result.tar.gz");
        let (_, command) = plan_archive_creation(
            temp.0.to_str().unwrap(),
            &["source dir".into(), "-option.txt".into()],
            output.to_str().unwrap(),
        ).unwrap();
        let run = |command: &str| std::process::Command::new("/bin/sh")
            .args(["-c", command]).output().unwrap();
        assert!(run(&command).status.success());
        let original_archive = std::fs::read(&output).unwrap();
        assert!(!run(&command).status.success());
        assert_eq!(std::fs::read(&output).unwrap(), original_archive);
        let destination = temp.0.join("unpacked");
        std::fs::create_dir_all(&destination).unwrap();
        let plan = crate::plan_archive_extraction(
            "result.tar.gz", output.to_str().unwrap(), destination.to_str().unwrap(),
        ).unwrap();
        assert!(run(&plan.command).status.success());
        assert_eq!(std::fs::read(destination.join("source dir/a'b.txt")).unwrap(), b"payload");
        assert_eq!(std::fs::read(destination.join("-option.txt")).unwrap(), b"second");
    }

    #[test]
    fn archive_creation_quotes_sources_and_rejects_self_inclusion() {
        let (_, command) = plan_archive_creation(
            "/tmp/a'b",
            &["-flag; touch injected".into()],
            "/tmp/result.zip",
        )
        .unwrap();
        assert!(command.contains("'./-flag; touch injected'"));
        assert!(command.contains("'/tmp/a'\\''b'"));
        assert!(plan_archive_creation("/tmp", &["source".into()], "/tmp/source/out.tar").is_err());
        assert!(plan_archive_creation("/tmp", &["../source".into()], "/tmp/out.zip").is_err());
        assert!(plan_archive_creation("/tmp", &["source".into()], "/tmp/out.rar").is_err());
    }
    #[test]
    fn missing_tools_are_checked_per_format() {
        assert_eq!(archive_tools(ArchiveKind::Zip, true), &["unzip"]);
        assert_eq!(archive_tools(ArchiveKind::Zip, false), &["zip"]);
        assert_eq!(crate::archive_kind("DATA.7Z"), Some(ArchiveKind::SevenZip));
        assert_eq!(crate::archive_kind("DATA.RAR"), Some(ArchiveKind::Rar));
    }
}
