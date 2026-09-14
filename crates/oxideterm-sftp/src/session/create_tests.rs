#[cfg(test)]
mod create_file_protocol_tests {
    use super::*;
    use russh_sftp::protocol::{Attrs, File, Handle, Name, Status};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct State { entries: HashMap<String, FileAttributes>, opens: usize, closes: usize }
    struct Server(Arc<Mutex<State>>);
    impl russh_sftp::server::Handler for Server {
        type Error = StatusCode;
        fn unimplemented(&self) -> Self::Error { StatusCode::OpUnsupported }
        async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
            // Model a server that normalizes even a nonexistent final component.
            Ok(Name { id, files: vec![File::dummy(path)] })
        }
        async fn open(&mut self, id: u32, filename: String, flags: OpenFlags, _: FileAttributes) -> Result<Handle, Self::Error> {
            let mut state = self.0.lock().unwrap();
            state.opens += 1;
            assert!(flags.contains(OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE));
            assert!(!flags.contains(OpenFlags::TRUNCATE));
            if state.entries.contains_key(&filename) { return Err(StatusCode::Failure); }
            if filename.ends_with("/denied") { return Err(StatusCode::PermissionDenied); }
            state.entries.insert(filename.clone(), FileAttributes::default());
            Ok(Handle { id, handle: filename })
        }
        async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
            let attrs = self.0.lock().unwrap().entries.get(&path).cloned().ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs { id, attrs })
        }
        async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> { self.lstat(id, path).await }
        async fn close(&mut self, id: u32, _: String) -> Result<Status, Self::Error> {
            self.0.lock().unwrap().closes += 1;
            Ok(Status { id, status_code: StatusCode::Ok, error_message: String::new(), language_tag: String::new() })
        }
    }

    #[tokio::test]
    async fn new_file_is_created_when_realpath_accepts_nonexistent_names() {
        let (client, server) = tokio::io::duplex(65536);
        let state = Arc::new(Mutex::new(State::default()));
        let task = tokio::spawn(russh_sftp::server::run(server, Server(state.clone())));
        let sftp = Arc::new(RusshSftpSession::new(client).await.unwrap());
        let session = SftpSession { sftp: sftp.clone(), session_id: "create-test".into(), home: "/root".into(), cwd: "/root".into(),
            channel_factory: Arc::new(|| Box::pin(async { Err(SftpError::ChannelError("unexpected sibling channel".into())) })) };
        let result = session.create_empty_file("/root/new.txt").await;
        assert!(result.is_ok(), "creation unexpectedly rejected: {result:?}");
        assert!(state.lock().unwrap().entries.contains_key("/root/new.txt"));
        assert_eq!(state.lock().unwrap().closes, 1);
        for (name, mode) in [("existing-file", 0o100644), ("existing-dir", 0o040755), ("dangling-link", 0o120777)] {
            let path = format!("/root/{name}");
            state.lock().unwrap().entries.insert(path.clone(), FileAttributes { permissions: Some(mode), size: Some(19), ..Default::default() });
            assert!(matches!(session.create_empty_file(&path).await, Err(SftpError::AlreadyExists(_))));
            let state = state.lock().unwrap();
            assert_eq!(state.entries[&path].size, Some(19));
            assert_eq!(state.entries[&path].permissions, Some(mode));
        }
        assert!(matches!(session.create_empty_file("/root/denied").await, Err(SftpError::PermissionDenied(_))));
        sftp.close().await.unwrap();
        task.abort();
        let _ = task.await;
    }
}
