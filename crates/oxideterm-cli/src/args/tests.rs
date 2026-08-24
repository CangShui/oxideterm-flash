// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::Parser;

use super::*;

#[test]
fn parses_temporary_ssh_launch() {
    let cli = Cli::parse_from(["oxideterm", "ssh", "alice@example.com", "-p", "2222"]);
    match cli.command {
        Command::Ssh(args) => {
            assert_eq!(args.target, "alice@example.com");
            assert_eq!(args.port, Some(2222));
            assert!(!args.password_stdin);
        }
        _ => panic!("expected ssh command"),
    }
}

#[test]
fn parses_connection_uri_launch_without_exposing_it_in_debug_output() {
    let cli = Cli::parse_from(["oxideterm", "open", "ssh://alice:uri-password@example.com"]);
    let rendered = format!("{cli:?}");

    assert!(matches!(cli.command, Command::Open(_)));
    assert!(!rendered.contains("uri-password"));

    let ssh_cli = Cli::parse_from([
        "oxideterm",
        "ssh",
        "ssh://alice:second-password@example.com",
    ]);
    assert!(!format!("{ssh_cli:?}").contains("second-password"));
}

#[test]
fn parses_json_only_commands() {
    let settings_sections = Cli::parse_from(["oxideterm", "settings", "sections", "--json"]);
    assert!(matches!(
        settings_sections.command,
        Command::Settings(command)
            if matches!(&command.action, SettingsAction::Sections(args) if args.json)
    ));

    let diagnostics = Cli::parse_from(["oxideterm", "diagnose", "--json"]);
    assert!(matches!(diagnostics.command, Command::Diagnose(args) if args.json));

    let backup_preview = Cli::parse_from(["oxideterm", "backup", "preview", "--json"]);
    assert!(matches!(
        backup_preview.command,
        Command::Backup(command)
            if matches!(&command.action, BackupAction::Preview(args) if args.json)
    ));
}

#[test]
fn parses_settings_export_sections() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "export",
        "--section",
        "general",
        "--include-local-terminal-env-vars",
        "--json",
    ]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Export(args) => {
                assert_eq!(args.sections, ["general"]);
                assert!(args.include_local_terminal_env_vars);
                assert!(args.json);
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_connections_export_format() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "export",
        "--format",
        "raw-safe",
        "--json",
    ]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Export(args) => {
                assert_eq!(args.format, ConnectionsExportFormat::RawSafe);
                assert!(args.json);
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_connections_create_and_edit_specs() {
    let create = Cli::parse_from([
        "oxideterm",
        "connections",
        "create",
        "--spec",
        "connection.json",
        "--dry-run",
        "--json",
    ]);
    match create.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Create(args) => {
                assert_eq!(args.spec_path.as_deref(), Some("connection.json"));
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected create command"),
        },
        _ => panic!("expected connections command"),
    }

    let edit = Cli::parse_from([
        "oxideterm",
        "connections",
        "edit",
        "prod",
        "--spec",
        "patch.json",
        "--yes",
        "--json",
    ]);
    match edit.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Edit(args) => {
                assert_eq!(args.query, "prod");
                assert_eq!(args.spec_path.as_deref(), Some("patch.json"));
                assert!(args.write.yes);
                assert!(args.write.json);
            }
            _ => panic!("expected edit command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_connections_group_rename() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "group",
        "rename",
        "old",
        "new",
        "--dry-run",
        "--json",
    ]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Group(command) => match command.action {
                ConnectionsGroupAction::Rename(args) => {
                    assert_eq!(args.old_name, "old");
                    assert_eq!(args.new_name, "new");
                    assert!(args.write.dry_run);
                    assert!(args.write.json);
                }
                _ => panic!("expected group rename command"),
            },
            _ => panic!("expected group command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_connections_apply_snapshot_strategy() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "apply-snapshot",
        "connections.json",
        "--strategy",
        "merge",
        "--dry-run",
        "--json",
    ]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::ApplySnapshot(args) => {
                assert_eq!(args.path, "connections.json");
                assert_eq!(args.strategy, ConnectionsApplyStrategy::Merge);
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected apply-snapshot command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_oxide_preview_import() {
    let cli = Cli::parse_from([
        "oxideterm",
        "oxide",
        "preview-import",
        "bundle.oxide",
        "--strategy",
        "replace",
        "--password-stdin",
        "--json",
    ]);
    match cli.command {
        Command::Oxide(command) => match command.action {
            OxideAction::PreviewImport(args) => {
                assert_eq!(args.path, "bundle.oxide");
                assert_eq!(args.strategy, OxideImportStrategy::Replace);
                assert!(args.password.password_stdin);
                assert!(args.json);
            }
            _ => panic!("expected oxide preview-import command"),
        },
        _ => panic!("expected oxide command"),
    }
}

#[test]
fn parses_oxide_import_defaults_to_dry_run_until_yes() {
    let cli = Cli::parse_from([
        "oxideterm",
        "oxide",
        "import",
        "bundle.oxide",
        "--strategy",
        "merge",
        "--password-env",
        "OXIDE_PASSWORD",
        "--section",
        "appearance",
        "--no-quick-commands",
        "--json",
    ]);
    match cli.command {
        Command::Oxide(command) => match command.action {
            OxideAction::Import(args) => {
                assert_eq!(args.strategy, OxideImportStrategy::Merge);
                assert_eq!(
                    args.password.password_env.as_deref(),
                    Some("OXIDE_PASSWORD")
                );
                assert!(!args.write.yes);
                assert_eq!(args.sections, vec!["appearance"]);
                assert!(args.no_quick_commands);
                assert!(args.write.json);
            }
            _ => panic!("expected oxide import command"),
        },
        _ => panic!("expected oxide command"),
    }
}

#[test]
fn parses_oxide_export() {
    let cli = Cli::parse_from([
        "oxideterm",
        "oxide",
        "export",
        "bundle.oxide",
        "--connection",
        "prod",
        "--password-stdin",
        "--overwrite",
        "--json",
    ]);
    match cli.command {
        Command::Oxide(command) => match command.action {
            OxideAction::Export(args) => {
                assert_eq!(args.connection_queries, ["prod"]);
                assert!(args.password.password_stdin);
                assert!(args.overwrite);
                assert!(args.json);
            }
            _ => panic!("expected oxide export command"),
        },
        _ => panic!("expected oxide command"),
    }
}

#[test]
fn parses_settings_unset_with_confirmation() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "unset",
        "ai.customSystemPrompt",
        "--yes",
        "--no-backup",
        "--json",
    ]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Unset(args) => {
                assert_eq!(args.key, "ai.customSystemPrompt");
                assert!(args.write.yes);
                assert!(args.write.no_backup);
                assert!(args.write.json);
            }
            _ => panic!("expected unset command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_settings_import_sections() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "import",
        "snapshot.json",
        "--section",
        "general",
        "--dry-run",
        "--json",
    ]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Import(args) => {
                assert_eq!(args.path, "snapshot.json");
                assert_eq!(args.sections, ["general"]);
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected import command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_strict_validation_commands() {
    let connection_validation =
        Cli::parse_from(["oxideterm", "connections", "validate", "--strict", "--json"]);
    assert!(matches!(
        connection_validation.command,
        Command::Connections(command)
            if matches!(&command.action, ConnectionsAction::Validate(args) if args.strict && args.json)
    ));

    let doctor = Cli::parse_from(["oxideterm", "doctor", "--strict", "--json"]);
    assert!(matches!(doctor.command, Command::Doctor(args) if args.strict && args.json));

    let settings_validation =
        Cli::parse_from(["oxideterm", "settings", "validate", "--strict", "--json"]);
    assert!(matches!(
        settings_validation.command,
        Command::Settings(command)
            if matches!(&command.action, SettingsAction::Validate(args) if args.strict && args.json)
    ));
}

#[test]
fn parses_backup_inspect() {
    // Full and section-scoped inspection share one backup parsing contract.
    for cli in [
        Cli::parse_from([
            "oxideterm",
            "backup",
            "inspect",
            "backup.json",
            "--full",
            "--json",
        ]),
        Cli::parse_from([
            "oxideterm",
            "backup",
            "inspect",
            "backup.json",
            "--section",
            "connections",
            "--json",
        ]),
    ] {
        match cli.command {
            Command::Backup(command) => match command.action {
                BackupAction::Inspect(args) => {
                    assert_eq!(args.query, "backup.json");
                    assert!(args.json);
                    assert_ne!(args.full, args.section.is_some());
                    if !args.full {
                        assert_eq!(args.section, Some(BackupInspectSection::Connections));
                    }
                }
                _ => panic!("expected inspect command"),
            },
            _ => panic!("expected backup command"),
        }
    }
}

#[test]
fn parses_backup_verify() {
    let cli = Cli::parse_from(["oxideterm", "backup", "verify", "backup.json", "--json"]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Verify(args) => {
                assert_eq!(args.query, "backup.json");
                assert!(args.json);
            }
            _ => panic!("expected verify command"),
        },
        _ => panic!("expected backup command"),
    }
}

#[test]
fn parses_backup_create_output() {
    let cli = Cli::parse_from([
        "oxideterm",
        "backup",
        "create",
        "--output",
        "/tmp/backup.json",
        "--json",
    ]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Create(args) => {
                assert_eq!(args.output.as_deref(), Some("/tmp/backup.json"));
                assert!(args.json);
            }
            _ => panic!("expected create command"),
        },
        _ => panic!("expected backup command"),
    }
}

#[test]
fn parses_backup_restore_defaults_to_dry_run_until_yes() {
    let cli = Cli::parse_from([
        "oxideterm",
        "backup",
        "restore",
        "backup.json",
        "--section",
        "settings",
        "--json",
    ]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Restore(args) => {
                assert_eq!(args.query, "backup.json");
                assert_eq!(args.section, Some(BackupInspectSection::Settings));
                assert!(!args.write.yes);
                assert!(args.write.json);
            }
            _ => panic!("expected restore command"),
        },
        _ => panic!("expected backup command"),
    }
}

#[test]
fn parses_connections_direct_create() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "create",
        "--name",
        "prod",
        "--host",
        "prod.example.com",
        "--user",
        "deploy",
        "--port",
        "2222",
        "--auth",
        "agent",
        "--dry-run",
    ]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Create(args) => {
                assert_eq!(args.direct.name.as_deref(), Some("prod"));
                assert_eq!(args.direct.host.as_deref(), Some("prod.example.com"));
                assert_eq!(args.direct.username.as_deref(), Some("deploy"));
                assert_eq!(args.direct.port, Some(2222));
                assert_eq!(args.direct.auth, Some(ConnectionAuthArg::Agent));
            }
            _ => panic!("expected connections create command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_forwards_quick_commands_and_secrets() {
    let forward = Cli::parse_from([
        "oxideterm",
        "forwards",
        "create",
        "--type",
        "local",
        "--bind-port",
        "8080",
        "--target-host",
        "localhost",
        "--target-port",
        "80",
    ]);
    assert!(matches!(
        forward.command,
        Command::Forwards(ForwardsCommand {
            action: ForwardsAction::Create(_)
        })
    ));

    let quick = Cli::parse_from([
        "oxideterm",
        "quick-commands",
        "create",
        "--name",
        "Uptime",
        "--command",
        "uptime",
    ]);
    assert!(matches!(
        quick.command,
        Command::QuickCommands(QuickCommandsCommand {
            action: QuickCommandsAction::Create(_)
        })
    ));

    let secret = Cli::parse_from([
        "oxideterm",
        "secrets",
        "set",
        "--scope",
        "connection",
        "--id",
        "prod",
        "--env",
        "OXIDE_CONNECTION_PASSWORD",
    ]);
    assert!(matches!(
        secret.command,
        Command::Secrets(SecretsCommand {
            action: SecretsAction::Set(_)
        })
    ));
}

#[test]
fn parses_portable_runtime_commands() {
    let status = Cli::parse_from(["oxideterm", "portable", "status", "--json"]);
    assert!(matches!(
        status.command,
        Command::Portable(PortableCommand {
            action: PortableAction::Status(PortableStatusArgs { json: true })
        })
    ));

    let setup = Cli::parse_from(["oxideterm", "portable", "setup", "--password-stdin"]);
    assert!(matches!(
        setup.command,
        Command::Portable(PortableCommand {
            action: PortableAction::Setup(PortablePasswordArgs {
                password_stdin: true,
                ..
            })
        })
    ));

    let change = Cli::parse_from([
        "oxideterm",
        "portable",
        "change-password",
        "--current-password-env",
        "OLD",
        "--new-password-env",
        "NEW",
    ]);
    assert!(matches!(
        change.command,
        Command::Portable(PortableCommand {
            action: PortableAction::ChangePassword(PortableChangePasswordArgs {
                current_password_env: Some(_),
                new_password_env: Some(_),
                ..
            })
        })
    ));
}

