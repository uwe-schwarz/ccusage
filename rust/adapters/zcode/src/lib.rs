use ccusage_adapter_common::{filter_loaded_entries_by_date, read_files_parallel};
use ccusage_core::*;

mod loader;
mod parser;
mod paths;
mod report;

use crate::{
    PricingMap, Result, cli::AgentCommandArgs, print_json_or_jq, print_usage_table, sort_summaries,
    wants_json,
};

pub use loader::load_entries;
pub(crate) use report::report_from_rows;
pub use report::summarize_entries;

pub fn run(args: AgentCommandArgs) -> Result<()> {
    let shared = args.shared;
    let pricing = PricingMap::load_with_overrides(
        shared.offline,
        crate::log_level() != Some(0),
        shared.pricing_overrides.iter(),
    );
    let mut entries = load_entries(&shared, &pricing)?;
    filter_loaded_entries_by_date(&mut entries, &shared);
    let mut rows = summarize_entries(&entries, args.kind)?;
    sort_summaries(&mut rows, &shared.order, |row| {
        ccusage_core::summary_period(row)
    });
    if wants_json(&shared) {
        return print_json_or_jq(
            report_from_rows(&rows, args.kind),
            shared.jq.as_deref(),
            shared.no_cost,
        );
    }
    print_usage_table(
        "ZCode Token Usage Report",
        ccusage_core::first_column(args.kind),
        &rows,
        &shared,
        false,
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod report_tests {
    use super::*;
    use crate::cli::{AgentReportKind, SharedArgs};
    use ccusage_test_support::{EnvVarGuard, fs_fixture};
    use serde_json::json;

    fn reports_from_fixture_db() -> serde_json::Value {
        let fixture = fs_fixture!({});
        let db_path = fixture.path("cli/db/db.sqlite");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let db = sqlite::open(&db_path).unwrap();
        db.execute(
            "
            CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT);
            CREATE TABLE model_usage (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                model_id TEXT,
                status TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                input_tokens INTEGER,
                output_tokens INTEGER,
                cache_creation_input_tokens INTEGER,
                cache_read_input_tokens INTEGER
            );
            ",
        )
        .unwrap();
        for (session_id, directory) in [("session-1", "/alpha"), ("session-2", "/beta")] {
            db.execute(format!(
                "INSERT INTO session (id, directory) VALUES ('{session_id}', '{directory}')"
            ))
            .unwrap();
        }
        // 2025-01-01, session-1: fresh 700 / out 300 / cache create 100 / read 200.
        db.execute("INSERT INTO model_usage VALUES ('usage-1', 'session-1', 'GLM-5.2', 'completed', 1735689600123, 1000, 300, 100, 200)").unwrap();
        // 2025-01-02, session-1: fresh 500 / out 200 / read 300.
        db.execute("INSERT INTO model_usage VALUES ('usage-2', 'session-1', 'GLM-5.2', 'completed', 1735776000123, 800, 200, 0, 300)").unwrap();
        // 2025-01-01, session-2: unpriced custom provider model.
        db.execute("INSERT INTO model_usage VALUES ('usage-3', 'session-2', 'custom-provider-model', 'completed', 1735689600123, 60, 6, 0, 0)").unwrap();
        // Non-completed rows never count.
        db.execute("INSERT INTO model_usage VALUES ('usage-4', 'session-2', 'GLM-5.2', 'cancelled', 1735689600123, 500, 100, 0, 0)").unwrap();
        drop(db);
        let _env = EnvVarGuard::set("ZCODE_HOME", fixture.root());

        let shared = SharedArgs {
            timezone: Some("UTC".to_string()),
            ..SharedArgs::default()
        };
        let pricing = PricingMap::load_embedded();
        let entries = load_entries(&shared, &pricing).unwrap();

        json!({
            "daily": report_from_rows(&summarize_entries(&entries, AgentReportKind::Daily).unwrap(), AgentReportKind::Daily),
            "monthly": report_from_rows(&summarize_entries(&entries, AgentReportKind::Monthly).unwrap(), AgentReportKind::Monthly),
            "session": report_from_rows(&summarize_entries(&entries, AgentReportKind::Session).unwrap(), AgentReportKind::Session),
        })
    }

    #[test]
    fn snapshots_zcode_reports_for_daily_monthly_and_session() {
        insta::assert_json_snapshot!(reports_from_fixture_db());
    }
}
