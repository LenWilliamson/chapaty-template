# Prompt: Triage AI Prompts After a chapaty Release

Run this after upgrading the `chapaty` dependency. Pass it to an LLM along with the diff of the chapaty source between the old and new version. The LLM reads the diff, decides if anything in `.ai/` is now wrong or missing, and makes the minimal update.

## Your task

You are reviewing whether the AI assistant prompts in `.ai/` need updating after a new chapaty release. You will be given the source diff (or the new source tree).

**Goal:** keep `.ai/chapaty-api.md` and `AI.md` accurate enough that an LLM implementing a trading agent will not hallucinate APIs or miss important patterns. This is NOT a 1:1 documentation effort — do not try to document every type and field.

## What to check (in order of importance)

### 1. Directory structure changed?

Compare the top-level `src/` tree to the layout documented in `AI.md §Step 2`. If a directory was added, renamed, or removed under `indicator/`, `gym/`, `sim/`, or `data/`, update the tree diagram in `AI.md`.

Key directories to watch:

- `indicator/batch/` — if a new file appears (e.g. `order_book.rs`), add a row to the batch indicator table in `chapaty-api.md §10a`.
- `indicator/streaming/` — if a new file appears, no doc change needed (the LLM discovers this by listing the directory at implementation time).

### 2. Core trait signatures changed?

Check `gym/trading/agent.rs`, `data/view.rs`, and `indicator/streaming/traits.rs`
for changes to:

- `Agent::act`, `Agent::reset`, `Agent::identifier`
- `StreamView::get_slice`, `rev_iter`, `new_events_since`, `len`, `last_event`
- `StreamingIndicator::update`, `reset`

If any signature changed, update the relevant section in `chapaty-api.md` (§5, §6, §10b).

### 3. New `Batch{Type}Indicator` enum variant?

Check `indicator/batch/ohlcv.rs` and `indicator/batch/trades.rs` for new enum variants. If found: add a **one-line note** at the end of `chapaty-api.md §10a` under a `#### Recent additions` heading. Do not expand the examples — just name it. Example: `BatchOhlcvIndicator::BollingerBands(BbConfig)` added in v1.4.0.

### 4. New data source / query builder?

Check `data/query.rs` for new `*Query` structs (e.g. `OrderBookQuery`). If found: add a row to the batch indicator structural table in `chapaty-api.md §10a` pairing the new query with its batch enum (if one exists in `indicator/batch/`).

### 5. Key event struct fields changed?

Check the specific fields called out in `chapaty-api.md`:

- `Atr { range: PriceDelta }` — not `price`
- `Roc { percentage: f64, absolute_change: PriceDelta, … }`
- `PivotPoint { price, trend, indexed_candle }`
- `FairValueGap` methods: `top()`, `bottom()`, `first()`, `displacement()`, `last()`, `creation_index()`

If any of these renamed or changed type, update the relevant snippet in the doc.

## What NOT to do

- Do not document every new method on every struct.
- Do not rewrite sections whose structural pattern is still accurate.
- Do not add indicator variant lists — the pattern "read the enum in the source file" is intentionally more durable than any list you could write.
- Do not touch `spec.md` files or agent implementations — those are user-owned.

## Update threshold

| Change type                                 | Action                                    |
| ------------------------------------------- | ----------------------------------------- |
| New indicator variant in existing enum      | One-line note in `chapaty-api.md §10a`    |
| New file in `indicator/batch/`              | New row in the batch table                |
| New top-level directory in `src/`           | Update tree in `AI.md §Step 2`            |
| Core trait method added (non-breaking)      | One-line addition to the relevant section |
| Core trait method renamed / removed         | Update the affected code snippet          |
| Module renamed / moved                      | Update all path references in both files  |
| Breaking change to `Agent` or `Observation` | Full section rewrite for affected area    |

If nothing in the above categories changed, **no update is needed**. Close the task.

## How to run this

1. Get the diff: `git diff v1.X.0 v1.Y.0 -- src/` in the chapaty repo, or compare
   the two registry directories under `~/.cargo/registry/src/`.
2. Paste this file as your system prompt, then paste the relevant diff sections.
3. The LLM will output only the changed lines/sections for `chapaty-api.md` or `AI.md`.
4. Review the output — the LLM is a triage tool, not a final authority. If a change
   looks wrong, verify against the source before applying.
