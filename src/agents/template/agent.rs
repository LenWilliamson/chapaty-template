//! # Template Agent
//!
//! This module is a starting point for building a trading strategy with
//! chapaty. Every struct, name, and helper here is an example. Keep what fits
//! your strategy, remove what you do not need, and add new building blocks such
//! as custom indicators, enums, or helper types whenever the strategy calls for
//! them.
//!
//! ## What to serialize
//!
//! Serialize only the grid search parameters, which are the values that
//! configure the agent's behaviour and that you sweep over during a grid
//! search. These are the fields that show up in the leaderboard. Mark
//! everything else with `#[serde(skip)]`: stream ids, streaming indicators,
//! trading state, counters, and idempotency timestamps.
//!
//! ## Streaming indicators
//!
//! Store the indicator's configuration as a serialized parameter, not the
//! indicator itself. For example, store the SMA period as a `u16` grid
//! parameter and keep the `StreamingSma` in a `#[serde(skip)]` field.
//! Serializing the indicator would write a large amount of internal state that
//! you do not need. Rebuild the indicator from its parameter inside the
//! matching `with_*` method, and reset it in `reset()`.
//!
//! ## Idempotency
//!
//! `act()` can be called several times for the same bar. Keep the close
//! timestamp of the last bar you processed and only advance your indicators and
//! state when a new bar arrives. Use one timestamp per data stream, so a second
//! OHLCV stream would get its own `last_processed_ts_2` field.
//!
//! ## The state machine is optional
//!
//! Many agents are naturally a state machine over time, so this template ships
//! with an `AgentState` enum as an example. If your strategy is simpler, delete
//! the enum and act directly on the observation.

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result};
use chapaty::prelude::*;
use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

/// A template trading agent.
///
/// See the module documentation for the rules on which fields to serialize and
/// why each group is marked `#[serde(skip)]`.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateAgent {
    // Stream ids used to read data in `act()`. Always `#[serde(skip)]`.
    #[serde(skip)]
    ohlcv_future_id: OhlcvId,

    // Grid search parameters. These configure the agent's behaviour and are the only
    // fields that are serialized.
    param_i32: i32,
    param_f64: f64,

    // Streaming indicators go here. Always `#[serde(skip)]`. Store the indicator's
    // configuration as a grid parameter above, for example an SMA period, and rebuild
    // the indicator from that parameter in the `with_*` methods.

    // Trading state. Always `#[serde(skip)]`.
    #[serde(skip)]
    state: AgentState,
    #[serde(skip)]
    trade_counter: i64,

    // Idempotency. One timestamp per data stream. A second OHLCV stream would add its
    // own `last_processed_ts_2`. Always `#[serde(skip)]`.
    #[serde(skip)]
    last_processed_ts: Option<DateTime<Utc>>,

    #[serde(skip)]
    agent_id: AgentIdentifier,
}

impl TemplateAgent {
    /// Builds the trading environment for this agent.
    ///
    /// This example builds the environment from scratch with `chapaty::make`,
    /// which fetches data from the configured `DataSource`.
    /// `DataSource::Hosted` reads `CHAPATY_CREDENTIAL` from the environment,
    /// so load your `.env` before calling this. If you only need a
    /// ready-made dataset, you can use a preset instead with
    /// `chapaty::load(EnvPreset::..., &io_cfg)`.
    ///
    /// Pick an episode length that is longer than any streaming indicator's
    /// warmup. The agent resets at every episode boundary, so a short
    /// episode can stop an indicator from ever warming up.
    /// `EpisodeLength::Infinite` never resets during the run and
    /// is the safe default.
    pub async fn env() -> Result<Environment> {
        let source = DataSource::Hosted;
        let ohlcv_query = ohlcv_future_query();
        let allowed_years = (2006..=2026).collect::<BTreeSet<_>>();
        let filter = FilterConfig {
            allowed_years: Some(allowed_years),
            ..FilterConfig::default()
        };
        let cfg = EnvConfig::default()
            .add_ohlcv_future(source.clone(), ohlcv_query)
            .with_episode_length(EpisodeLength::Infinite)
            .with_filter_config(filter)
            .with_trade_hint(2);

        chapaty::make(cfg)
            .await
            .context("Failed to load trading environment")
    }

    /// Creates a new agent with the default parameters.
    pub fn new() -> Self {
        Self {
            ohlcv_future_id: ohlcv_future_id(),
            param_i32: 1,
            param_f64: 2.0,
            state: AgentState::default(),
            trade_counter: 0,
            last_processed_ts: None,
            agent_id: AgentIdentifier::Named(Arc::new("TemplateAgent".to_string())),
        }
    }

    /// Overrides `param_i32`. Use this when building a grid of parameters.
    pub fn with_param_i32(self, param_i32: i32) -> Self {
        Self { param_i32, ..self }
    }

    /// Overrides `param_f64`. Use this when building a grid of parameters.
    ///
    /// If a parameter configures a streaming indicator, rebuild the indicator
    /// here from the new value, for example `fast_sma:
    /// StreamingSma::new(SmaWindow(param))`.
    pub fn with_param_f64(self, param_f64: f64) -> Self {
        Self { param_f64, ..self }
    }
}

impl Agent for TemplateAgent {
    fn identifier(&self) -> AgentIdentifier {
        self.agent_id.clone()
    }

    fn reset(&mut self) {
        // Reset every streaming indicator here by calling its `.reset()` method. Never
        // rebuild an indicator from scratch, because that forces you to thread its
        // configuration through again by hand and one mismatch changes the indicator
        // silently between episodes.
        self.state = AgentState::default();
        self.trade_counter = 0;
        self.last_processed_ts = None;
    }

    fn act(&mut self, obs: Observation) -> ChapatyResult<Actions> {
        let market_view = &obs.market_view;

        // 1. Read the latest bar. If the stream has no data yet, wait for the next
        //    call.
        let Some(candle) = market_view.ohlcv().last_event(&self.ohlcv_future_id) else {
            return Ok(Actions::no_op());
        };

        // If you need the live market price for a calculation or an order, read it
        // safely. Never use `?` or `.unwrap()` here, because the price can be missing
        // when the market has not printed a tick yet, and that would crash the run.
        //
        //     let price = match
        // market_view.try_resolved_close_price(self.ohlcv_future_id.symbol) {
        //         Ok(p) => p.0,
        //         Err(_) => return Ok(Actions::no_op()),
        //     };

        // 2. Advance internal state once per bar (idempotency check). `act()` can be
        //    called several times for the same bar, so only move forward when the close
        //    timestamp is new. This is also where you would feed a new close into a
        //    streaming indicator, for example `self.sma.update(candle.close.0)`.
        if self.last_processed_ts != Some(candle.close_timestamp) {
            self.last_processed_ts = Some(candle.close_timestamp);
        }

        // If you use streaming indicators, they return `None` until they have enough
        // bars to warm up. Return `Actions::no_op()` while they are still warming up.

        // 3. Decide what to do in the current state: entry, trade management, or exit.
        let actions = match self.state {
            AgentState::PreTrade { .. } | AgentState::InTrade { .. } | AgentState::PostTrade => {
                Actions::no_op()
            }
        };

        // 4. Return the actions for the engine to execute.
        Ok(actions)
    }
}

impl TemplateAgent {
    #[expect(
        dead_code,
        reason = "Optional market open helper provided as a template building block. Remove it if not needed."
    )]
    fn open_market(&mut self, trade_type: TradeKind) -> Action {
        self.trade_counter += 1;
        Action::Open(OpenCmd {
            agent_id: self.identifier(),
            trade_id: TradeId(self.trade_counter),
            trade_type,
            quantity: Quantity(1.0),
            entry_price: None, // None means a market order.
            stop_loss: None,
            take_profit: None,
        })
    }

    #[expect(
        dead_code,
        reason = "Optional market close helper provided as a template building block. Remove it if not needed."
    )]
    fn close_market(&self, trade_id: TradeId) -> Action {
        Action::MarketClose(MarketCloseCmd {
            agent_id: self.identifier(),
            trade_id,
            quantity: None, // None closes the full position.
        })
    }
}

// ================================================================================================
// Helper Types
// ================================================================================================

/// The phase the agent is in during the current trading session.
///
/// Many agents are a state machine over time. Modelling that explicitly often
/// keeps `act()` simple. Remove this enum if your strategy does not need it.
#[expect(
    dead_code,
    reason = "Optional state machine provided as a template building block. Remove it if not needed."
)]
#[expect(
    clippy::enum_variant_names,
    reason = "PreTrade, InTrade, and PostTrade are the clearest names for this state machine; dropping the shared Trade suffix would lose meaning."
)]
#[derive(Debug, Clone, Copy)]
enum AgentState {
    PreTrade { active_setup: Option<ActiveSetup> },
    InTrade { entry_time: DateTime<Utc> },
    PostTrade,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::PreTrade { active_setup: None }
    }
}

/// Data for a trade setup that is active or waiting for confirmation.
#[expect(
    dead_code,
    reason = "Optional setup metadata provided as a template building block. Remove it if not needed."
)]
#[derive(Debug, Copy, Clone)]
struct ActiveSetup {
    direction: TradeKind,
    price: Price,
    ts: DateTime<Utc>,
}

// ================================================================================================
// Grid Search Builder
// ================================================================================================

pub struct TemplateAgentGrid {
    i32_grid: Vec<i32>,
    f64_grid: GridAxis,
}

impl TemplateAgentGrid {
    /// A baseline search space. Use `GridAxis` for float ranges and plain
    /// iterators for integer ranges.
    pub fn baseline() -> ChapatyResult<Self> {
        Ok(Self {
            i32_grid: (-5..5).step_by(1).collect(),
            f64_grid: GridAxis::new("1.0", "2.0", "0.1")?,
        })
    }

    /// Builds every agent in the grid. Assign a unique id with `enumerate` and
    /// filter out invalid parameter combinations before collecting.
    pub fn build(self) -> Vec<(usize, TemplateAgent)> {
        let f64s = self.f64_grid.generate();

        iproduct!(self.i32_grid, f64s)
            .enumerate()
            .map(|(uid, (param_i32, param_f64))| {
                (
                    uid,
                    TemplateAgent::new()
                        .with_param_i32(param_i32)
                        .with_param_f64(param_f64),
                )
            })
            .collect()
    }
}

// ================================================================================================
// Environment Data Queries
// ================================================================================================

/// The OHLCV stream this agent trades on.
///
/// This query has no batch indicators. To add one, put it in the `indicators`
/// list, for example `vec![BatchOhlcvIndicator::Sma(SmaWindow(20))]`. A
/// non-empty `vec!` allocates, so the function can no longer be `const fn` once
/// you add an indicator.
const fn ohlcv_future_query() -> OhlcvFutureQuery {
    OhlcvFutureQuery {
        broker: DataBroker::NinjaTrader,
        exchange: Some(Exchange::Cme),
        symbol: Symbol::Future(FutureContract {
            root: FutureRoot::EminiNasdaq100,
            month: ContractMonth::September,
            year: ContractYear::Y6,
        }),
        period: Period::Hour(4),
        batch_size: 1000,
        indicators: Vec::new(),
    }
}

// ================================================================================================
// Stream IDs
//
// Every data stream is referenced by a stream id. There are two ways to get
// one.
//
// 1. From a query. Every query type (OhlcvFutureQuery, OhlcvSpotQuery,
//    TradesSpotQuery, EconomicCalendarQuery, and so on) implements the
//    `QueryId` trait, which is in the prelude. Call `.to_id()` on the query to
//    get its stream id. It returns a `ChapatyResult`, because it validates the
//    broker and exchange combination. You then use that id to read the stream
//    in `act()`, for example `obs.market_view.ohlcv().last_event(&ohlcv_id)`.
//
//        let ohlcv_id: OhlcvId = ohlcv_future_query().to_id()?;
//
// 2. From a batch indicator. Batch indicators do not have a `.to_id()` helper,
//    so you build their id by hand. Every batch indicator id has a `parent`
//    field, which is the OHLCV stream id the indicator was computed from, plus
//    the same config you added to the query's `indicators` list. The field
//    names are not uniform, so read `indicator/batch/event.rs` in the chapaty
//    source to confirm them.
//
//        // If the query carried this indicator:
//        //     indicators: vec![BatchOhlcvIndicator::Sma(SmaWindow(20))]
//        // then the matching id is:
//        let sma_id = SmaId{parent: ohlcv_future_id(), length: SmaWindow(20)};
//        // and you read it with:
//        //     obs.market_view.sma().last_event(&sma_id)
//
//        // A session range example, used by overnight strategies:
//        //     let cfg = SessionCfg {
//        //         window: SessionWindow::us_overnight(),
//        //         price_aggregation: AggregatedPrice::Hlc3,
//        //     };
//        //     indicators: vec![BatchOhlcvIndicator::OvernightRange(cfg)]
//        // then:
//        //     let session_id = OhlcvSessionId{parent:ohlcv_future_id(),cfg};
//        //     obs.market_view.ohlcv_session().last_event(&session_id)
// ================================================================================================

#[expect(
    clippy::expect_used,
    reason = "ohlcv_future_query() is a hardcoded, valid literal. If to_id() fails, the fault is inside chapaty, not this template, so panicking surfaces the real problem immediately."
)]
fn ohlcv_future_id() -> OhlcvId {
    ohlcv_future_query().to_id().expect(
        "CHAPATY BUG: to_id() rejected a hardcoded OhlcvFutureQuery. \
         This is not a template or user error. \
         Please report it at https://github.com/LenWilliamson/chapaty/issues \
         or on Discord at https://discord.gg/MmMAB6NCuK",
    )
}
