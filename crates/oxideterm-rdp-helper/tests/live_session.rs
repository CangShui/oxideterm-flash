//! Live-session diagnostic harness.
//!
//! Connects the real helper binary to a reachable RDP server, composites the
//! streamed frame events exactly like the app-side frame surface does, and
//! dumps the composited framebuffer to disk for visual inspection.
//!
//! Gated behind OXIDETERM_RDP_LIVE_TARGET so `cargo test` never dials out.

use std::io::{BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use oxideterm_remote_desktop::{
    RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate, RemoteDesktopHelperEvent, RemoteDesktopRect,
    RemoteDesktopSize, read_event_line,
};

struct Canvas {
    size: RemoteDesktopSize,
    bytes: Vec<u8>,
}

impl Canvas {
    fn new(size: RemoteDesktopSize) -> Self {
        let len = size.width as usize * size.height as usize * 4;
        Self {
            size,
            bytes: vec![0x20; len],
        }
    }

    fn apply(&mut self, update: &RemoteDesktopFrameUpdate) {
        if update.size.width != self.size.width || update.size.height != self.size.height {
            // A resize invalidates the canvas; rebuild at the new size.
            *self = Self::new(update.size);
        }
        let bytes_per_pixel = update.format.bytes_per_pixel();
        let row_len = update.size.width as usize * bytes_per_pixel;
        let rect_x = usize::try_from(update.rect.x).unwrap_or(0);
        let rect_y = usize::try_from(update.rect.y).unwrap_or(0);
        let rect_w = usize::try_from(update.rect.width).unwrap_or(0);
        let rect_h = usize::try_from(update.rect.height).unwrap_or(0);
        for row in 0..rect_h {
            let src_start = row * row_len;
            let src_end = src_start + rect_w * bytes_per_pixel;
            if src_end > update.bytes.len() {
                return;
            }
            let dst_start = (rect_y + row) * self.size.width as usize * 4 + rect_x * 4;
            let dst_end = dst_start + rect_w * 4;
            if dst_end > self.bytes.len() {
                return;
            }
            for (src, dst) in update.bytes[src_start..src_end]
                .chunks_exact(bytes_per_pixel)
                .zip(self.bytes[dst_start..dst_end].chunks_exact_mut(4))
            {
                // Normalize every format into RGBA for the dump.
                match update.format {
                    RemoteDesktopFrameFormat::Rgba8 => dst.copy_from_slice(src),
                    RemoteDesktopFrameFormat::Bgra8 => {
                        dst[0] = src[2];
                        dst[1] = src[1];
                        dst[2] = src[0];
                        dst[3] = src[3];
                    }
                }
            }
        }
    }
}

#[test]
fn live_rdp_session_renders_a_readable_desktop() {
    let Ok(target) = std::env::var("OXIDETERM_RDP_LIVE_TARGET") else {
        // Not a real test run: skip silently so CI stays hermetic.
        return;
    };
    let (host, port) = match target.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse::<u16>().unwrap_or(3389)),
        None => (target, 3389),
    };
    let username = std::env::var("OXIDETERM_RDP_LIVE_USER").unwrap_or_default();
    let password = std::env::var("OXIDETERM_RDP_LIVE_PASSWORD").unwrap_or_default();
    let dump_prefix =
        std::env::var("OXIDETERM_RDP_LIVE_DUMP").unwrap_or_else(|_| "/tmp/rdp-live".to_string());

    let mut helper = Command::new(env!("CARGO_BIN_EXE_oxideterm-rdp-helper"))
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("helper binary should spawn");
    let mut stdin = helper.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(helper.stdout.take().expect("stdout"));

    // The helper expects StartConnect first, then Connect once the user
    // confirms the certificate challenge.
    let start_connect = format!(
        r#"{{"type":"startConnect","protocol":"rdp","endpoint":{{"host":"{host}","port":{port}}},"size":{{"width":1024,"height":768}},"readOnly":false}}"#,
    );
    stdin
        .write_all(start_connect.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .expect("start-connect request");
    stdin.flush().ok();

    // A quiet desktop stops streaming; the blocked read must still yield.
    // A watchdog kills the helper after the capture window so the composited
    // framebuffer can be dumped from a blocked read.
    let watchdog = {
        let pid = helper.id();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(25));
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .output();
        })
    };
    let mut canvas: Option<Canvas> = None;
    let mut frames = 0usize;
    let mut updates = 0usize;
    let mut _last_frame_at: Option<Instant> = None;

    loop {
        // Frames stream as length-prefixed binary records; read_event_line
        // demuxes JSON lines and binary payloads on one stream.
        let event = match read_event_line(&mut stdout) {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(error) => {
                eprintln!("stdout read failed: {error}");
                break;
            }
        };
        match event {
            RemoteDesktopHelperEvent::Status { status, message } => {
                eprintln!("[status] {status:?} {message:?}");
            }
            RemoteDesktopHelperEvent::Connected { size } => {
                eprintln!("[connected] {size:?}");
                canvas = Some(Canvas::new(size));
            }
            RemoteDesktopHelperEvent::ServerCertificate {
                certificate,
            } => {
                let (challenge_id, sha256_fingerprint) = (
                    certificate.challenge_id.clone(),
                    certificate.sha256_fingerprint.clone(),
                );
                eprintln!("[certificate] challenge {challenge_id}");
                // Test environment: trust the presented certificate blindly.
                let authenticate = format!(
                    r#"{{"type":"authenticate","challengeId":{challenge},"sha256Fingerprint":{fingerprint},"username":{username_json},"password":{password_json},"domain":null}}"#,
                    challenge = serde_json_line(&challenge_id),
                    fingerprint = serde_json_line(&sha256_fingerprint),
                    username_json = serde_json_line(&username),
                    password_json = serde_json_line(&password),
                );
                stdin
                    .write_all(authenticate.as_bytes())
                    .and_then(|_| stdin.write_all(b"\n"))
                    .expect("authenticate request");
                let connect = format!(
                    r#"{{"type":"connect","protocol":"rdp","endpoint":{{"host":"{host}","port":{port}}},"username":{username_json},"password":{password_json},"domain":null,"size":{{"width":1024,"height":768}},"readOnly":false}}"#,
                    username_json = serde_json_line(&username),
                    password_json = serde_json_line(&password),
                );
                stdin
                    .write_all(connect.as_bytes())
                    .and_then(|_| stdin.write_all(b"\n"))
                    .expect("connect request");
                stdin.flush().ok();
            }
            RemoteDesktopHelperEvent::Frame { frame } => {
                frames += 1;
                _last_frame_at = Some(Instant::now());
                let update = RemoteDesktopFrameUpdate::new(
                    frame.size,
                    RemoteDesktopRect::new(0, 0, frame.size.width, frame.size.height),
                    frame.format,
                    frame.bytes,
                );
                canvas
                    .get_or_insert_with(|| Canvas::new(frame.size))
                    .apply(&update);
            }
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                updates += 1;
                _last_frame_at = Some(Instant::now());
                canvas.as_mut().expect("canvas before update").apply(&update);
            }
            RemoteDesktopHelperEvent::FrameUpdateBatch { batch } => {
                for update in batch.updates {
                    updates += 1;
                    _last_frame_at = Some(Instant::now());
                    canvas.as_mut().expect("canvas before update").apply(&update);
                }
            }
            RemoteDesktopHelperEvent::ConnectionFailure { message, .. } => {
                panic!("connection failed: {message:?}");
            }
            _ => {}
        }
    }

    let _ = helper.kill();
    let _ = helper.wait();
    watchdog.join().ok();

    assert!(frames > 0, "expected at least one full frame from the server");
    let canvas = canvas.expect("canvas after frames");
    let raw_path = format!("{dump_prefix}-frame.raw");
    let meta_path = format!("{dump_prefix}-frame.meta");
    std::fs::write(&raw_path, &canvas.bytes).expect("frame dump");
    std::fs::write(
        &meta_path,
        format!(
            "{} {}\nframes={frames} updates={updates}\n",
            canvas.size.width, canvas.size.height
        ),
    )
    .expect("meta dump");
    eprintln!(
        "[dump] {raw_path} ({}x{}, frames={frames}, updates={updates})",
        canvas.size.width, canvas.size.height
    );
}

/// Serializes a plain string as a JSON line value without pulling serde_json
/// into dev-dependencies.
fn serde_json_line(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if (ch as u32) < 0x20 => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}
