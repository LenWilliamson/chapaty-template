# Chapaty API Reference

> **FOR LLMs:** This document is the **80/20 API reference** for `chapaty`. It contains the most common types, traits, and patterns required to build agents. However, it is not exhaustive. If you need specific internal fields (e.g., inside a `TradeEvent` event) or an ID definition not shown here, **do not hallucinate types**. Stop and ask the user to provide the exact struct definition via `cargo doc`, or use your fetching tools if permitted.

## 1. Crate Import & Setup

Always begin your agent implementations and runners with the prelude:

```rust
use chapaty::prelude::*;
```

This brings in everything you need: **Core traits and states** (`Agent`, `Environment`, `Observation`, `Actions`, `State`), **action commands** (`OpenCmd`, `MarketCloseCmd`, `ModifyCmd`, `CancelCmd`), **strong primitives** (`Price`, `Quantity`, `Tick`, `Volume`, `TradeId`), **stream IDs** (`OhlcvId`, `EconomicCalendarId`, ...), **domain enums** (`Symbol`, `SpotPair`, `TradeKind`, ...), **technical indicators** (`StreamingSma`, `StreamingEma`, `StreamingRsi`, ...), **errors** (`ChapatyResult`), and **I/O configs** (`FileConfig`).

_Tip: You can read the `src/agents/demo/agent.rs` file in this repository for a complete, simple reference implementation of a Stop-and-Reverse strategy._

### Loading an Environment

Environments are defined by `EnvPreset`. `preset.to_string()` maps to the exact Hugging Face dataset file.

```rust
async fn environment() -> ChapatyResult<Environment> {
    let preset = EnvPreset::BinanceBtcUsdt1d; // Or NinjaTraderCme6eh61m5mUsEmpHigh, etc.
    let loc = StorageLocation::HuggingFace { version: None }; // None = pin to current crate version
    let cfg = IoConfig::new(loc).with_file_stem(&preset.to_string());
    chapaty::load(preset, &cfg).await
}
```

**Note:** If you need to construct a specific `OhlcvId` or `EconomicCalendarId` and you don't know the exact enum variants for the preset the user requested, **stop and ask the user to paste the rustdoc for that `EnvPreset` variant.**

## 2. Strong Domain Types (Newtypes)

Chapaty enforces strong typing to prevent float-related business logic errors. You MUST wrap raw `f64` or `i64` values in their respective tuple structs:

- **`Price(f64)`**: Price levels (Entries, Stops, Closes).
- **`Quantity(f64)`**: Order sizing.
- **`Tick(i64)`**: Discrete market movements.
- **`Volume(f64)`**: Aggregated volume data.
- **`TradeId(i64)`**: Unique identifier for trades.

```rust
// CORRECT
let size = Quantity(1.5);
let target = Price(50000.0);
```

## 3. CRITICAL GOTCHA: Asynchronous Data & Missing Prices

Chapaty is an event-driven engine. External events (like economic news) and price data (OHLCV) stream asynchronously. Assets may have different inception dates (e.g., BTC data might start in 2017, but SOL in 2020), or news events may trigger on weekends when exchanges are closed.

This creates two critical scenarios where price data may be temporarily missing:

**A. Crashing on Price Fetching**
When fetching a price for execution logic, NEVER use `?` or `.unwrap()` on `try_resolved_close_price` unless the strategy fundamentally cannot proceed. Doing so will crash the simulation loop.

**B. Blind Market Orders & Penalties**
If you submit a Market Order for an asset that hasn't registered its first price tick yet, the order cannot be filled and will be dropped. The environment captures this as an **invalid action** and will apply a penalty to the step's reward (configurable via `env.with_invalid_action_penalty(penalty)`).

**The Graceful Wait Pattern**
To be 100% safe, always verify that the market has a resolved price before executing calculations OR sending an order. Use a `match` statement. If the price is missing, return `Ok(Actions::no_op())` to yield execution and wait for the market to open or the asset's history to begin.

```rust
// Safely check if the market has data before acting
let current_price = match obs.market_view.try_resolved_close_price(self.symbol) {
    Ok(price) => price.0,
    Err(_) => return Ok(Actions::no_op()), // Market is closed or hasn't started streaming yet
};

// Now it is safe to calculate targets and 100% safe to send Market Orders
```

## 4. Risk Management & The `Instrument` Trait

Symbols (`SpotPair`, `FutureContract`) implement the `Instrument` trait, providing powerful helpers for calculating risk. **Use these instead of manual float math.**

- `symbol.tick_size() -> f64`
- `symbol.usd_to_price_dist(usd: f64) -> Price`: Converts a USD risk amount directly into a price distance.
- `symbol.normalize_price(price: f64) -> f64`: Snaps a raw f64 to the nearest valid tick.

```rust
// Example: Risk $50. Where should my Stop Loss be?
let risk_distance = self.symbol.usd_to_price_dist(50.0);
let sl_price = Price(entry_price.0 - risk_distance.0); // Assuming Long
```

## 5. The `Agent` Trait & State Machine

Agents are stateful structs evaluated in parallel grid searches.

- **Required Derives:** `#[derive(Debug, Clone, Copy, Serialize)]`
- **Internal State:** Must be marked `#[serde(skip)]`. Use enums for complex states, not multiple booleans.
- **Trade Identification:** You MUST maintain an internal `trade_counter: i64` to assign unique `TradeId(self.trade_counter)` to your orders. **This ID must be unique per episode.** It is standard practice to increment it before every `OpenCmd` and reset it to `0` inside the `reset()` function.

```rust
pub trait Agent {
    fn act(&mut self, obs: Observation) -> ChapatyResult<Actions>;
    fn identifier(&self) -> AgentIdentifier { /* default: UnnamedAgent */ }
    fn reset(&mut self) { /* default: no-op */ }
}
```

_Idiomatic naming:_ `AgentIdentifier::Named(Arc::new("MyAgent".to_string()))`

## 6. Observation Space

Query the world state (`market_view`) and portfolio state (`states`).

```rust
// === Temporal & Price ===
let ts = obs.market_view.current_timestamp();                                   // DateTime<Utc>
let prev_ts = obs.market_view.previous_timestamp();                             // DateTime<Utc>
let last_price = obs.market_view.try_resolved_close_price(ohlcv_id.symbol)?;    // ChapatyResult<Price>

// === Full Slice Access (StreamView trait) ===
// get_slice returns ALL events up to and including the current timestep.
// Use this to look up any historical candle by index — NO manual ring buffer needed.
let slice: Option<&[Ohlcv]> = obs.market_view.ohlcv().get_slice(&ohlcv_id);    // Option<&[Ohlcv]>
let len: usize = obs.market_view.ohlcv().len(&ohlcv_id);                       // total event count so far
let last = obs.market_view.ohlcv().last_event(&ohlcv_id);                      // Option<&Ohlcv>  (= slice.last())

// Iterate history newest-to-oldest (efficient: stop early with take_while/find)
if let Some(iter) = obs.market_view.ohlcv().rev_iter(&ohlcv_id) {
    let prev_candle = iter.nth(1); // second-to-last candle
}

// Only events newer than a known timestamp (e.g. last step's previous_ts)
if let Some(new_events) = obs.market_view.ohlcv().new_events_since(&ohlcv_id, prev_ts) {
    for candle in new_events { /* process */ }
}

// Access a specific historical candle by global index (0 = oldest, len-1 = newest):
let current_index = obs.market_view.ohlcv().len(&ohlcv_id).saturating_sub(1);
if let Some(slice) = obs.market_view.ohlcv().get_slice(&ohlcv_id) {
    let some_past_candle: Option<&Ohlcv> = slice.get(current_index.saturating_sub(3));
}

// Batch-computed indicators (pre-configured in env, no streaming needed):
let atr = obs.market_view.atr().last_event(&atr_id);                           // Option<&AtrEvent>
let roc = obs.market_view.roc().last_event(&roc_id);                           // Option<&RocEvent>
let sma = obs.market_view.sma().last_event(&sma_id);                           // Option<&SmaEvent>
let news = obs.market_view.economic_news().last_event(&cal_id);                // Option<&EconomicEvent>

// Price-check: did any stream reach `price` between the previous and current step?
let was_hit = obs.market_view.reached_price(price, symbol, TradeKind::Long);   // bool

// === Portfolio State ===
let in_trade = obs.states.any_active_trade_for_agent(&self.identifier());      // bool
// find_active_trade_for_agent returns the live trade + its market context:
if let Some((_, active_trade)) = obs.states.find_active_trade_for_agent(&self.agent_id) {
    let id: TradeId = active_trade.trade_id();
}
```

**Key Event Payloads:**

- **`Ohlcv`**: `.open`, `.high`, `.low`, `.close` (all `Price`), `.volume` (`Volume`), `.open_timestamp`, `.close_timestamp` (`DateTime<Utc>`). Helper: `.direction()`.
- **`TradeEvent`**: `.price`, `.quantity`, `.is_buyer_maker`. _(Note: This is the raw market execution, do not confuse with the agent's internal `Trade` state)._
- **`EconomicEvent`**: `.actual`, `.forecast`, `.previous`, `.economic_impact`.
- **`VolumeProfile` / `Tpo`**: `.poc`, `.value_area_high`, `.value_area_low`.

## 7. Emitting Actions

Return actions from `act()` via the `Actions` container. Pair the command wrapped in an `Action` enum with the target `MarketId`.

```rust
// 1. Do nothing
return Ok(Actions::no_op());

// 2. Open an order
self.trade_counter += 1;
let cmd = OpenCmd {
    agent_id: self.identifier(),
    trade_id: TradeId(self.trade_counter),
    trade_type: TradeKind::Long,       // or ::Short
    quantity: Quantity(1.0),
    entry_price: None,                 // None = market order; Some(Price(x)) = limit
    stop_loss: Some(Price(stop)),
    take_profit: Some(Price(target)),
};
let market_id: MarketId = ohlcv_id.into();
return Ok(Actions::from((market_id, Action::Open(cmd))));
```

Other action variants:

- `Action::Modify(ModifyCmd { ... })`: change SL/TP (and entry price for pending orders).
- `Action::MarketClose(MarketCloseCmd { quantity: None, ... })`: close position at market.
- `Action::Cancel(CancelCmd { ... })`: cancel a pending order.

## 8. Running & Evaluating

### Single Agent Evaluation

```rust
let mut env = environment().await?;
let mut agent = MyAgent::new(/* ... */);
let journal = env.evaluate_agent(&mut agent)?;

let reports_dir = Path::new("chapaty/reports");
let cfg = FileConfig::default().with_dir(reports_dir);
journal.to_file(cfg.clone())?;
journal.cumulative_returns()?.to_file(cfg.clone())?;
journal.portfolio_performance()?.to_file(cfg.clone())?;
journal.trade_stats()?.to_file(cfg)?;
```

### Grid Search Evaluation (Parameter Sweeping)

> **CRITICAL: GRID SEARCH EXECUTION**
>
> 1. **Eager Allocation:** Use `itertools::iproduct!`, filter valid parameters, assign unique IDs using `.enumerate()`, and eagerly collect the instantiated agents into a standard `Vec<(usize, Agent)>`.
> 2. **API:** Pass the `Vec` directly to `evaluate_agents`. The environment natively handles parallelization (`rayon`) and smooth progress tracking.
> 3. **Runtime Estimation:** For massive grid searches (1M+ agents), benchmark a single representative agent first using `env.evaluate_agent()` to estimate total parallel wait time.

**Grid axis boundary (must-follow):**

- **GridAxis is for float ranges; integer grids should use standard iterators.**
- Use `GridAxis` when generating decimal/float ranges (`0.1`, `0.05`, etc.).
- Use `start..end`, `start..=end`, arrays, or `Vec` for integer/categorical axes.

**Do / Don't**

```rust
// DO: float axis via GridAxis
let sl_axis = GridAxis::new("0.8", "2.1", "0.1")?; // end is exclusive
let sl_values = sl_axis.generate();

// DO: integer axis via standard iterators
let lookbacks = (14..=60).step_by(2).collect();

// DON'T: use GridAxis for integer-only ranges
// let lookback_axis = GridAxis::new("14", "61", "1")?;
```

```rust
// 1. Grid Builder implementation
pub fn build(self) -> Vec<(usize, MyAgent)> {
    let sls = self.sl_axis.generate();
    let tps = self.tp_axis.generate();

    // Eagerly collect into a Vec with unique IDs mapped via enumerate()
    iproduct!(sls, tps)
        .filter(|(sl, tp)| sl < tp) // Filter invalid logic
        .enumerate()
        .map(|(uid, (sl, tp))| (uid, MyAgent::new(sl, tp)))
        .collect::<Vec<_>>()
}

// 2. Execution in main.rs
let agents = MyAgentGrid::baseline()?.build();

// evaluate_agents handles rayon parallelization and progress bars natively
let leaderboard = env.evaluate_agents(
    agents,
    100, // top_k to retain
)?;

leaderboard.to_file_sync(&FileConfig::default())?;
```

**Canonical GridAxis API + mixed-grid example:**

```rust
use itertools::iproduct;

pub fn build(self) -> ChapatyResult<Vec<(usize, MyAgent)>> {
    // Float axis -> GridAxis
    let sl_mults = GridAxis::new("0.8", "2.1", "0.1")?.generate(); // end-exclusive

    // Integer axis -> standard iterators
    let lookback_days: Vec<i64> = vec![14, 20, 30, 45, 60];

    Ok(iproduct!(sl_mults, lookback_days)
        .enumerate()
        .map(|(uid, (sl_mult, lookback))| (uid, MyAgent::new(sl_mult, lookback)))
        .collect::<Vec<(usize, MyAgent)>>())
}
```

## 9. Canonical Gym Loop (For custom researchers)

If you need full control over the step transition rather than using `evaluate_agent()`:

```rust
let (mut obs, mut reward, mut outcome) = env.reset()?;
while !outcome.is_done() {
    let actions = obs.action_space().sample()?; // or your own policy
    (obs, reward, outcome) = env.step(actions)?;

    if outcome.is_terminal() {
        drop(obs); // Release borrow on env
        (obs, reward, outcome) = env.reset()?;
    }
}
drop(obs);
let journal = env.journal()?;
```

## 10. Indicators: Batch vs. Streaming

Chapaty has **two distinct indicator systems**. Choosing the right one matters for performance and correctness.

### 10a. Batch Indicators (preferred when applicable)

Batch indicators are **pre-computed at environment construction time** over the entire dataset. During `act()`, reading them is O(1) — no agent state required.

**When to use batch:**

- The indicator is stateless (SMA, ATR, ROC, EMA, RSI, VWAP, session ranges) — essentially anything that can be computed as a rolling window over a sorted price series.
- The parameter does NOT vary across grid-search runs (or you're willing to reload the env per grid point).

**When NOT to use batch:**

- The indicator is sequential and order-dependent (e.g. FVG, HHLL), these cannot be precomputed in bulk.
- The parameter DOES vary across grid-search runs and reloading the env is too slow. In that case, use a streaming indicator inside the agent (the env stays the same; only the agent resets between runs).

**Cost:** each configured batch indicator increases environment memory. If you add 100 SMA variants, the env stores 100 × N precomputed values. Keep grids over batch indicator parameters small or batch only the fixed parameters.

**Structural pattern — how to discover what batch indicators exist:**

The batch system follows a strict naming convention. For every data source type there is a corresponding enum in `indicator/batch/`:

| Data source query  | Batch enum             | Source file                 |
| ------------------ | ---------------------- | --------------------------- |
| `OhlcvFutureQuery` | `BatchOhlcvIndicator`  | `indicator/batch/ohlcv.rs`  |
| `OhlcvSpotQuery`   | `BatchOhlcvIndicator`  | `indicator/batch/ohlcv.rs`  |
| `TradesQuery`      | `BatchTradesIndicator` | `indicator/batch/trades.rs` |

**Before assuming a batch indicator does not exist, read the corresponding source file.** The variants inside the enum are the ground truth. Do not guess from memory.

**Configure (in `env()`)** by pushing variants onto the query's `indicators` field. The full variant set lives in the source file above — read it, don't guess:

```rust
let query = OhlcvFutureQuery {
    // ...
    indicators: vec![
        BatchOhlcvIndicator::Sma(SmaWindow(20)),
        // Atr, RateOfChange, Rsi, OvernightRange, … — see indicator/batch/ohlcv.rs
    ],
};
```

**Access (in `act()`)** by building the ID struct that mirrors the config, then querying the matching `market_view` accessor. ID and accessor names follow the indicator name (`SmaId` → `.sma()`, `AtrId` → `.atr()`):

```rust
let sma_id = SmaId { parent: m15_ohlcv_id, length: SmaWindow(20) };
if let Some(sma) = obs.market_view.sma().last_event(&sma_id) {
    let value: f64 = sma.price.0;
}
```

**Event field gotchas.** The value field is NOT always `.price`. Confirm against the struct, but these are the ones that silently return wrong numbers:

| Indicator     | ID struct        | Accessor           | Value field                        |
| ------------- | ---------------- | ------------------ | ---------------------------------- |
| SMA           | `SmaId`          | `.sma()`           | `.price: Price`                    |
| ATR           | `AtrId`          | `.atr()`           | `.range: PriceDelta` (NOT `price`) |
| ROC           | `RocId`          | `.roc()`           | `.percentage: f64`                 |
| Session range | `OhlcvSessionId` | `.ohlcv_session()` | `.high`, `.low: Price`             |

### 10b. Streaming Indicators (use when batch is not applicable)

All streaming indicators share one trait: store the indicator on the agent (`#[serde(skip)]`), call `.update(input)` once per new candle, and `.reset()` at the start of an episode. `update` returns a borrowed `Output` that is usually `Option<T>` — `None` until the indicator warms up.

> **GOTCHA — In `Agent::reset()`, call `indicator.reset()`. Never reconstruct.**
> Every streaming indicator provides `.reset()`, which clears state while preserving its configuration. Rebuilding it (`self.roc = StreamingRateOfChange::new(...)`) is an antipattern: it forces you to re-thread every config parameter by hand, and a single mismatch silently changes the indicator between episodes.
>
> ```rust
> fn reset(&mut self) {
>     self.vol_sma.reset();        // ✅ clears state, keeps SmaWindow(15)
>     self.roc.reset();            // ✅
>     // ❌ NOT: self.roc = StreamingRateOfChange::new(LookbackWindow::Time(...))?;
>     self.prev_close = None;      // plain agent state still reset by hand
> }
> ```

> **GOTCHA — Episode length is a hard floor on indicator warmup.**
> The engine resets the agent (and therefore its streaming indicators) at every episode boundary, and within one episode the agent only ever sees that episode's candles. So an indicator can **never** warm up across more time than one episode spans. If `with_episode_length(EpisodeLength::Week)` but a `LookbackWindow::Time(Duration::days(30))` ROC needs 30 days, it returns `None` for the entire 7-day episode — **forever, every episode → zero trades.** Pick an `EpisodeLength` that comfortably exceeds your longest warmup (e.g. `Annual`/`Infinite` for a 30-day trend filter; `Quarter` is the bare minimum). Note `LookbackWindow::Bars(n)` warms in `n` candles (time-cheap on intraday data), whereas `Time(d)` warms only after the buffer spans the full duration `d`.

**The builder methods and accessors are discoverable from the source — read `indicator/streaming/<name>.rs` (see `AI.md § 1a`) for the full set.** Documented below is only what a source signature does NOT reveal: calling conventions and the gotchas that caused real bugs.

#### StreamingSma

Window size is the `SmaWindow` newtype — never a bare integer. Output is `Option<f64>`.

```rust
let vol_sma = StreamingSma::new(SmaWindow(15)); // NOT StreamingSma::new(15)
```

#### StreamingHhll

`Input = IndexedOhlcv`, `Output = Option<(MarketStructureEvent, PivotPoint)>`. Two non-obvious bits: **you supply the global index yourself**, and the pivot kind comes via `pivot.trend.as_pivot_type()` — not a direct field.

```rust
let m15_index = obs.market_view.ohlcv().len(&m15_id).saturating_sub(1);
if let Some((evt, pivot)) = hhll.update(IndexedOhlcv { index: m15_index, candle: *candle }) {
    // evt: MarketStructureEvent::{BreakOfStructure | MarketStructureShift | NoChange}
    let kind: PivotType = pivot.trend.as_pivot_type(); // ::High | ::Low
}
```

#### StreamingFairValueGap

`Input = IndexedOhlcv`, `Output = &[FairValueGap<OpenState>]` (the currently-open gaps).

**The semantic source signatures will NOT tell you:** `with_price_source` controls the **fill condition only**. Gap boundaries are **always wick-based** (`top = rhs.low`, `bottom = lhs.high`) — that is the definition of an FVG.

- `PriceSource::HighLow` (default): filled when a **wick** reaches the boundary.
- `PriceSource::OpenClose`: filled only when the **candle body** reaches it (stricter; wick-only touches don't count).

**Do NOT build a manual ring buffer** to recover the candles around a gap. `creation_index()` is the global index of the right-hand candle, so index the live slice directly:

```rust
let fvg = StreamingFairValueGap::default()
    .with_price_source(PriceSource::OpenClose)
    .with_ttl_policy(TtlPolicy::Filled); // gap expires once filled (default)

let m15_index = obs.market_view.ohlcv().len(&m15_id).saturating_sub(1);
let active = fvg.update(IndexedOhlcv { index: m15_index, candle: *candle }); // &[FairValueGap<OpenState>]

// SL reference = candle before the left FVG candle (creation_index - 3):
let slice = obs.market_view.ohlcv().get_slice(&m15_id).unwrap_or(&[]);
let sl_ref = slice.get(gap.creation_index().saturating_sub(3)); // Option<&Ohlcv>
```

**Custom Indicators:** If the user requires Technical Analysis (TA) that is not available out of the box, do not be blocked. Implement it yourself as a stateful utility struct within the agent's file. If you do this, politely inform the user that they can submit a Pull Request to the core `chapaty` library, or drop a request in the `#data-requests` channel on Discord to make this indicator available to everyone.

## 11. Error Handling

All errors return `ChapatyResult<T> = Result<T, ChapatyError>`. Use `?` to propagate engine errors.
To construct a logic error in your agent, use: `AgentError::InvalidInput("...".to_string()).into()`.
