# Chapaty API Reference

> **FOR LLMs:** This document is the **80/20 API reference** for `chapaty`. It covers the most common types, traits, and patterns needed to build agents and environments. It is deliberately not exhaustive, because small details change between versions. Treat it as a strong starting point, not the final word. If you need a field or variant not shown here, **do not hallucinate**. Stop and read the actual source file (see `AI.md § 1a`), or ask the user to paste the rustdoc.

## 1. Crate Import & Setup

Always start agent files and runners with the prelude:

```rust
use chapaty::prelude::*;
```

This brings in the core traits and states (`Agent`, `Environment`, `Observation`, `Actions`), action commands (`OpenCmd`, `MarketCloseCmd`, `ModifyCmd`, `CancelCmd`), strong primitives (`Price`, `Quantity`, `Tick`, `Volume`, `TradeId`), queries and their stream IDs (`OhlcvSpotQuery` and `OhlcvId`, and so on), config and filter types (`EnvConfig`, `FilterConfig`, `EpisodeLength`, `DataSource`), domain enums (`Symbol`, `SpotPair`, `Period`, `DataBroker`, `Exchange`), batch indicator IDs and configs (`SmaId`, `AtrId`, `SmaWindow`, `AtrConfig`, `SessionCfg`), streaming indicators and their trait (`StreamingSma`, `StreamingIndicator`), errors (`ChapatyResult`, `ChapatyError`, `AgentError`), and I/O configs (`FileConfig`, `IoConfig`, `StorageLocation`).

_Tip: read `src/agents/demo/agent.rs` in this repository for a complete, simple reference implementation._

### Two ways to get an `Environment`

1. **From a preset** (fastest). A preset is a named, ready-made `EnvConfig`.
2. **From a hand-built `EnvConfig`** (full control). You describe exactly which data streams, indicators, filters, and episode length you want. This is covered in section 5.

```rust
// Path A: preset, loaded from a cached Hugging Face dataset
async fn environment() -> ChapatyResult<Environment> {
    let preset = EnvPreset::BinanceBtcUsdt1d;
    let loc = StorageLocation::HuggingFace { version: None }; // None pins to the current crate version
    let cfg = IoConfig::new(loc).with_file_stem(&preset.to_string());
    chapaty::load(preset, &cfg).await
}
```

- `chapaty::make(cfg)` builds an environment by fetching data from the configured `DataSource` (the hosted API or your self-hosted gRPC endpoint). Use this for any hand-built `EnvConfig`.
- `chapaty::load(cfg, &io_cfg)` first tries to read a cached dataset from storage (for example a published Hugging Face file), and falls back to `make` on a cache miss. Both accept anything that is `Into<EnvConfig>`, which includes `EnvPreset` and a raw `EnvConfig`.

## 2. Strong Domain Types (Newtypes)

Chapaty uses strong typing to prevent float-related logic errors. Wrap raw `f64` or `i64` values in their tuple structs and read the inner value with `.0`.

- **`Price(f64)`**: price levels (entries, stops, closes).
- **`Quantity(f64)`**: order sizing.
- **`Tick(i64)`**: discrete market movements.
- **`Volume(f64)`**: aggregated volume data.
- **`TradeId(i64)`**: unique identifier for trades.

```rust
let size = Quantity(1.5);
let target = Price(50000.0);
let raw: f64 = target.0; // 50000.0
```

## 3. Domain Enums Reference (real variants, do not invent)

These are the actual variants in the current library. If you need one that is not listed, read `src/data/domain.rs`.

- **`Symbol`**: `Symbol::Spot(SpotPair)` or `Symbol::Future(FutureContract)`.
- **`SpotPair`**: `BtcUsdt`, `BnbUsdt`, `EthUsdt`, `SolUsdt`, `XrpUsdt`, `TrxUsdt`, `AdaUsdt`, `XlmUsdt`.
- **`Period`**: `Second(u8)`, `Minute(u8)`, `Hour(u8)`, `Day(u8)`, `Week(u8)`, `Month(u8)`. For example `Period::Minute(15)`, `Period::Day(1)`.
- **`DataBroker`**: `Binance`, `NinjaTrader`, `InvestingCom`.
- **`Exchange`**: `Binance`, `Cme`.
- **`AggregatedPrice`** (used by VWAP and session configs): `Hlc3`, `Hl2`, `Ohlc4`, `Close`.
- **`FutureContract`**: a struct `{ root: FutureRoot, month: ContractMonth, year: ContractYear }`.
- **`FutureRoot`**: `AudUsd`, `GbpUsd`, `CadUsd`, `EurUsd`, `JpyUsd`, `NzdUsd`, `Btc`, `EminiSp500`, `EminiNasdaq100`.
- **`ContractMonth`**: `January` through `December`.
- **`ContractYear`**: `Y0` through `Y9` (for example `Y6` means 2026).

```rust
let btc = Symbol::Spot(SpotPair::BtcUsdt);
let eurusd = Symbol::Future(FutureContract {
    root: FutureRoot::EurUsd,
    month: ContractMonth::September,
    year: ContractYear::Y6,
});
```

## 4. CRITICAL GOTCHA: Asynchronous Data & Missing Prices

Chapaty is event-driven. Price data (OHLCV) and external events (like economic news) stream asynchronously. Assets have different inception dates (BTC data might start in 2017, SOL in 2020), and news can fire on weekends when an exchange is closed. So the price for a symbol can be temporarily missing.

**A. Never crash on a price fetch.** Do not use `?` or `.unwrap()` on `try_resolved_close_price` in execution logic. It will crash the whole simulation loop.

**B. Blind market orders are penalized.** If you send a market order for an asset that has not printed its first tick yet, the order cannot fill and is dropped. The environment records this as an invalid action and applies a penalty to the step reward (set with `.with_invalid_action_penalty(...)`).

**The graceful wait pattern.** Confirm the market has a resolved price before you calculate or send an order. If it is missing, return `Ok(Actions::no_op())` and wait.

```rust
let current_price = match obs.market_view.try_resolved_close_price(self.symbol) {
    Ok(price) => price.0,
    Err(_) => return Ok(Actions::no_op()), // Market closed or not streaming yet
};
// Now it is safe to calculate targets and safe to send market orders
```

## 5. Building an Environment from Scratch

A preset is just a prebuilt `EnvConfig`. You build your own the same way the presets do: start from `EnvConfig::default()`, add one query per data stream, then set filters and episode length. Pass the finished config to `chapaty::make(...)`.

### 5a. Queries and their stream IDs

Each data stream is described by a query struct. Every query implements `to_id()`, which returns the stream identifier you use later to read that stream from the observation. `to_id()` drops operational fields (like `batch_size`) and keeps only what identifies the stream. It returns a `ChapatyResult`, so use `?`.

| Query struct             | `to_id()` returns    | Read it in `act()` with        |
| ------------------------ | -------------------- | ------------------------------ |
| `OhlcvSpotQuery`         | `OhlcvId`            | `market_view.ohlcv()`          |
| `OhlcvFutureQuery`       | `OhlcvId`            | `market_view.ohlcv()`          |
| `TradesSpotQuery`        | `TradesId`           | `market_view.trades()`         |
| `TpoSpotQuery`           | `TpoId`              | `market_view.tpo()`            |
| `TpoFutureQuery`         | `TpoId`              | `market_view.tpo()`            |
| `VolumeProfileSpotQuery` | `VolumeProfileId`    | `market_view.volume_profile()` |
| `EconomicCalendarQuery`  | `EconomicCalendarId` | `market_view.economic_news()`  |

`OhlcvSpotQuery` and `OhlcvFutureQuery` fields: `broker`, `symbol`, `exchange: Option<Exchange>` (None uses the broker default), `period`, `batch_size: i32` (100 to 10000, use 1000), `indicators: Vec<BatchOhlcvIndicator>`.

`TradesSpotQuery` fields: `broker`, `symbol`, `exchange`, `batch_size`, `indicators: Vec<BatchTradesIndicator>`.

`TpoSpotQuery` / `TpoFutureQuery` / `VolumeProfileSpotQuery` fields: `broker`, `symbol`, `exchange`, `aggregation: Option<ProfileAggregation>` (None uses 1m and finest price granularity), `batch_size`.

`EconomicCalendarQuery` fields: `broker`, `data_source: Option<EconomicDataSource>`, `country_code: Option<CountryCode>` (for example `CountryCode::Us`), `category: Option<EconomicCategory>` (for example `EconomicCategory::Employment`), `importance: Option<EconomicEventImpact>` (for example `EconomicEventImpact::High`), `batch_size`. A `None` filter means "all".

Keep the query (or its `to_id()` result) on your agent so you can read the stream later:

```rust
let m15_query = OhlcvSpotQuery {
    broker: DataBroker::Binance,
    symbol: Symbol::Spot(SpotPair::BtcUsdt),
    exchange: Some(Exchange::Binance),
    period: Period::Minute(15),
    batch_size: 1000,
    indicators: vec![],
};
let m15_id: OhlcvId = m15_query.to_id()?; // use this to read candles in act()
```

### 5b. Assembling the `EnvConfig`

`DataSource::Hosted` uses Chapaty's hosted API and reads `CHAPATY_API_KEY` from the environment (put it in `.env` and load it). `DataSource::SelfHosted(endpoint)` points at your own gRPC endpoint.

```rust
async fn environment() -> ChapatyResult<Environment> {
    let source = DataSource::Hosted;

    let ohlcv_1m = OhlcvSpotQuery {
        broker: DataBroker::Binance,
        symbol: Symbol::Spot(SpotPair::BtcUsdt),
        exchange: Some(Exchange::Binance),
        period: Period::Minute(1),
        batch_size: 1000,
        indicators: vec![],
    };
    let ohlcv_15m = OhlcvSpotQuery {
        period: Period::Minute(15),
        ..ohlcv_1m.clone()
    };

    let filter = FilterConfig {
        allowed_years: Some((2017..=2026).collect()),
        ..FilterConfig::default()
    };

    let cfg = EnvConfig::default()
        .add_ohlcv_spot(source.clone(), ohlcv_1m)
        .add_ohlcv_spot(source, ohlcv_15m)
        .with_filter_config(filter)
        .with_episode_length(EpisodeLength::Infinite);

    chapaty::make(cfg).await
}
```

Builder methods (each takes the `DataSource` plus the matching query): `add_ohlcv_spot`, `add_ohlcv_future`, `add_trades_spot`, `add_tpo_spot`, `add_tpo_future`, `add_volume_profile_spot`, `add_economic_calendar`. Settings: `with_filter_config(FilterConfig)`, `with_episode_length(EpisodeLength)`, `with_trade_hint(u32)` (expected trades per episode, up to 32, for buffer sizing), `with_invalid_action_penalty(InvalidActionPenalty)` (must be `<= 0`, default `-100.0`), `with_risk_metrics_cfg(RiskMetricsConfig)`. You can add several OHLCV streams (for example 1m and 15m) so one agent sees multiple timeframes.

### 5c. Filtering data (`FilterConfig`)

`FilterConfig` has three optional filters. `None` means the filter is off (all data passes).

- `allowed_years: Option<BTreeSet<u16>>`: keep only these calendar years. Make sure they overlap with the years the data actually exists.
- `allowed_trading_hours: Option<BTreeMap<Weekday, Vec<TradingWindow>>>`: allow trading only inside these windows. A weekday missing from the map has no allowed hours. `TradingWindow::new(start, end)` is UTC and half-open `[start, end)`, hours only, no wrap over midnight. For an overnight session use two windows, for example `[22, 24)` on one day and `[0, 2)` on the next. `TradingWindow::full_day()` is `[0, 24)`.
- `economic_news_policy: Option<EconomicCalendarPolicy>`: `Unrestricted` (default), `OnlyWithEvents` (train only on days that contain economic events), or `ExcludeEvents` (drop days that contain events).

```rust
use std::collections::BTreeMap;

let filter = FilterConfig {
    allowed_years: Some([2021, 2022, 2023].into_iter().collect()),
    allowed_trading_hours: Some(BTreeMap::from([
        (Weekday::Monday, vec![TradingWindow::new(13, 20)?]), // 13:00-20:00 UTC
        (Weekday::Tuesday, vec![TradingWindow::new(13, 20)?]),
    ])),
    economic_news_policy: Some(EconomicCalendarPolicy::ExcludeEvents),
};
```

### 5d. Episode length and the warmup trap (read this)

`EpisodeLength` decides how often the engine resets the episode. On every reset, your agent's `reset()` runs, which resets every streaming indicator it holds. Variants: `Day`, `Week`, `Month`, `Quarter`, `SemiAnnual`, `Annual`, `Infinite` (default). Within one episode the agent only ever sees that episode's candles.

**The classic "my SMA strategy makes zero trades" bug.** If you run a **streaming** SMA(20) on daily candles with `EpisodeLength::Day`, the indicator resets at the end of every day. Each new day it starts cold and needs 20 daily candles to warm up, but the day ends long before that. It returns `None` forever, so you never trade. The same happens for any streaming indicator whose warmup is longer than the episode.

Two fixes:

1. Pick an `EpisodeLength` that comfortably exceeds your longest warmup. For a 20-day or 30-day streaming lookback use `Annual` or `Infinite`. `LookbackWindow::Bars(n)` warms in `n` candles; `LookbackWindow::Time(d)` warms only after the buffer spans the full duration `d`.
2. Or use the **batch** version of the indicator instead. Batch indicators are precomputed once over the entire dataset and are not reset per episode, so they never suffer this warmup problem. This is often the better choice for stateless indicators (SMA, EMA, ATR, ROC, RSI, session ranges) on short episodes. See section 12.

## 6. Risk Management & The `Instrument` Trait

`SpotPair` and `FutureContract` implement `Instrument`. Use these helpers instead of manual float math.

- `symbol.tick_size() -> f64`
- `symbol.usd_to_price_dist(usd: f64) -> Price`: converts a USD risk amount into a price distance.
- `symbol.normalize_price(price: f64) -> f64`: snaps a raw `f64` to the nearest valid tick.

```rust
let risk_distance = self.symbol.usd_to_price_dist(50.0); // risk $50
let sl_price = Price(entry_price.0 - risk_distance.0);    // long position
```

## 7. The `Agent` Trait & State Machine

Agents are stateful structs evaluated in parallel grid searches.

- **Required derives:** `#[derive(Debug, Clone, Copy, Serialize)]`.
- **Internal state:** mark it `#[serde(skip)]`. Prefer enums for complex states over several booleans.
- **Trade identification:** keep an internal `trade_counter: i64` and assign a unique `TradeId(self.trade_counter)` to each order. It must be unique per episode. Increment it before every `OpenCmd`, and reset it to `0` in `reset()`.
- **`reset()`:** reset plain state and call `.reset()` on every streaming indicator you hold. Never rebuild an indicator here (see section 12b).

```rust
pub trait Agent {
    fn act(&mut self, obs: Observation) -> ChapatyResult<Actions>;
    fn identifier(&self) -> AgentIdentifier { /* default: UnnamedAgent */ }
    fn reset(&mut self) { /* default: no-op */ }
}
```

_Idiomatic naming:_ `AgentIdentifier::Named(Arc::new("MyAgent".to_string()))`

## 8. Observation Space

An `Observation` has two halves. `obs.market_view` is the world state, which is all market data visible at the current step. `obs.states` is the portfolio state, which is every trade the engine knows about. You reference each data stream with the id you got from `query.to_id()`.

The engine never shows you data from the future. Every slice, iterator, and price lookup is already truncated at the current step, so you cannot accidentally look ahead.

### 8a. Time

```rust
let now = obs.market_view.current_timestamp();      // DateTime<Utc>
let prev = obs.market_view.previous_timestamp();    // Option<DateTime<Utc>>
```

`previous_timestamp` returns `None` on the first step of an episode, because there is no previous step yet. This is not only the first step of the whole run. The engine resets it at every episode boundary.

Handle the `None` case before you ask a stream for events since the last step. The visible history is not cleared between episodes, so treating a missing timestamp as an open lower bound would hand you the entire history as if it had just arrived.

```rust
let Some(prev) = obs.market_view.previous_timestamp() else {
    return Ok(Actions::no_op()); // First step of the episode, nothing is new yet.
};
```

The more robust pattern is to track your own `last_processed_ts` on the agent, which also gives you idempotency when `act` runs more than once for the same bar.

### 8b. Reading a data stream

Every stream accessor returns a view that implements the `StreamView` trait, so all streams are read the same way. The five methods are:

| Method                      | Returns                 | Use it for                                      |
| --------------------------- | ----------------------- | ----------------------------------------------- |
| `get_slice(&id)`            | `Option<&[Event]>`      | All events so far, indexable by position        |
| `len(&id)`                  | `usize`                 | How many events exist so far                    |
| `last_event(&id)`           | `Option<&Event>`        | The newest event, which is the common case      |
| `rev_iter(&id)`             | `Option<impl Iterator>` | Walking backwards from newest to oldest         |
| `new_events_since(&id, ts)` | `Option<impl Iterator>` | Only events newer than a timestamp you remember |

```rust
// The newest bar. This is what most strategies need.
let Some(candle) = obs.market_view.ohlcv().last_event(&self.m15_id) else {
    return Ok(Actions::no_op()); // The stream has no data yet.
};

// The whole history so far, if you need to index into it by position.
let slice: Option<&[Ohlcv]> = obs.market_view.ohlcv().get_slice(&self.m15_id);
let count: usize = obs.market_view.ohlcv().len(&self.m15_id);

// The global index of the newest bar. Streaming indicators such as
// StreamingHhll and StreamingFairValueGap need this index from you.
let index = obs.market_view.ohlcv().len(&self.m15_id).saturating_sub(1);

// Walk backwards from newest to oldest. This stops early, so it stays cheap.
if let Some(mut iter) = obs.market_view.ohlcv().rev_iter(&self.m15_id) {
    let previous_candle = iter.nth(1); // The bar before the newest one.
}

// Only the events that arrived since the last step. Note the guard on prev.
if let Some(prev) = obs.market_view.previous_timestamp()
    && let Some(new_events) = obs.market_view.ohlcv().new_events_since(&self.m15_id, prev)
{
    for candle in new_events { /* React to each new bar. */ }
}
```

`rev_iter` and `new_events_since` both run newest to oldest and stop as soon as they pass your cutoff, so prefer them over scanning the whole slice.

**All stream accessors on `market_view`:** `ohlcv()`, `trades()`, `economic_news()`, `volume_profile()`, `tpo()`, `ema()`, `sma()`, `rsi()`, `atr()`, `roc()`, `ohlcv_vwap()`, `trades_vwap()`, `ohlcv_session()`, `trades_session()`. An accessor only returns data if you configured the matching query or batch indicator. Otherwise every method returns `None` or zero.

```rust
// Batch indicators are read the same way, using the id you built by hand.
let sma = obs.market_view.sma().last_event(&sma_id);              // Option<&Sma>
let atr = obs.market_view.atr().last_event(&atr_id);              // Option<&Atr>
let session = obs.market_view.ohlcv_session().last_event(&sid);   // Option<&OhlcvSession>
let news = obs.market_view.economic_news().last_event(&cal_id);   // Option<&EconomicEvent>
```

### 8c. Price helpers on `market_view`

```rust
// The most recent close price for a symbol, resolved across every stream that
// can supply one. If several streams carry the symbol, the newest wins.
let price = obs.market_view.try_resolved_close_price(self.symbol); // ChapatyResult<Price>
// In execution logic never write `?` or `.unwrap()` on this call. See section 4.

// Did any stream touch this price between the previous step and now? This checks
// every price-capable stream, not just one, so a fast stream can register a touch
// that a slow stream would have missed. On the first step of an episode the whole
// visible history counts as the window.
let touched = obs.market_view.reached_price(price, self.symbol, TradeKind::Long); // bool

// The bar that was open at a given timestamp, meaning open <= ts < close.
let candle: Option<Ohlcv> = obs.market_view.find_candle(&self.m15_id, some_ts);

// Every market this environment trades.
let markets: Arc<[MarketId]> = obs.market_view.market_ids();
```

`try_resolved_close_price` is the safe way to ask what a symbol is worth right now. It looks at every stream that can produce a close price, keeps the one with the newest timestamp, and never reads past the current step. It returns an error when no stream has data for that symbol yet, which happens before an asset's first tick.

### 8d. Portfolio state (`obs.states`)

The engine splits trades into two groups. Live trades are pending and active ones, and they are cheap to iterate. Archived trades are closed and canceled ones, which you normally only read for reporting.

```rust
// The two calls almost every agent needs.
let busy = obs.states.any_active_trade_for_agent(&self.identifier());          // bool
if let Some((market_id, trade)) = obs.states.find_active_trade_for_agent(&self.agent_id) {
    let id = trade.trade_id();
}

// Orders that have not filled yet.
let waiting = obs.states.any_pending_trade_for_agent(&self.agent_id);         // bool
let first_pending = obs.states.find_pending_trade_for_agent(&self.agent_id);  // Option<(MarketId, &State)>

// Every live trade of this agent, pending and active. Use this when one check is
// not enough, for example to cancel the sibling of a filled bracket order.
for (market_id, trade) in obs.states.live_trades_for_agent(&self.agent_id) {
    if trade.is_pending() { /* still waiting to fill */ }
}

// Iterating everything.
for trade in obs.states.iter_live() { /* pending and active */ }
for (market_id, trade) in obs.states.iter_live_with_market() { /* same, with market */ }
for trade in obs.states.iter_archive() { /* closed and canceled */ }
for trade in obs.states.iter_all() { /* everything */ }

// Direct lookups.
let trade: Option<&State> = obs.states.get_by_id(&TradeId(7)); // live trades only
let per_market: &[State] = obs.states.get_live_trades(&market_id);

// Portfolio level.
let flat = obs.states.all_closed();                    // bool, nothing live anywhere
let pnl = obs.states.pnl();                            // f64, realized plus unrealized
let markets = obs.states.markets();                    // iterator over MarketId
```

**Important:** a limit order that has not filled yet is **pending**, not active. `any_active_trade_for_agent` and `find_active_trade_for_agent` ignore pending orders on purpose, so a strategy that only calls those will think it is flat while its orders are still working. Use the pending helpers, or `live_trades_for_agent` when you need to see both kinds at once.

`get_by_id` searches live trades only. It returns `None` once a trade has closed or been canceled.

`pnl()` is the running total of realized and unrealized profit since the episode began. It does not reset between steps.

### 8e. Reading one trade (`State`)

`States` hands you a `&State`. It covers all four lifecycle stages, so several accessors return `Option` and give you `None` when the stage does not carry that information.

```rust
// Identity.
let id: TradeId = trade.trade_id();
let agent: &AgentIdentifier = trade.agent_id();
let direction: TradeKind = trade.trade_kind();
let qty: Quantity = trade.quantity();

// Order levels.
let sl: Option<Price> = trade.stop_loss();
let tp: Option<Price> = trade.take_profit();
let entry: Price = trade.anticipated_entry_price(); // limit price while pending, fill price once active

// Lifecycle. Prefer these over guessing from other fields.
let stage: StateKind = trade.kind();   // Pending, Active, Closed, or Canceled
trade.is_pending(); trade.is_active(); trade.is_closed(); trade.is_canceled();

// Profit and loss. Active gives unrealized, Closed gives realized,
// and pending or canceled trades give None.
let pnl: Option<f64> = trade.pnl_usd();
let pnl_ticks: Option<Tick> = trade.pnl_ticks(symbol);

// Planned risk from the stop loss and take profit. These need the symbol,
// because ticks and USD depend on the contract. Use them instead of writing
// your own risk math.
let risk_ticks: Option<Tick> = trade.expected_loss_in_ticks(symbol);
let reward_ticks: Option<Tick> = trade.expected_profit_in_ticks(symbol);
let risk_usd: Option<f64> = trade.expected_loss_in_usd(symbol);
let reward_usd: Option<f64> = trade.expected_profit_in_usd(symbol);
let rrr: Option<RiskRewardRatio> = trade.risk_reward_ratio(symbol); // read it with .ratio()

// Timing and outcome.
let entered: Option<DateTime<Utc>> = trade.entry_ts();       // active and closed only
let exited: Option<DateTime<Utc>> = trade.exit_ts();         // closed and canceled only
let exit_px: Option<Price> = trade.exit_price();             // closed only
let why: Option<TerminationReason> = trade.exit_reason();    // StopLoss, TakeProfit, MarketClose, Canceled
```

Use `exit_reason()` when you want to react to how the last trade ended, for example to stop trading for the session after a stop loss.

### Key event payloads

- **`Ohlcv`**: `.open`, `.high`, `.low`, `.close` (all `Price`), `.volume` (`Volume`), `.open_timestamp`, `.close_timestamp` (`DateTime<Utc>`). Helper `.direction()` returns `CandleDirection::{Bullish, Bearish, Doji}`.
- **`TradeEvent`**: `.price` (`Price`), `.quantity` (`Quantity`), `.is_buyer_maker` (`Option<LiquiditySide>`). This is a raw market execution, not your own trade.
- **`EconomicEvent`**: `.timestamp`, `.actual`, `.forecast`, `.previous` (all `Option<EconomicValue>`), `.economic_impact` (`EconomicEventImpact`).
- **`VolumeProfile` and `Tpo`**: `.poc`, `.value_area_high`, `.value_area_low` (all `Price`).
- **`OhlcvSession`** (from the batch overnight range): `.session` (`SessionDate`), `.open_timestamp`, `.close_timestamp`, `.high`, `.low`, `.highest_close`, `.lowest_close`, `.volume`, `.vwap`.

## 9. Emitting Actions

Return actions from `act()` in the `Actions` container. Pair a command, wrapped in `Action`, with the target `MarketId`. Get the `MarketId` from an `OhlcvId` with `.into()`.

```rust
// 1. Do nothing
return Ok(Actions::no_op());

// 2. Open an order
self.trade_counter += 1;
let cmd = OpenCmd {
    agent_id: self.identifier(),
    trade_id: TradeId(self.trade_counter),
    trade_kind: TradeKind::Long,       // or TradeKind::Short
    quantity: Quantity(1.0),
    entry_price: None,                 // None = market order; Some(Price(x)) = limit
    stop_loss: Some(Price(stop)),
    take_profit: Some(Price(target)),
};
let market_id: MarketId = self.m15_id.into();
return Ok(Actions::from((market_id, Action::Open(cmd))));
```

Other variants: `Action::Modify(ModifyCmd { new_entry_price, new_stop_loss, new_take_profit, ... })`, `Action::MarketClose(MarketCloseCmd { quantity: None, ... })` (`None` closes the full size), `Action::Cancel(CancelCmd { ... })`.

## 10. Running & Evaluating

### Single Agent Evaluation

Reports implement `to_file_sync(&FileConfig)` (there is no `to_file`; the config is taken by reference).

```rust
let mut env = environment().await?;
let mut agent = MyAgent::new(/* ... */);
let journal = env.evaluate_agent(&mut agent)?;

let cfg = FileConfig::default().with_dir(Path::new("chapaty/reports"));
journal.to_file_sync(&cfg)?;
journal.cumulative_returns()?.to_file_sync(&cfg)?;
journal.portfolio_performance()?.to_file_sync(&cfg)?;
journal.trade_stats()?.to_file_sync(&cfg)?;
```

### Grid Search Evaluation (Parameter Sweeping)

> **CRITICAL: GRID SEARCH EXECUTION**
>
> 1. **Eager allocation:** use `itertools::iproduct!`, filter valid parameters, assign unique IDs with `.enumerate()`, and eagerly collect into a `Vec<(usize, Agent)>`.
> 2. **API:** pass the `Vec` to `evaluate_agents`. The environment handles parallelization (`rayon`) and progress tracking.
> 3. **Runtime estimation:** for very large grids (1M or more), benchmark one agent with `env.evaluate_agent()` first.

**Grid axis boundary (must follow):**

- `GridAxis::new(start, end, step)` takes string arguments, returns `ChapatyResult`, and `.generate()` returns `Vec<f64>` with the end exclusive. Use it only for float ranges.
- Use standard iterators (`start..=end`, arrays, `Vec`) for integer and categorical axes.

```rust
// DO: float axis via GridAxis
let sl_values = GridAxis::new("0.8", "2.1", "0.1")?.generate(); // end exclusive
// DO: integer axis via iterators
let lookbacks: Vec<usize> = (14..=60).step_by(2).collect();
// DON'T: GridAxis for integer-only ranges
```

```rust
use itertools::iproduct;

pub fn build(self) -> ChapatyResult<Vec<(usize, MyAgent)>> {
    let sl_mults = GridAxis::new("0.8", "2.1", "0.1")?.generate(); // floats
    let lookbacks: Vec<i64> = vec![14, 20, 30, 45, 60];            // ints

    Ok(iproduct!(sl_mults, lookbacks)
        .filter(|(sl, _)| *sl > 0.0)
        .enumerate()
        .map(|(uid, (sl, lb))| (uid, MyAgent::new(sl, lb)))
        .collect())
}

// In main.rs:
let agents = MyAgentGrid::baseline()?.build()?;
let leaderboard = env.evaluate_agents(agents, 100)?; // keep top 100
leaderboard.to_file_sync(&FileConfig::default())?;
```

## 11. Canonical Gym Loop (for custom researchers)

```rust
let (mut obs, mut reward, mut outcome) = env.reset()?;
while !outcome.is_done() {
    let actions = obs.action_space().sample()?; // or your own policy
    (obs, reward, outcome) = env.step(actions)?;
    if outcome.is_terminal() {
        drop(obs); // release the borrow on env
        (obs, reward, outcome) = env.reset()?;
    }
}
drop(obs);
let journal = env.journal()?;
```

## 12. Indicators: Batch vs. Streaming

Chapaty has two indicator systems. Choosing the right one matters for performance and correctness.

### 12a. Batch Indicators (preferred when applicable)

Batch indicators are precomputed at environment build time over the whole dataset. In `act()` reading them is O(1) and needs no agent state, and they are not reset per episode (so no warmup trap).

**Add them to a query** by pushing onto its `indicators` field, or with `.with_indicator(...)` / `.with_indicators(...)` from the `WithBatchIndicators` trait.

`BatchOhlcvIndicator` variants and their config types: `Ema(EmaWindow)`, `Sma(SmaWindow)`, `Rsi(RsiWindow)`, `Atr(AtrConfig)`, `RateOfChange(LookbackWindow)`, `Vwap(AggregatedPrice)`, `OvernightRange(SessionCfg)`.
`BatchTradesIndicator` variants: `Vwap(AggregatedPrice)`, `OvernightRange(SessionCfg)`.

These variants are the ground truth. Before assuming an indicator does not exist, read `indicator/batch/ohlcv.rs` or `indicator/batch/trades.rs`.

```rust
let query = OhlcvSpotQuery {
    broker: DataBroker::Binance,
    symbol: Symbol::Spot(SpotPair::BtcUsdt),
    exchange: Some(Exchange::Binance),
    period: Period::Day(1),
    batch_size: 1000,
    indicators: vec![
        BatchOhlcvIndicator::Sma(SmaWindow(20)),
        BatchOhlcvIndicator::Sma(SmaWindow(50)),
    ],
};
```

**Read a batch indicator in `act()`** by building its ID struct yourself. There is no `to_id()` for batch indicators. Every batch ID has a `parent: OhlcvId` (the OHLCV stream it is computed from, which is `query.to_id()?`), plus the same config you added. The field names are not uniform:

| Indicator     | ID struct        | Fields                               | Accessor           | Event type     | Value field                        |
| ------------- | ---------------- | ------------------------------------ | ------------------ | -------------- | ---------------------------------- |
| SMA           | `SmaId`          | `parent`, `length: SmaWindow`        | `.sma()`           | `Sma`          | `.price: Price`                    |
| EMA           | `EmaId`          | `parent`, `length: EmaWindow`        | `.ema()`           | `Ema`          | `.price: Price`                    |
| RSI           | `RsiId`          | `parent`, `length: RsiWindow`        | `.rsi()`           | `Rsi`          | `.price: Price`                    |
| ATR           | `AtrId`          | `parent`, `cfg: AtrConfig`           | `.atr()`           | `Atr`          | `.range: PriceDelta` (NOT `price`) |
| ROC           | `RocId`          | `parent`, `lookback: LookbackWindow` | `.roc()`           | `Roc`          | `.percentage: f64`                 |
| Session range | `OhlcvSessionId` | `parent`, `cfg: SessionCfg`          | `.ohlcv_session()` | `OhlcvSession` | `.high`, `.low: Price`             |

Note that the event structs have no `Event` suffix (`Sma`, `Atr`, `Roc`, `OhlcvSession`), and the value field is not always `.price`.

```rust
let ohlcv_id: OhlcvId = query.to_id()?;
let sma_id = SmaId { parent: ohlcv_id, length: SmaWindow(20) };
if let Some(sma) = obs.market_view.sma().last_event(&sma_id) {
    let value: f64 = sma.price.0;
}
```

**Session ranges: batch vs streaming.** A `SessionCfg` is `{ window: SessionWindow, price_aggregation: AggregatedPrice }`. `SessionWindow` has ready-made presets (`us_core_session()`, `us_overnight()`, `us_extended_overnight()`, `london_core_session()`, and more) or `SessionWindow::new(timezone, start, end)`.

```rust
let session = SessionCfg {
    window: SessionWindow::us_overnight(),
    price_aggregation: AggregatedPrice::Hlc3,
};
// Batch: configured on the query, precomputed for the whole dataset, read via .ohlcv_session()
let indicators = vec![BatchOhlcvIndicator::OvernightRange(session)];
let session_id = OhlcvSessionId { parent: ohlcv_id, cfg: session };
if let Some(s) = obs.market_view.ohlcv_session().last_event(&session_id) {
    let (hi, lo) = (s.high.0, s.low.0);
}
```

Use the **batch** `OvernightRange` when the session config is fixed for the run. Use the **streaming** `StreamingOvernightRange` (stored on the agent, updated each candle) when the session parameters vary per grid agent, since changing a batch parameter means rebuilding the environment.

### 12b. Streaming Indicators (use when batch is not applicable)

All streaming indicators implement one trait. Store the indicator on the agent (`#[serde(skip)]`), call `.update(input)` once per new candle, and `.reset()` at the start of an episode. `update` returns a borrowed `Output`, usually `Option<T>`, which is `None` until the indicator warms up.

```rust
pub trait StreamingIndicator: Debug + Send + Sync {
    type Input;
    type Output<'a> where Self: 'a;
    fn update(&mut self, input: Self::Input) -> Self::Output<'_>;
    fn reset(&mut self);
}
```

> **GOTCHA: in `Agent::reset()`, call `indicator.reset()`. Never reconstruct.**
> `.reset()` clears state and keeps configuration. Rebuilding (`self.roc = StreamingRateOfChange::new(...)`) forces you to re-thread every config value by hand, and one mismatch silently changes the indicator between episodes.
>
> ```rust
> fn reset(&mut self) {
>     self.vol_sma.reset();   // keeps SmaWindow(15)
>     self.roc.reset();
>     self.prev_close = None; // plain state reset by hand
> }
> ```

> **GOTCHA: episode length is a hard floor on warmup.** See section 5d. A streaming indicator can never warm up across more time than one episode spans. If the warmup exceeds the episode you get zero trades, every episode, forever. Use a longer `EpisodeLength` or the batch version.

Read the source of each indicator (`indicator/streaming/<name>.rs`) for exact builder methods and accessors. Documented below is only what a signature does not reveal.

#### StreamingSma

Window size is `SmaWindow`, never a bare integer. `Output = Option<f64>`.

```rust
let vol_sma = StreamingSma::new(SmaWindow(15)); // NOT StreamingSma::new(15)
```

#### StreamingHhll

`Input = IndexedOhlcv`, `Output = Option<(MarketStructureEvent, PivotPoint)>`. You supply the global index yourself, and the pivot kind comes from `pivot.trend.as_pivot_type()`.

```rust
let idx = obs.market_view.ohlcv().len(&self.m15_id).saturating_sub(1);
if let Some((evt, pivot)) = hhll.update(IndexedOhlcv { index: idx, candle: *candle }) {
    // evt: MarketStructureEvent::{BreakOfStructure | MarketStructureShift | NoChange}
    let kind: PivotType = pivot.trend.as_pivot_type(); // PivotType::High | PivotType::Low
}
```

#### StreamingFairValueGap

`Input = IndexedOhlcv`, `Output = &[FairValueGap<OpenState>]` (currently open gaps). `with_price_source` controls the fill condition only. Gap boundaries are always wick-based (`top = rhs.low`, `bottom = lhs.high`).

- `PriceSource::HighLow` (default): filled when a wick reaches the boundary.
- `PriceSource::OpenClose`: filled only when the candle body reaches it.

Do not build a manual ring buffer to recover candles around a gap. `creation_index()` is the global index of the right-hand candle, so index the live slice directly.

```rust
let fvg = StreamingFairValueGap::default()
    .with_price_source(PriceSource::OpenClose)
    .with_ttl_policy(TtlPolicy::Filled);

let idx = obs.market_view.ohlcv().len(&self.m15_id).saturating_sub(1);
let active = fvg.update(IndexedOhlcv { index: idx, candle: *candle });

let slice = obs.market_view.ohlcv().get_slice(&self.m15_id).unwrap_or(&[]);
let sl_ref = slice.get(gap.creation_index().saturating_sub(3)); // Option<&Ohlcv>
```

### 12c. Writing a custom indicator

If the technical analysis you need is not in the library, do not be blocked. Implement `StreamingIndicator` on your own struct, store it on the agent, and reset it in `Agent::reset()`. This is the same trait every built-in streaming indicator uses.

```rust
#[derive(Debug, Default)]
struct RollingMax {
    window: usize,
    buf: std::collections::VecDeque<f64>,
}

impl StreamingIndicator for RollingMax {
    type Input = f64;
    type Output<'a> = Option<f64>;

    fn update(&mut self, x: f64) -> Option<f64> {
        self.buf.push_back(x);
        if self.buf.len() > self.window {
            self.buf.pop_front();
        }
        if self.buf.len() < self.window {
            return None; // still warming up
        }
        self.buf.iter().copied().fold(None, |m, v| Some(m.map_or(v, |m: f64| m.max(v))))
    }

    fn reset(&mut self) {
        self.buf.clear();
    }
}
```

When you add a custom indicator, let the user know they can submit a pull request to the core `chapaty` library, or request it in the `#data-requests` channel on Discord, so everyone gets it.

## 13. Error Handling

Everything returns `ChapatyResult<T> = Result<T, ChapatyError>`. Use `?` to propagate engine errors (except the one call in section 4 you must not blindly `?`). To raise a logic error from your agent:

```rust
return Err(AgentError::InvalidInput("reason here".to_string()).into());
```
