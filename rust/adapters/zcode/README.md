# ZCode Adapter

ZCode (Z.ai) stores usage in one SQLite database at
`${ZCODE_HOME:-~/.zcode}/cli/db/db.sqlite`. The adapter opens the database
read-only and loads only `model_usage` rows whose status is `completed`.

## Usage

```sh
ccusage zcode daily
ccusage zcode monthly
ccusage zcode session --json
```

ZCode is also included in `ccusage daily`, `ccusage monthly`, and
`ccusage session` when the database is present.

## Data path

Set `ZCODE_HOME` to one or more comma-separated ZCode home directories when
using an alternate or archived location. Each directory is expected to contain
`cli/db/db.sqlite`.

```sh
ZCODE_HOME="$HOME/.zcode,/archive/zcode" ccusage zcode daily
```

## Token and cost mapping

`model_usage.input_tokens` includes cache reads and cache creation. ccusage
reports and prices fresh input as:

```text
input_tokens - cache_read_input_tokens - cache_creation_input_tokens
```

Cache read and creation tokens remain separate usage fields; cache reads use
the cached-input rate and GLM bills no cache writes. ZCode stores no USD cost,
so `auto` and `calculate` use token pricing and `display` reports zero. This
applies to every `model_id`, including custom providers; models with no pricing
data remain in usage reports with a zero cost and a missing-pricing warning.
GLM-5.2 and GLM-5.3 have embedded pricing so `--offline` works too.

`started_at` is Unix milliseconds. ZCode already includes `reasoning_tokens`
in its output-token accounting, so ccusage does not count it a second time. The
`session.directory` column provides the project path shown in session reports.
