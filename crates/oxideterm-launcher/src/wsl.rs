// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Minimal `wsl.exe` interop for the launcher's distribution list.
//!
//! The launcher only needs to enumerate and start distributions; it does not
//! own any graphics session state, so these helpers stay free of VNC concerns.

use crate::model::WslDistro;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// List registered distributions, or an error string when WSL is unusable.
pub fn list_distros() -> Result<Vec<WslDistro>, String> {
    list_distros_impl()
}

/// Start a distribution's default shell in a new window.
pub fn launch_distro(distro: &str) -> Result<(), String> {
    launch_distro_impl(distro)
}

#[cfg(target_os = "windows")]
fn wsl_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let mut command = Command::new("wsl.exe");
    // Avoid flashing a console window when probing WSL from the UI process.
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
fn list_distros_impl() -> Result<Vec<WslDistro>, String> {
    let output = wsl_command()
        .args(["--list", "--verbose"])
        .output()
        .map_err(|_| "WSL is not available or no distributions installed".to_string())?;
    if !output.status.success() {
        return Err("WSL is not available or no distributions installed".to_string());
    }

    let distros = parse_wsl_distro_list(&decode_wsl_output(&output.stdout));
    if distros.is_empty() {
        return Err("WSL is not available or no distributions installed".to_string());
    }
    Ok(distros)
}

#[cfg(not(target_os = "windows"))]
fn list_distros_impl() -> Result<Vec<WslDistro>, String> {
    Err("WSL is only available on Windows".to_string())
}

#[cfg(target_os = "windows")]
fn launch_distro_impl(distro: &str) -> Result<(), String> {
    use std::process::Command;

    Command::new("wsl")
        .args(["-d", distro])
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn launch_distro_impl(_distro: &str) -> Result<(), String> {
    Err("WSL is only available on Windows".to_string())
}

/// Decode `wsl.exe` output, which may be UTF-8 or UTF-16LE with or without a BOM.
pub fn decode_wsl_output(raw: &[u8]) -> String {
    if raw.len() >= 2 && raw[0] == 0xff && raw[1] == 0xfe {
        return decode_utf16le(&raw[2..]);
    }
    if raw.len() >= 4 && raw[1] == 0x00 && raw[3] == 0x00 {
        return decode_utf16le(raw);
    }
    String::from_utf8_lossy(raw).to_string()
}

fn decode_utf16le(data: &[u8]) -> String {
    let u16_iter = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
    char::decode_utf16(u16_iter)
        .filter_map(Result::ok)
        .filter(|ch| *ch != '\0')
        .collect()
}

fn parse_wsl_distro_list(stdout: &str) -> Vec<WslDistro> {
    let mut distros = Vec::new();
    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let is_default = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() >= 2 {
            distros.push(WslDistro {
                name: parts[0].to_string(),
                is_default,
                is_running: parts
                    .get(1)
                    .is_some_and(|state| state.eq_ignore_ascii_case("Running")),
            });
        }
    }
    distros
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_wsl_output_handles_supported_encodings() {
        // WSL output may arrive as UTF-8 or UTF-16LE with or without a BOM.
        let text = "NAME STATE\nUbuntu Running\n";
        let utf16 = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut utf16_with_bom = vec![0xff, 0xfe];
        utf16_with_bom.extend_from_slice(&utf16);

        for raw in [text.as_bytes(), utf16_with_bom.as_slice(), utf16.as_slice()] {
            assert_eq!(decode_wsl_output(raw), text);
        }
    }

    #[test]
    fn parse_wsl_distro_list_matches_default_and_running_fields() {
        let distros = parse_wsl_distro_list(
            "  NAME                   STATE           VERSION\n* Ubuntu                 Running         2\n  Debian                 Stopped         2\n",
        );
        assert_eq!(
            distros,
            vec![
                WslDistro {
                    name: "Ubuntu".to_string(),
                    is_default: true,
                    is_running: true,
                },
                WslDistro {
                    name: "Debian".to_string(),
                    is_default: false,
                    is_running: false,
                },
            ]
        );
    }
}
