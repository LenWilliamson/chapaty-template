# Agent Plan: The Spec-First Protocol

> **CRITICAL PROTOCOL FOR ALL LLMs: Follow this exact workflow. Do not skip steps.**

This repository uses a strict **Spec-First** workflow. Skipping to writing Rust code before a specification is finalized is the single most common way strategies ship with silent bugs. **Do not do it.**

## User Fallback (If the user is lost)

If the user pastes a trading strategy directly into the chat but has not created a `spec.md` file yet, **do not start writing code.** Politely offer to set up the directory structure for them, or instruct them to do it:

1. Create the directory: `mkdir -p src/agents/<strategy_name>`
2. Create the spec: `touch src/agents/<strategy_name>/spec.md`
3. Paste their idea into the `spec.md` file.

If the user is completely stuck and doesn't know what to build, guide them to `.ai/algorithm-ideas.md` to pick a seed strategy.

## Phase 1: Ingestion & Verification

1. Read `src/agents/<name>/spec.md` exactly as the user wrote it.
2. Read `.ai/chapaty-api.md` to understand the 80/20 core building blocks of the `chapaty` engine.
3. Read `.ai/rust-vibe-rules.md` to understand the Rust style required in this repo.
4. Identify the asset, timeframe, and required data. Propose an `EnvPreset` (e.g., `BinanceBtcUsdt1d`).
   - **Data-Agnostic Fallback:** Chapaty's logic is data-agnostic. If a user wants to trade an unsupported asset (e.g., a specific stock), tell them to request the data in Discord, but **proceed immediately** using a placeholder preset (like BTC-USDT). The trading logic remains identical; they will only need to swap the `MarketId` once their data is available.

## Phase 2: Clarify & Parametrize

Ask the user about every ambiguity. Start slowly with simple and reasonable defaults to avoid premature optimization. You must have explicit answers for:

- **Entry condition(s)**: An exact, testable predicate over `Observation`.
- **Exit condition(s)**: Stop-loss, take-profit, time-based, or a mix.
- **Position sizing**: Fixed quantity or risk-based?
- **Trade type**: Always long, always short, or directional by signal?

**Crucial: Parameterization for Grid Search**
The `chapaty` engine is built for evaluating agents in parallel.

- Every "magic number" (e.g., SL/TP percentages, RSI thresholds, wait durations) must be a field on the Agent struct, allowing the generation of a parametrized grid for parallel backtesting.
- The Agent struct must derive `Clone, Serialize, Debug`.

## Phase 3: Rewrite Spec & Halt

Rewrite `spec.md` as a formal specification with these sections (in exactly this order):

1. **Summary**: One paragraph overview.
2. **Environment**: The chosen `EnvPreset` and why.
3. **Observation Inputs**: Which fields of `obs.market_view` (e.g., `ohlcv().rev_iter()`) and `obs.states` (e.g., `iter_live()`) you will read.
4. **Entry Logic**: Plain English / pseudocode (no Rust yet).
5. **Exit Logic**: Plain English / pseudocode (no Rust yet).
6. **Parameters**: List every configurable field, its default value, and a sensible grid range for future parallel sweeps.
7. **Assumptions / Out of Scope**: Anything you had to guess at or explicitly ignored.

**STOP HERE.**
End your response with:

> _Please review `src/agents/<name>/spec.md`. Reply "approved" to continue, or describe the changes you want._

**Do not write any Rust until the user explicitly approves.**

## Phase 4: Implementation (The Modern Module Convention)

Once approved, build the strategy using the modern Rust (non-`mod.rs`) directory convention.

1. **Create the Implementation:** Write the agent logic in `src/agents/<name>/agent.rs`.
2. **Create the Module Declaration:** Create `src/agents/<name>.rs` containing:
   ```rust
   pub mod agent;
   pub use agent::*;
   ```
3. **Register the Module:** Append `pub mod <name>;` to `src/agents.rs` (create the file if missing).
4. **Wire it into `src/main.rs`:**

- **Single Agent Evaluation (REQUIRED):** ALWAYS run `env.evaluate_agent()` on a baseline agent first, and export its reports. This guarantees the Python visualization script succeeds.
- **Grid Search Execution (Optional):** Include a grid search block. Build the grid by eagerly collecting agents into a `Vec<(usize, Agent)>` (assigning unique IDs via `.enumerate()`) and pass the vector directly to `env.evaluate_agents()`, which natively handles `rayon` parallelization and progress tracking.
- **Runtime Estimation (CRITICAL for Large Grids):** Before launching massive grid searches (e.g., 1M+ agents), use the single baseline agent to benchmark the execution time and estimate your total parallel wait time using `(Single Time * Total Agents) / CPU Cores`.

## Phase 5: Handoff

Summarize the completion for the user in plain English:

- The parameters shipped and their defaults.
- A 2-sentence recap of the entry/exit logic.
- Tell them to run `make run` to backtest and generate the HTML tearsheet.

_Note for LLM: If `make run` throws a Python error because `journal.csv` is missing, you failed the instruction in Phase 4. Ensure `main.rs` always evaluates a single agent to produce the journal._

## Hard Engine Rules & Constraints

1. **Never invent `chapaty` types.** If `.ai/chapaty-api.md` doesn't cover what you need, ask the user to provide the Rust documentation for the required module.
2. **Observation Space Rules:**
   - To scan price history, use `obs.market_view.ohlcv().rev_iter(id)` (searches newest to oldest).
   - To check agent positions, iterate the hot path via `obs.states.iter_live()` or `obs.states.any_active_trade_for_agent()`.
3. **Never add `<'a>` lifetimes** to user-facing strategy code. Use `.clone()` or `Copy` types.
4. **Error Handling:** All strategy functions must return `ChapatyResult<T>`.
5. **Event Loop & Missing Price Data:** In financial simulations with multiple streams, data starts at different times. Additionally, news events can arrive on weekends when markets are closed. Therefore, `obs.market_view.try_resolved_close_price(&symbol)` may return an error if data hasn't streamed in yet.
   **Do NOT use `?` or `.unwrap()` on price lookups inside the `act` loop.** Doing so will crash the simulation. Handle it gracefully:
   ```rust
   let current_price = match obs.market_view.try_resolved_close_price(&self.symbol) {
       Ok(price) => price.0,
       Err(_) => return Ok(Actions::no_op()), // Wait for the next tick
   };
   ```
   Do not blindly fire market orders without checking if price data exists; otherwise, it will be rejected as an invalid action.
6. **Python Visualization is Off-Limits:** Do not modify `visualization/generate_tearsheet.py`. It defensively handles `groupby("date")` logic for EOD downsampling. Leave it exactly as is.
