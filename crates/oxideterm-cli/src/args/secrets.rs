// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand, ValueEnum};
use serde::Deserialize;

#[derive(Debug, Args)]
#[command(
    long_about = "Inspect and manage keychain-backed connection secrets without printing secret values."
)]
#[command(
    after_help = "Examples:\n  oxideterm secrets status --json\n  oxideterm secrets set --scope connection --id prod --key password --stdin\n  oxideterm secrets clear --scope connection --id prod --key passphrase\n  oxideterm secrets import ./secrets.json"
)]
pub struct SecretsCommand {
    #[command(subcommand)]
    pub action: SecretsAction,
}

#[derive(Debug, Subcommand)]
pub enum SecretsAction {
    #[command(about = "Show secret status/hints without values")]
    Status(SecretsStatusArgs),
    #[command(about = "Set one secret from stdin or an environment variable")]
    Set(SecretsSetArgs),
    #[command(about = "Clear one secret")]
    Clear(SecretsClearArgs),
    #[command(about = "Import secrets from JSON")]
    Import(SecretsImportArgs),
}

#[derive(Debug, Args)]
pub struct SecretsStatusArgs {
    #[arg(long, value_enum, help = "Secret scope to inspect")]
    pub scope: Option<SecretScopeArg>,
    #[arg(long, help = "Connection id or name")]
    pub id: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SecretsSetArgs {
    #[arg(long, value_enum, help = "Secret scope")]
    pub scope: SecretScopeArg,
    #[arg(long, help = "Connection id or name")]
    pub id: Option<String>,
    #[arg(long, help = "Secret key within the selected scope")]
    pub key: Option<String>,
    #[arg(
        long,
        conflicts_with = "env",
        help = "Read the secret value from stdin"
    )]
    pub stdin: bool,
    #[arg(
        long = "env",
        value_name = "VAR",
        conflicts_with = "stdin",
        help = "Read the secret from environment variable VAR"
    )]
    pub env: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SecretsClearArgs {
    #[arg(long, value_enum, help = "Secret scope")]
    pub scope: SecretScopeArg,
    #[arg(long, help = "Connection id or name")]
    pub id: Option<String>,
    #[arg(long, help = "Secret key within the selected scope")]
    pub key: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SecretsImportArgs {
    #[arg(help = "Path to a secrets import JSON file")]
    pub path: String,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum SecretScopeArg {
    Connection,
}
