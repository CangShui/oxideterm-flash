impl SftpSession {
    pub async fn new<O>(connection: O, session_id: String) -> Result<Self, SftpError>
    where
        O: SftpChannelOpener,
    {
        info!("正在为会话 {session_id} 打开 SFTP 子系统");
        // Store an erased channel factory so directory transfers can open
        // short-lived sibling SFTP channels without changing the public opener API.
        let channel_factory: SftpChannelFactory = Arc::new(move || {
            let connection = connection.clone();
            Box::pin(async move { connection.open_sftp_channel().await })
        });
        let sftp = open_russh_sftp_session(&channel_factory).await?;
        // Dropbear and other constrained servers may reject REALPATH for ".".
        // Keep the session usable by falling back to a listable absolute root.
        let cwd = resolve_initial_remote_cwd(&sftp).await;
        info!("会话 {session_id} 的 SFTP 子系统已打开");
        Ok(Self {
            sftp: Arc::new(sftp),
            channel_factory,
            session_id,
            home: cwd.clone(),
            cwd,
        })
    }

    async fn open_sibling_sftp(&self) -> Result<RusshSftpSession, SftpError> {
        open_russh_sftp_session(&self.channel_factory).await
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Returns the SFTP server's initial directory even after the file manager changes cwd.
    pub fn home(&self) -> &str {
        &self.home
    }

    pub fn set_cwd(&mut self, path: String) {
        self.cwd = path;
    }

    pub async fn canonicalize(&self, path: &str) -> Result<String, SftpError> {
        self.resolve_path(path).await
    }

    pub async fn list_dir(
        &self,
        path: &str,
        filter: Option<ListFilter>,
    ) -> Result<Vec<FileInfo>, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        self.list_dir_resolved(&canonical_path, filter).await
    }

    pub async fn list_dir_with_cwd(
        &self,
        path: &str,
        filter: Option<ListFilter>,
    ) -> Result<(String, Vec<FileInfo>), SftpError> {
        let canonical_path = match self.resolve_path(path).await {
            Ok(path) => path,
            Err(error) => {
                // Some SFTP servers, including OpenWrt Dropbear pairings, can
                // list a directory they cannot canonicalize.
                if path.is_empty() {
                    self.cwd.clone()
                } else if is_absolute_remote_path(path) {
                    path.to_string()
                } else {
                    return Err(error);
                }
            }
        };
        let entries = self.list_dir_resolved(&canonical_path, filter).await?;
        Ok((canonical_path, entries))
    }

    async fn list_dir_resolved(
        &self,
        canonical_path: &str,
        filter: Option<ListFilter>,
    ) -> Result<Vec<FileInfo>, SftpError> {
        debug!("正在读取 SFTP 目录：{canonical_path}");
        let read_dir = self
            .sftp
            .read_dir(canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, canonical_path))?;
        let mut entries = Vec::new();
        let mut skipped_invalid_entries = 0usize;

        for entry in read_dir {
            let name = entry.file_name();
            match list_entry_action(&name, filter.as_ref().is_some_and(|f| f.show_hidden)) {
                ListEntryAction::Keep => {}
                ListEntryAction::SkipInvalid => {
                    // One unusual remote name (for example a component with a
                    // backslash) must not make a whole folder unreadable. Such
                    // entries stay out of transfer paths, so skipping them is
                    // safe and keeps the rest of the directory browsable.
                    skipped_invalid_entries += 1;
                    continue;
                }
                ListEntryAction::SkipDot | ListEntryAction::SkipHidden => continue,
            }

            let full_path = join_remote_path(canonical_path, &name);
            let metadata = entry.metadata();
            let entry_file_type = file_type_from_attrs(&metadata);
            let (symlink_target, target_file_type) = if entry_file_type == FileType::Symlink {
                let symlink_target = self.sftp.read_link(&full_path).await.ok();
                let target_file_type = self
                    .sftp
                    .metadata(&full_path)
                    .await
                    .ok()
                    .map(|target_metadata| file_type_from_attrs(&target_metadata));
                (symlink_target, target_file_type)
            } else {
                (None, None)
            };
            let file_type = classify_list_entry_file_type(entry_file_type, target_file_type);
            entries.push(FileInfo {
                name,
                path: full_path,
                file_type,
                size: metadata.size.unwrap_or(0),
                modified: metadata.mtime.map(|mtime| mtime as i64).unwrap_or(0),
                permissions: metadata
                    .permissions
                    .map(|permissions| format!("{:o}", permissions & 0o777))
                    .unwrap_or_else(|| "000".to_string()),
                owner: metadata.uid.map(|uid| uid.to_string()),
                group: metadata.gid.map(|gid| gid.to_string()),
                is_symlink: entry_file_type == FileType::Symlink,
                symlink_target,
            });
        }
        if skipped_invalid_entries > 0 {
            info!(
                skipped_invalid_entries,
                path = canonical_path,
                "目录读取完成；名称不安全的条目已跳过，未使整个目录加载失败"
            );
        }

        if let Some(pattern) = filter.as_ref().and_then(|filter| filter.pattern.as_ref())
            && let Ok(glob_pattern) = glob::Pattern::new(pattern)
        {
            entries.retain(|entry| glob_pattern.matches(&entry.name));
        }

        let sort_order = filter
            .as_ref()
            .map(|filter| filter.sort)
            .unwrap_or_default();
        sort_entries(&mut entries, sort_order);
        Ok(entries)
    }

    pub async fn stat(&self, path: &str) -> Result<FileInfo, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        let name = Path::new(&canonical_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_type = file_type_from_attrs(&metadata);
        let symlink_target = if file_type == FileType::Symlink {
            self.sftp.read_link(&canonical_path).await.ok()
        } else {
            None
        };
        Ok(FileInfo {
            name,
            path: canonical_path,
            file_type,
            size: metadata.size.unwrap_or(0),
            modified: metadata.mtime.map(|mtime| mtime as i64).unwrap_or(0),
            permissions: metadata
                .permissions
                .map(|permissions| format!("{:o}", permissions & 0o777))
                .unwrap_or_else(|| "000".to_string()),
            owner: metadata.uid.map(|uid| uid.to_string()),
            group: metadata.gid.map(|gid| gid.to_string()),
            is_symlink: file_type == FileType::Symlink,
            symlink_target,
        })
    }

    pub async fn read_file_bytes(&self, path: &str) -> Result<Vec<u8>, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if metadata.is_dir() {
            return Err(SftpError::DirectoryNotFound(canonical_path));
        }
        self.read_file_limited(
            &canonical_path,
            metadata.size.unwrap_or(0).try_into().unwrap_or(usize::MAX),
        )
        .await
    }

    /// Reads one bounded range without allocating for the complete remote file.
    pub async fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<(String, u64, Zeroizing<Vec<u8>>), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if metadata.is_dir() {
            return Err(SftpError::DirectoryNotFound(canonical_path));
        }
        let total_size = metadata.size.unwrap_or(0);
        if offset > total_size {
            return Err(SftpError::InvalidPath(format!(
                "offset exceeds remote file size: {canonical_path}"
            )));
        }
        let read_limit =
            (total_size - offset).min(u64::try_from(maximum_bytes).unwrap_or(u64::MAX));
        let mut file = self
            .sftp
            .open(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut bytes = Zeroizing::new(Vec::with_capacity(
            usize::try_from(read_limit).unwrap_or(maximum_bytes),
        ));
        file.take(read_limit)
            .read_to_end(&mut bytes)
            .await
            .map_err(SftpError::IoError)?;
        Ok((canonical_path, total_size, bytes))
    }

    /// Resolves an existing path or a new file beneath its canonical parent.
    pub async fn canonicalize_write_target(&self, path: &str) -> Result<String, SftpError> {
        match self.resolve_path(path).await {
            Ok(path) => Ok(path),
            Err(_) => self.resolve_new_file_path(path).await,
        }
    }

    pub async fn write_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<WriteContentResult, SftpError> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        debug!(
            target: "oxideterm::audit",
            operation_id,
            stage = "sftp.write.request",
            content_bytes = content.len(),
            "远程文件写入请求已进入 SFTP 会话，开始原子写入"
        );
        let canonical_path = match self.resolve_path(path).await {
            Ok(path) => path,
            Err(_) => self.resolve_new_file_path(path).await?,
        };
        let swap_path = swap_path(&canonical_path);
        match self
            .write_to_swap_and_rename(&canonical_path, &swap_path, content)
            .await
        {
            Ok(()) => {
                debug!(
                    target: "oxideterm::audit",
                    operation_id,
                    stage = "sftp.write.response",
                    atomic_write = true,
                    content_bytes = content.len(),
                    result = "completed",
                    "远程文件已通过原子写入保存"
                );
                Ok(WriteContentResult { atomic_write: true })
            }
            Err(error) => {
                let error_string = error.to_string();
                let recoverable = matches!(error, SftpError::PermissionDenied(_))
                    || error_string.contains(".oxswp")
                    || error_string.contains("Atomic rename failed");
                if !recoverable {
                    warn!(
                        target: "oxideterm::audit",
                        operation_id,
                        stage = "sftp.write.response",
                        result = "failed",
                        failure_detail_redacted = true,
                        "远程文件原子写入失败，且该错误不可回退，写入未完成"
                    );
                    return Err(error);
                }
                warn!(
                    target: "oxideterm::audit",
                    operation_id,
                    stage = "sftp.write.fallback",
                    result = "fallback",
                    "SFTP 原子写入 {canonical_path} 失败（{error_string}），回退为直接覆盖写入"
                );
                match self.write_direct(&canonical_path, content).await {
                    Ok(()) => {
                        debug!(
                            target: "oxideterm::audit",
                            operation_id,
                            stage = "sftp.write.response",
                            atomic_write = false,
                            content_bytes = content.len(),
                            result = "completed",
                            "远程文件已通过直接覆盖写入保存"
                        );
                        Ok(WriteContentResult {
                            atomic_write: false,
                        })
                    }
                    Err(error) => {
                        warn!(
                            target: "oxideterm::audit",
                            operation_id,
                            stage = "sftp.write.response",
                            result = "failed",
                            failure_detail_redacted = true,
                            "远程文件直接覆盖写入也失败，写入未完成"
                        );
                        Err(error)
                    }
                }
            }
        }
    }

    /// Creates a new empty remote file without replacing an existing file or directory.
    pub async fn create_empty_file(&self, path: &str) -> Result<(), SftpError> {
        let operation_id = uuid::Uuid::new_v4().to_string();
        debug!(target: "oxideterm::audit", operation_id, stage = "sftp.create.request",
            path_length = path.chars().count(), "创建远端文件：只解析父目录，由独占创建确认是否存在");
        // REALPATH can succeed for a missing final component. Only OPEN with
        // CREATE|EXCLUDE can atomically establish whether this name is available.
        let canonical_path = match self.resolve_new_file_path(path).await {
            Ok(path) => path,
            Err(error) => {
                warn!(target: "oxideterm::audit", operation_id, stage = "sftp.create.parent",
                    result = "failed", "父目录解析失败，未发送文件创建请求");
                return Err(error);
            }
        };
        match self.sftp.open_with_flags(
            &canonical_path, OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
        ).await {
            Ok(mut file) => {
                // Wait for CLOSE acknowledgement rather than relying on an asynchronous drop.
                let result = file.shutdown().await.map_err(|error| SftpError::WriteError(error.to_string()));
                debug!(target: "oxideterm::audit", operation_id, stage = "sftp.create.response",
                    created = true, close_acknowledged = result.is_ok(), "新文件已独占创建，等待关闭确认后返回结果");
                result
            }
            Err(error) => {
                let mapped = self.map_sftp_error(error, &canonical_path);
                // LSTAT detects even dangling symlinks; never follow a final link
                // or truncate an existing entry while classifying a v3 failure.
                let exists = self.sftp.symlink_metadata(&canonical_path).await.is_ok();
                warn!(target: "oxideterm::audit", operation_id, stage = "sftp.create.response",
                    result = "failed", existing_entry_confirmed = exists,
                    permission_denied = matches!(&mapped, SftpError::PermissionDenied(_)),
                    "独占创建失败，单独检查目标是否存在；不把路径解析成功当作同名冲突");
                if exists { Err(SftpError::AlreadyExists(canonical_path)) } else { Err(mapped) }
            }
        }
    }

    /// Replaces a user configuration file without following-path or metadata loss.
    pub async fn replace_config_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<WriteContentResult, SftpError> {
        let canonical_path = match self.resolve_path(path).await {
            Ok(path) => path,
            Err(_) => self.resolve_new_file_path(path).await?,
        };
        let metadata = self.sftp.metadata(&canonical_path).await.ok();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let swap_path = format!("{canonical_path}.oxideterm-{suffix}.tmp");
        let backup_path = format!("{canonical_path}.oxideterm-{suffix}.bak");

        self.write_direct(&swap_path, content).await?;
        let written = self.read_file_limited(&swap_path, content.len()).await?;
        if written != content {
            let _ = self.sftp.remove_file(&swap_path).await;
            return Err(SftpError::WriteError(format!(
                "Remote configuration verification failed for {canonical_path}"
            )));
        }
        if let Some(metadata) = metadata.as_ref() {
            let preserved = FileAttributes {
                uid: metadata.uid,
                gid: metadata.gid,
                permissions: metadata.permissions,
                ..FileAttributes::empty()
            };
            if let Err(error) = self.sftp.set_metadata(&swap_path, preserved).await {
                let _ = self.sftp.remove_file(&swap_path).await;
                return Err(SftpError::WriteError(format!(
                    "Failed to preserve metadata for {canonical_path}: {error}"
                )));
            }
            if let Err(error) = self.sftp.rename(&canonical_path, &backup_path).await {
                let _ = self.sftp.remove_file(&swap_path).await;
                return Err(self.map_sftp_error(error, &canonical_path));
            }
        }

        if let Err(error) = self.sftp.rename(&swap_path, &canonical_path).await {
            let rollback_error = if metadata.is_some() {
                self.sftp.rename(&backup_path, &canonical_path).await.err()
            } else {
                None
            };
            let _ = self.sftp.remove_file(&swap_path).await;
            if let Some(rollback_error) = rollback_error {
                return Err(SftpError::WriteError(format!(
                    "Failed to replace {canonical_path}: {error}; rollback failed: {rollback_error}. The original file remains at {backup_path}"
                )));
            }
            return Err(SftpError::WriteError(format!(
                "Failed to replace {canonical_path}: {error}"
            )));
        }
        if metadata.is_some() {
            let _ = self.sftp.remove_file(&backup_path).await;
        }
        Ok(WriteContentResult { atomic_write: true })
    }
}

/// Decides whether one SFTP directory entry participates in the visible list.
enum ListEntryAction {
    Keep,
    /// Dot entries are never shown.
    SkipDot,
    /// Hidden entries are skipped only while hidden files are not requested.
    SkipHidden,
    /// Entries that could escape the selected destination on transfer.
    SkipInvalid,
}

fn list_entry_action(name: &str, show_hidden: bool) -> ListEntryAction {
    if name == "." || name == ".." {
        return ListEntryAction::SkipDot;
    }
    if validate_remote_entry_name(name).is_err() {
        return ListEntryAction::SkipInvalid;
    }
    if !show_hidden && name.starts_with('.') {
        return ListEntryAction::SkipHidden;
    }
    ListEntryAction::Keep
}

#[cfg(test)]
mod list_entry_action_tests {
    use super::{ListEntryAction, list_entry_action};

    #[test]
    fn backslash_named_entry_is_skipped_but_never_aborts_the_listing() {
        assert!(matches!(
            list_entry_action(r"C:\Program Files\Brave-Browser\Application", false),
            ListEntryAction::SkipInvalid
        ));
        assert!(matches!(
            list_entry_action("normal-folder", false),
            ListEntryAction::Keep
        ));
    }

    #[test]
    fn hidden_entries_follow_the_show_hidden_filter() {
        assert!(matches!(
            list_entry_action(".codex", false),
            ListEntryAction::SkipHidden
        ));
        assert!(matches!(
            list_entry_action(".codex", true),
            ListEntryAction::Keep
        ));
    }

    #[test]
    fn dot_entries_are_never_shown() {
        assert!(matches!(
            list_entry_action(".", true),
            ListEntryAction::SkipDot
        ));
        assert!(matches!(
            list_entry_action("..", true),
            ListEntryAction::SkipDot
        ));
    }
}
