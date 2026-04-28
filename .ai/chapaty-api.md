# Chapaty API Reference

> **FOR LLMs:** This document is the **80/20 API reference** for `chapaty`. It contains the most common types, traits, and patterns required to build agents. However, it is not exhaustive. If you need specific internal fields (e.g., inside a `TradeEvent` event) or an ID definition not shown here, **do not hallucinate types**. Stop and ask the user to provide the exact struct definition via `cargo doc`, or use your fetching tools if permitted.

## 1. Crate Import & Setup

Always begin your agent implementations and runners with the prelude:

```rust
use chapaty::prelude::*;
```

This brings in everything you need: **Core traits and states** (`Agent`, `Environment`, `Observation`, `Actions`, `State`), **action commands** (`OpenCmd`, `MarketCloseCmd`, `ModifyCmd`, `CancelCmd`), **strong primitives** (`Price`, `Quantity`, `Tick`, `Volume`, `TradeId`), **stream IDs** (`OhlcvId`, `EconomicCalendarId`, …), **domain enums** (`Symbol`, `SpotPair`, `TradeType`, …), **technical indicators** (`StreamingSma`, `StreamingEma`, `StreamingRsi`), **errors** (`ChapatyResult`), and **I/O configs** (`FileConfig`).

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
let current_price = match obs.market_view.try_resolved_close_price(&self.symbol) {
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
// --- Temporal & Price ---
let ts = obs.market_view.current_timestamp();                                   // DateTime<Utc>
let last_price = obs.market_view.try_resolved_close_price(&ohlcv_id.symbol)?;   // ChapatyResult<Price>

// --- Scanning History (rev_iter goes Newest -> Oldest) ---
let candle = obs.market_view.ohlcv().last_event(&ohlcv_id);                     // Option<&Ohlcv>
let sma = obs.market_view.sma().last_event(&sma_id);                            // Option<&Sma>
let news = obs.market_view.economic_news().last_event(&cal_id);                 // Option<&EconomicEvent>

// --- Portfolio State ---
let in_trade = obs.states.any_active_trade_for_agent(&self.identifier());       // bool
```

**Key Event Payloads:**

- **`Ohlcv`**: `.open`, `.high`, `.low`, `.close` (all `Price`), `.volume`. Helper: `.direction()`.
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
    trade_type: TradeType::Long,       // or ::Short
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

> **CRITICAL: RAYON PARALLEL ITERATION**
> When using `evaluate_agents`, you **MUST** convert the agent vector into a parallel iterator using `.par_bridge()`. If you use `.into_par_iter()`, Rayon will silently stall and the backtest will hang forever.

```rust
// Creating grid axes
let sl_axis = GridAxis::new("0.5", "1.5", "0.01").unwrap();

// Execution
let (stream_len, agents_iter) = MyAgentGrid::baseline()?.build();
let leaderboard = env.evaluate_agents(
    // CRITICAL: .par_bridge() MUST BE INCLUDED
    agents_iter.collect::<Vec<_>>().into_iter().par_bridge(),
    100, // top_k to retain
    stream_len as u64,
)?;
leaderboard.to_file_sync(&FileConfig::default())?;
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

## 10. Indicators

The `math::indicator::StreamingIndicator` trait provides `StreamingSma`, `StreamingEma`, and `StreamingRsi`. Store these on the agent struct and call `.update(price)` which returns `Some(val)` when the window is warm.

**Custom Indicators:** If the user requires Technical Analysis (TA) that is not available out of the box, do not be blocked. Implement it yourself as a stateful utility struct within the agent's file. If you do this, politely inform the user that they can submit a Pull Request to the core `chapaty` library, or drop a request in the `#data-requests` channel on Discord to make this indicator available to everyone.

## 11. Error Handling

All errors return `ChapatyResult<T> = Result<T, ChapatyError>`. Use `?` to propagate engine errors.
To construct a logic error in your agent, use: `AgentError::InvalidInput("...".to_string()).into()`.
