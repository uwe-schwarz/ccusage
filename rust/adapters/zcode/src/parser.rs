use std::{collections::HashSet, sync::Arc};

use jiff::tz::TimeZone as JiffTimeZone;

use crate::{
    LoadedEntry, PricingMap, TimestampMs, TokenUsageRaw, UsageEntry, UsageMessage,
    calculate_cost_for_usage, cli::CostMode, format_date_tz, format_rfc3339_millis,
    missing_pricing_model_for_candidates, total_usage_tokens,
};

pub(super) struct ZCodeEntry {
    pub(super) id: String,
    pub(super) session_id: String,
    pub(super) model: String,
    pub(super) timestamp: TimestampMs,
    pub(super) directory: Option<String>,
    pub(super) usage: TokenUsageRaw,
}

/// Reads one `model_usage` row. Column order matches the loader's SELECT.
pub(super) fn read_model_usage_row(statement: &sqlite::Statement<'_>) -> Option<ZCodeEntry> {
    let id = read_string(statement, 0)?;
    let session_id = read_string(statement, 1)?;
    let model = statement.read::<String, _>(2).ok()?.trim().to_string();
    let started_at = read_i64(statement, 3)?;
    let timestamp = (started_at > 0).then(|| TimestampMs::from_millis(started_at))?;
    if id.is_empty() || session_id.is_empty() || model.is_empty() {
        return None;
    }
    let input_tokens = read_u64(statement, 4);
    let output_tokens = read_u64(statement, 5);
    let cache_creation_input_tokens = read_u64(statement, 6);
    let cache_read_input_tokens = read_u64(statement, 7);
    // `input_tokens` folds cached prompt tokens in, so fresh input is the
    // remainder after both cache buckets are taken out.
    let fresh_input_tokens = input_tokens
        .saturating_sub(cache_read_input_tokens)
        .saturating_sub(cache_creation_input_tokens);
    if fresh_input_tokens == 0
        && output_tokens == 0
        && cache_creation_input_tokens == 0
        && cache_read_input_tokens == 0
    {
        return None;
    }
    let directory = statement
        .read::<String, _>(8)
        .ok()
        .filter(|value| !value.trim().is_empty());
    Some(ZCodeEntry {
        id,
        session_id,
        model,
        timestamp,
        directory,
        usage: TokenUsageRaw {
            input_tokens: fresh_input_tokens,
            output_tokens,
            cache_creation_input_tokens,
            cache_read_input_tokens,
            speed: None,
            cache_creation: None,
        },
    })
}

fn read_string(statement: &sqlite::Statement<'_>, index: usize) -> Option<String> {
    statement
        .read::<String, _>(index)
        .ok()
        .or_else(|| read_i64(statement, index).map(|value| value.to_string()))
        .or_else(|| {
            statement
                .read::<f64, _>(index)
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| value.to_string())
        })
}

fn read_i64(statement: &sqlite::Statement<'_>, index: usize) -> Option<i64> {
    statement.read::<i64, _>(index).ok().or_else(|| {
        statement
            .read::<f64, _>(index)
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| value.trunc() as i64)
    })
}

fn read_u64(statement: &sqlite::Statement<'_>, index: usize) -> u64 {
    read_i64(statement, index)
        .and_then(|value| u64::try_from(value.max(0)).ok())
        .unwrap_or(0)
}

pub(super) fn to_loaded_entry(
    entry: ZCodeEntry,
    tz: Option<&JiffTimeZone>,
    mode: CostMode,
    pricing: &PricingMap,
) -> LoadedEntry {
    let candidates = model_candidates(&entry.model);
    // Candidate precedence is repriced first, then pricing existence, never a
    // positive cost: an override that prices a model at exactly zero (for
    // example a subscription-backed flat-rate GLM id) must stay authoritative
    // instead of falling through to the next alias, and pricing refreshed or
    // overridden under the provider-qualified alias must win over the frozen
    // raw built-in spelling.
    let cost = candidates
        .iter()
        .find(|candidate| pricing.is_repriced(candidate))
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| pricing.find(candidate).is_some())
        })
        .map(|candidate| {
            calculate_cost_for_usage(Some(candidate), entry.usage, None, mode, Some(pricing))
        })
        .unwrap_or(0.0);
    let missing_pricing_model = (mode != CostMode::Display)
        .then(|| {
            missing_pricing_model_for_candidates(
                &entry.model,
                candidates,
                total_usage_tokens(entry.usage),
                Some(pricing),
            )
        })
        .flatten();
    let timestamp_text = format_rfc3339_millis(entry.timestamp);
    let project_path = entry.directory.unwrap_or_else(|| "ZCode".to_string());
    let data = UsageEntry {
        session_id: Some(entry.session_id.clone()),
        timestamp: timestamp_text,
        version: None,
        message: UsageMessage {
            usage: entry.usage,
            model: Some(entry.model.clone()),
            id: Some(format!("zcode:{}", entry.id)),
        },
        cost_usd: None,
        request_id: Some(entry.id),
        is_api_error_message: None,
        is_sidechain: None,
    };
    LoadedEntry {
        date: format_date_tz(entry.timestamp, tz),
        timestamp: entry.timestamp,
        project: Arc::from("zcode"),
        session_id: Arc::from(entry.session_id),
        project_path: Arc::from(project_path),
        cost,
        credits: None,
        extra_total_tokens: 0,
        message_count: None,
        model: Some(entry.model),
        usage_limit_reset_time: None,
        missing_pricing_model,
        data,
    }
}

fn model_candidates(model: &str) -> Vec<String> {
    let candidates = [
        model.to_string(),
        format!("zai/{}", model.to_ascii_lowercase()),
    ];
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(model: &str, usage: TokenUsageRaw) -> ZCodeEntry {
        ZCodeEntry {
            id: format!("usage-{model}"),
            session_id: "session-1".to_string(),
            model: model.to_string(),
            timestamp: TimestampMs::from_millis(1_735_689_600_123),
            directory: Some("/workspace/zcode".to_string()),
            usage,
        }
    }

    fn glm_usage() -> TokenUsageRaw {
        TokenUsageRaw {
            input_tokens: 700,
            output_tokens: 300,
            cache_creation_input_tokens: 100,
            cache_read_input_tokens: 200,
            speed: None,
            cache_creation: None,
        }
    }

    #[test]
    fn calculates_glm_cost_with_fresh_input_and_cached_tokens() {
        let pricing = PricingMap::load_embedded();

        let loaded = to_loaded_entry(
            entry("GLM-5.2", glm_usage()),
            None,
            CostMode::Calculate,
            &pricing,
        );

        assert_eq!(loaded.data.message.usage.input_tokens, 700);
        assert_eq!(loaded.extra_total_tokens, 0);
        // 700 * 1.4 + 300 * 4.4 + 100 * 0 + 200 * 0.28, per million tokens.
        assert!((loaded.cost - 0.002_356).abs() < 1e-9);
    }

    #[test]
    fn prices_uppercase_zcode_model_ids() {
        let pricing = PricingMap::load_embedded();

        for model in ["GLM-5.2", "GLM-5.3"] {
            let loaded = to_loaded_entry(
                entry(model, glm_usage()),
                None,
                CostMode::Calculate,
                &pricing,
            );
            assert!(loaded.cost > 0.0, "{model} should have embedded pricing");
            assert_eq!(loaded.missing_pricing_model, None);
        }
    }

    #[test]
    fn unknown_provider_models_report_zero_cost_without_failing() {
        let pricing = PricingMap::load_embedded();
        let model = "custom-zcode-provider-unknown-v1";
        assert!(
            model_candidates(model)
                .iter()
                .all(|candidate| pricing.find(candidate).is_none())
        );

        for mode in [CostMode::Auto, CostMode::Calculate] {
            let loaded = to_loaded_entry(
                entry(
                    model,
                    TokenUsageRaw {
                        input_tokens: 10,
                        output_tokens: 20,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        speed: None,
                        cache_creation: None,
                    },
                ),
                None,
                mode,
                &pricing,
            );

            assert_eq!(loaded.cost, 0.0);
            assert_eq!(loaded.missing_pricing_model.as_deref(), Some(model));
        }
    }

    #[test]
    fn zero_cost_pricing_override_stays_authoritative() {
        let zero = crate::cli::PricingOverride {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            cache_creation_input_token_cost: Some(0.0),
            cache_read_input_token_cost: Some(0.0),
            ..crate::cli::PricingOverride::default()
        };
        let model = "GLM-5.3";
        let pricing = PricingMap::load_with_overrides(
            true,
            false,
            std::iter::once((&model.to_string(), &zero)),
        );

        let loaded = to_loaded_entry(
            entry(model, glm_usage()),
            None,
            CostMode::Calculate,
            &pricing,
        );

        assert_eq!(loaded.cost, 0.0);
        assert_eq!(loaded.missing_pricing_model, None);
    }

    #[test]
    fn repriced_alias_wins_over_frozen_raw_builtin() {
        let repriced_alias = crate::cli::PricingOverride {
            input_cost_per_token: Some(2e-6),
            output_cost_per_token: Some(8e-6),
            cache_read_input_token_cost: Some(0.5e-6),
            ..crate::cli::PricingOverride::default()
        };
        let pricing = PricingMap::load_with_overrides(
            true,
            false,
            std::iter::once((&"zai/glm-5.3".to_string(), &repriced_alias)),
        );

        let loaded = to_loaded_entry(
            entry("GLM-5.3", glm_usage()),
            None,
            CostMode::Calculate,
            &pricing,
        );

        // 700 * 2 + 300 * 8 + 200 * 0.5, per million tokens - the alias
        // rates, not the built-in raw spelling's 1.4 / 4.4 / 0.26.
        assert!((loaded.cost - 0.0039).abs() < 1e-9);
    }

    #[test]
    fn display_mode_reports_zero_when_zcode_has_no_recorded_cost() {
        let pricing = PricingMap::load_embedded();

        let loaded = to_loaded_entry(
            entry("GLM-5.2", glm_usage()),
            None,
            CostMode::Display,
            &pricing,
        );

        assert_eq!(loaded.cost, 0.0);
        assert_eq!(loaded.missing_pricing_model, None);
    }
}
