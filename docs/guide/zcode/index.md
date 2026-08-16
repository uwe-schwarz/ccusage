# ZCode (Z.ai) Data Source

ccusage can read local ZCode usage data as a supported data source. ZCode uses the same unified and focused report model as other agents.

## Focused Views

```bash
# Daily ZCode usage
ccusage zcode daily

# Monthly ZCode usage
ccusage zcode monthly

# ZCode sessions
ccusage zcode session
```

Most users can start with unified reports such as `ccusage daily`. Add the `zcode` namespace only when you want to focus the same report shape on ZCode usage.

## Data Source

ZCode (Z.ai's coding CLI) stores usage in one SQLite database. ccusage opens it read-only; no ZCode data is changed.

Root resolution (highest first):

1. A non-empty `ZCODE_HOME` (comma-separated homes are supported)
2. `~/.zcode`

```text
~/.zcode/                 # or $ZCODE_HOME
└── cli/
    └── db/
        └── db.sqlite     # PRIMARY (model_usage + session tables)
```

Only `model_usage` rows whose `status` is `completed` are counted. `running`, `error`, and `cancelled` rows are excluded.

## Report Views

| Focused view            | Description                 | See also                                |
| ----------------------- | --------------------------- | --------------------------------------- |
| `ccusage zcode daily`   | Aggregate usage by date     | [Daily Usage](/guide/daily-reports)     |
| `ccusage zcode monthly` | Aggregate usage by month    | [Monthly Usage](/guide/monthly-reports) |
| `ccusage zcode session` | Group usage by ZCode session | [Session Usage](/guide/session-reports) |

These views support `--json`, `--compact`, `--mode`, and `--offline`.

## What Gets Calculated

- **Token usage** - ZCode records OpenAI-style usage where `input_tokens` includes cached prompt tokens. ccusage reports fresh input as `input_tokens - cache_read_input_tokens - cache_creation_input_tokens` and keeps cache reads and cache creation as their own fields.
- **Reasoning tokens** - `reasoning_tokens` are already included in `output_tokens`, so they are not counted a second time.
- **Timestamps** - `started_at` is Unix milliseconds. The `session.directory` column provides the project path shown in session reports.
- **Precomputed cost** - ZCode never stores USD costs, so `display` reports zero. `auto` and `calculate` estimate from the pricing tables.
- **Pricing** - GLM-5.2 and GLM-5.3 have embedded pricing, so `--offline` works. Custom-provider `model_id` values (for example models routed through other Anthropic-compatible endpoints) stay in the reports with zero cost and a missing-pricing warning until pricing data covers them.

## Environment Variables

| Variable     | Description                                        |
| ------------ | -------------------------------------------------- |
| `ZCODE_HOME` | ZCode home directory (default `~/.zcode`)          |
| `LOG_LEVEL`  | Adjust verbosity (0 silent ... 5 trace)            |

## Configuration

```json
{
	"zcode": {
		"defaults": {
			"offline": true
		},
		"commands": {
			"session": {
				"json": true
			}
		}
	}
}
```

The `zcode` namespace supports the same shared report options as other focused sources. Use `zcode.defaults` for all ZCode reports and a matching `zcode.commands.daily`, `zcode.commands.monthly`, or `zcode.commands.session` object for report-specific overrides. The database is discovered from `ZCODE_HOME` or `~/.zcode/cli/db/db.sqlite`, not from ccusage configuration.

## Troubleshooting

::: details No ZCode usage data found
Ensure `~/.zcode/cli/db/db.sqlite` exists. Set `ZCODE_HOME` to the ZCode home directory (the one containing `cli/db/db.sqlite`), not to the SQLite file itself. Set it to a comma-separated list to include archived homes.
:::

::: details Costs showing as $0.00
ZCode stores no costs, so `display` mode always reports zero. Use the default `auto` or `--mode calculate` to estimate from pricing tables. Models without pricing data keep zero cost and a missing-pricing warning appears.
:::
