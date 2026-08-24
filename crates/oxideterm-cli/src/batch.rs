// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, fs};

use serde::{Deserialize, Serialize};

use crate::{
    args::{BatchAction, BatchApplyArgs, BatchCommand, ConnectionsApplyStrategy},
    connections,
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    settings,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchPlan {
    settings: Option<BatchSettingsApply>,
    connections: Option<BatchConnectionsApply>,
}

#[derive(Debug, Deserialize)]
struct BatchSettingsApply {
    path: String,
    #[serde(default)]
    sections: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BatchConnectionsApply {
    path: String,
    #[serde(default = "default_connections_strategy")]
    strategy: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchApplyResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    steps: Vec<&'static str>,
}

pub(crate) fn run(command: BatchCommand) -> CliResult<i32> {
    match command.action {
        BatchAction::Apply(args) => apply(args),
    }
}

fn apply(args: BatchApplyArgs) -> CliResult<i32> {
    let text = fs::read_to_string(&args.path).map_err(|error| {
        CliError::new(
            "batch_read_failed",
            format!("failed to read batch plan {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let plan = serde_json::from_str::<BatchPlan>(&text)
        .map_err(|error| CliError::new("batch_parse_failed", error.to_string(), args.write.json))?;

    let mut steps = Vec::new();
    let write = args.write.clone();
    if let Some(settings_plan) = plan.settings {
        steps.push("settings");
        let sections = selected_sections(&settings_plan.sections);
        settings::apply_settings_snapshot(settings_plan.path, sections, write.clone())?;
    }
    if let Some(connections_plan) = plan.connections {
        steps.push("connections");
        connections::apply_connections_snapshot(
            connections_plan.path,
            parse_connections_strategy(&connections_plan.strategy, write.json)?,
            write.clone(),
        )?;
    }

    let response = BatchApplyResponse {
        path: args.path,
        applied: write.yes && !write.dry_run,
        dry_run: write.dry_run || !write.yes,
        steps,
    };
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format!(
                "applied: {} dryRun={} steps={}",
                response.applied,
                response.dry_run,
                response.steps.join(",")
            ));
            Ok(())
        }
    }?;
    Ok(0)
}

fn selected_sections(sections: &[String]) -> Option<HashSet<String>> {
    let selected = sections
        .iter()
        .map(|section| section.trim())
        .filter(|section| !section.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    (!selected.is_empty()).then_some(selected)
}

fn parse_connections_strategy(value: &str, json: bool) -> CliResult<ConnectionsApplyStrategy> {
    match value {
        "skip" => Ok(ConnectionsApplyStrategy::Skip),
        "replace" => Ok(ConnectionsApplyStrategy::Replace),
        "merge" => Ok(ConnectionsApplyStrategy::Merge),
        _ => Err(CliError::new(
            "batch_invalid_strategy",
            format!("unsupported connections strategy: {value}"),
            json,
        )),
    }
}

fn default_connections_strategy() -> String {
    "skip".to_string()
}
