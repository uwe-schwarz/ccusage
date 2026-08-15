use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{
    BucketKind, LoadedEntry, Result, SessionAccumulator,
    cli::{AgentReportKind, WeekDay},
    summarize_by_key, summarize_summaries_by_bucket, totals_json,
};

pub fn report_from_rows(rows: &[crate::UsageSummary], kind: AgentReportKind) -> Value {
    let rows_json = rows
        .iter()
        .map(|row| ccusage_core::agent_summary_json(row, kind, kind == AgentReportKind::Session))
        .collect::<Vec<_>>();
    json!({
        rows_key(kind): rows_json,
        "totals": totals_json(rows),
    })
}

pub fn summarize_entries(
    entries: &[LoadedEntry],
    kind: AgentReportKind,
) -> Result<Vec<crate::UsageSummary>> {
    match kind {
        AgentReportKind::Daily => summarize_by_key(
            entries,
            |entry| entry.date.clone(),
            |date| (date.to_string(), None),
        ),
        AgentReportKind::Monthly => {
            let daily = summarize_entries(entries, AgentReportKind::Daily)?;
            Ok(summarize_summaries_by_bucket(
                &daily,
                BucketKind::Monthly,
                WeekDay::Sunday,
            ))
        }
        // Accumulating rather than grouping by key is what carries the activity
        // range and the session directory onto the row, which the session table
        // and JSON both report.
        AgentReportKind::Session => {
            let mut groups = BTreeMap::<String, SessionAccumulator>::new();
            for entry in entries {
                groups
                    .entry(entry.session_id.to_string())
                    .or_default()
                    .add_entry(entry);
            }
            groups
                .into_values()
                .map(SessionAccumulator::into_summary)
                .collect()
        }
        AgentReportKind::Weekly => {
            let daily = summarize_entries(entries, AgentReportKind::Daily)?;
            Ok(summarize_summaries_by_bucket(
                &daily,
                BucketKind::Weekly,
                WeekDay::Sunday,
            ))
        }
    }
}

fn rows_key(kind: AgentReportKind) -> &'static str {
    match kind {
        AgentReportKind::Daily => "daily",
        AgentReportKind::Weekly => "weekly",
        AgentReportKind::Monthly => "monthly",
        AgentReportKind::Session => "sessions",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{TokenUsageRaw, UsageEntry, UsageMessage};

    fn entry(session_id: &str, date: &str, millis: i64, project_path: &str) -> LoadedEntry {
        LoadedEntry {
            data: UsageEntry {
                session_id: Some(session_id.to_string()),
                timestamp: format!("{date}T00:00:00.123Z"),
                version: None,
                message: UsageMessage {
                    usage: TokenUsageRaw {
                        input_tokens: 700,
                        output_tokens: 300,
                        cache_creation_input_tokens: 100,
                        cache_read_input_tokens: 200,
                        speed: None,
                        cache_creation: None,
                    },
                    model: Some("GLM-5.2".to_string()),
                    id: Some(format!("zcode:{millis}")),
                },
                cost_usd: None,
                request_id: Some(format!("usage-{millis}")),
                is_api_error_message: None,
                is_sidechain: None,
            },
            timestamp: crate::TimestampMs::from_millis(millis),
            date: date.to_string(),
            project: Arc::from("zcode"),
            session_id: Arc::from(session_id),
            project_path: Arc::from(project_path),
            cost: 0.002_356,
            credits: None,
            extra_total_tokens: 0,
            message_count: None,
            model: Some("GLM-5.2".to_string()),
            usage_limit_reset_time: None,
            missing_pricing_model: None,
        }
    }

    #[test]
    fn aggregates_zcode_fresh_input_and_cache_tokens() {
        let rows = summarize_entries(
            &[entry(
                "session-1",
                "2025-01-01",
                1_735_689_600_123,
                "/workspace/zcode",
            )],
            AgentReportKind::Daily,
        )
        .unwrap();
        let report = report_from_rows(&rows, AgentReportKind::Daily);

        assert_eq!(report["daily"][0]["inputTokens"], 700);
        assert_eq!(report["daily"][0]["cacheCreationTokens"], 100);
        assert_eq!(report["daily"][0]["cacheReadTokens"], 200);
        assert_eq!(report["daily"][0]["totalTokens"], 1_300);
    }

    #[test]
    fn session_report_carries_project_path_and_activity() {
        let rows = summarize_entries(
            &[
                entry(
                    "session-1",
                    "2025-01-01",
                    1_735_689_600_123,
                    "/workspace/zcode",
                ),
                entry(
                    "session-1",
                    "2025-01-02",
                    1_735_776_000_123,
                    "/workspace/zcode",
                ),
            ],
            AgentReportKind::Session,
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id.as_deref(), Some("session-1"));
        assert_eq!(rows[0].project_path.as_deref(), Some("/workspace/zcode"));
        assert_eq!(
            rows[0].first_activity.as_deref(),
            Some("2025-01-01T00:00:00.123Z")
        );
        assert_eq!(
            rows[0].last_activity.as_deref(),
            Some("2025-01-02T00:00:00.123Z")
        );
    }

    #[test]
    fn session_json_includes_activity_and_project_path() {
        let rows = summarize_entries(
            &[entry(
                "session-1",
                "2025-01-01",
                1_735_689_600_123,
                "/workspace/zcode",
            )],
            AgentReportKind::Session,
        )
        .unwrap();
        let report = report_from_rows(&rows, AgentReportKind::Session);

        assert_eq!(report["sessions"][0]["sessionId"], "session-1");
        assert_eq!(report["sessions"][0]["projectPath"], "/workspace/zcode");
        assert_eq!(
            report["sessions"][0]["lastActivity"],
            "2025-01-01T00:00:00.123Z"
        );
    }
}
