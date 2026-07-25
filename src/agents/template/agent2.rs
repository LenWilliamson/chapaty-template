//! # Template Agent
//!
//! This module is a starting point for building a trading strategy with chapaty.
//! Every struct, name, and helper here is an example. Keep what fits your strategy,
//! remove what you do not need, and add new building blocks such as custom indicators,
//! enums, or helper types whenever the strategy calls for them.
//!
//! For a complete, minimal working strategy, read the demo agent that ships alongside
//! this template. For advanced, production-grade references, see chapaty-zoo.
//!
//! ## What to serialize
//!
//! Serialize only the grid search parameters, which are the values that configure the
//! agent's behaviour and that you sweep over during a grid search. These are the fields
//! that show up in the leaderboard. Mark everything else with `#[serde(skip)]`: stream
//! ids, streaming indicators, trading state, counters, and idempotency timestamps.
//!
//! ## Streaming indicators
//!
//! Store the indicator's configuration as a serialized parameter, not the indicator
//! itself. For example, store the SMA period as a grid parameter and keep the
//! `StreamingSma` in a `#[serde(skip)]` field. Serializing the indicator would write a
//! large amount of internal state you do not need. Rebuild the indicator from its
//! parameter inside the matching `with_*` method, and reset it in `reset()`.
//!
//! ## Idempotency
//!
//! `act()` can be called several times for the same bar. Keep the close timestamp of the
//! last bar you processed and only advance your indicators and state when a new bar
//! arrives. Use one timestamp per data stream, so a second OHLCV stream would get its own
//! `last_processed_ts_2` field.
//!
//! ## The state machine is optional
//!
//! Many agents are naturally a state machine over time, so this template drives `act()`
//! with an `AgentState` enum. If your strategy is simpler, delete the enum and act
//! directly on the observation.

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result};
use chapaty::prelude::*;
use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

/// A template trading agent.
///
/// See the module documentation for the rules on which fields to serialize and why each
/// group is marked `#[serde(skip)]`.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateAgent {
    // Stream ids used to read data in `act()`. Always `#[serde(skip)]`.
    #[serde(skip)]
    ohlcv_future_id: OhlcvId,

    // Grid search parameters. These configure the agent's behaviour and are the only
    // fields that are serialized. Read them wherever your logic needs them: inside
    // `act()`, to size an order, or to configure an indicator or the query.
    param_i32: i32,
    param_f64: f64,

    // Streaming indicators go here. Always `#[serde(skip)]`. Store the indicator's
    // configuration as a grid parameter above, for example an SMA period, and rebuild the
    // indicator from that parameter in the `with_*` methods.

    // Trading state. Always `#[serde(skip)]`.
    #[serde(skip)]
    state: AgentState,
    #[serde(skip)]
    trade_counter: i64,

    // Idempotency. One timestamp per data stream. A second OHLCV stream would add its own
    // `last_processed_ts_2`. Always `#[serde(skip)]`.
    #[serde(skip)]
    last_processed_ts: Option<DateTime<Utc>>,

    #[serde(skip)]
    agent_id: AgentIdentifier,
}

impl TemplateAgent {
    /// Builds the trading environment for this agent.
    ///
    /// This example builds the environment from scratch with `chapaty::make`, which
    /// fetches data from the configured `DataSource`. `DataSource::Hosted` reads
    /// `CHAPATY_API_KEY` from the environment, so load your `.env` before calling this. If
    /// you only need a ready-made dataset, use a preset instead with
    /// `chapaty::load(EnvPreset::..., &io_cfg)`.
    ///
    /// Pick an episode length that is longer than any streaming indicator's warmup. The
    /// agent resets at every episode boundary, so a short episode can stop an indicator
    /// from ever warming up. `EpisodeLength::Infinite` never resets during the run and is
    /// the safe default.
    pub async fn env() -> Result<Environment> {
        let source = DataSource::Hosted;
        let ohlcv_query = ohlcv_future_query();
        let allowed_years = (2006..=2026).collect::<BTreeSet<_>>();
        let filter = FilterConfig {
            allowed_years: Some(allowed_years),
            ..FilterConfig::default()
        };
        // Clone `source` when you add more than one data stream.
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
    /// If a parameter configures a streaming indicator, rebuild the indicator here from
    /// the new value, for example `sma: StreamingSma::new(SmaWindow(period))`.
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

        // 1. Read the latest bar. Wait for the next call if the stream has no data yet.
        let Some(candle) = market_view.ohlcv().last_event(&self.ohlcv_future_id) else {
            return Ok(Actions::no_op());
        };

        // If you need the live market price for a calculation or an order, read it safely.
        // Never use `?` or `.unwrap()` here, because the price can be missing when the
        // market has not printed a tick yet, and that would crash the run.
        //
        //     let price = match market_view.try_resolved_close_price(self.ohlcv_future_id.symbol) {
        //         Ok(p) => p.0,
        //         Err(_) => return Ok(Actions::no_op()),
        //     };

        // 2. Advance internal state once per new bar (idempotency check). `act()` may run
        //    several times for the same bar, so only move forward on a new close
        //    timestamp. Feed any streaming indicator here, for example
        //    `self.current_sma = self.sma.update(candle.close.0)`. While an indicator is
        //    warming up it returns `None`, so return `Actions::no_op()` until it is ready.
        if self.last_processed_ts != Some(candle.close_timestamp) {
            self.last_processed_ts = Some(candle.close_timestamp);
        }

        // 3. Read the current position. Entry and exit logic branch on whether a trade is
        //    open, and closing a trade needs its `trade_id`.
        let active_trade = obs.states.find_active_trade_for_agent(&self.identifier());
        let market_id: MarketId = self.ohlcv_future_id.into();

        // 4. Drive the state machine. Replace the placeholder conditions with your logic.
        let actions = match self.state {
            AgentState::PreTrade => {
                // Evaluate your entry condition here, for example from a streaming
                // indicator or the candle. When it triggers, open a trade and advance to
                // `InTrade`.
                let entry_signal = false; // TODO: replace with your entry condition.
                if entry_signal {
                    self.state = AgentState::InTrade {
                        entry_time: market_view.current_timestamp(),
                    };
                    Actions::from((market_id, self.open_market(TradeKind::Long)))
                } else {
                    Actions::no_op()
                }
            }

            AgentState::InTrade { entry_time } => {
                // The engine handles the stop loss and take profit you set on the order.
                // Add discretionary exits here. This example shows a maximum holding time;
                // replace the rule and the literal with your own.
                let held_minutes = market_view
                    .current_timestamp()
                    .signed_duration_since(entry_time)
                    .num_minutes();
                let time_exit = held_minutes >= 30; // TODO: make this a grid parameter.

                match active_trade {
                    // The trade is still open and our exit rule fired: close it.
                    Some((_, state)) if time_exit => {
                        self.state = AgentState::PostTrade;
                        Actions::from((market_id, self.close_market(state.trade_id())))
                    }
                    // The engine already closed the trade on its stop loss or take profit.
                    None => {
                        self.state = AgentState::PostTrade;
                        Actions::no_op()
                    }
                    // The trade is still open: keep holding.
                    Some(_) => Actions::no_op(),
                }
            }

            AgentState::PostTrade => {
                // The trade is done for this session. Wait for the next episode, or reset
                // `self.state` here if your strategy re-arms within the same episode.
                Actions::no_op()
            }
        };

        // 5. Return the actions for the engine to execute.
        Ok(actions)
    }
}

impl TemplateAgent {
    /// Opens a market order and assigns it the next unique trade id.
    fn open_market(&mut self, trade_type: TradeKind) -> Action {
        self.trade_counter += 1;
        Action::Open(OpenCmd {
            agent_id: self.identifier(),
            trade_id: TradeId(self.trade_counter),
            trade_type,
            quantity: Quantity(1.0),
            entry_price: None, // None means a market order. Some(Price(x)) is a limit.
            stop_loss: None,
            take_profit: None,
        })
    }

    /// Closes the given trade at market. `quantity: None` closes the full position.
    fn close_market(&self, trade_id: TradeId) -> Action {
        Action::MarketClose(MarketCloseCmd {
            agent_id: self.identifier(),
            trade_id,
            quantity: None,
        })
    }
}

// ================================================================================================
// Helper Types
// ================================================================================================

/// The phase the agent is in during the current trading session.
///
/// A state can also carry data, the way `InTrade` carries `entry_time`. The real strategy
/// examples use this to remember pending orders or an active setup between calls.
#[expect(
    clippy::enum_variant_names,
    reason = "PreTrade, InTrade, and PostTrade are the clearest names for this state machine; dropping the shared Trade suffix would lose meaning."
)]
#[derive(Debug, Clone, Copy)]
enum AgentState {
    PreTrade,
    InTrade { entry_time: DateTime<Utc> },
    PostTrade,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::PreTrade
    }
}

// ================================================================================================
// Grid Search Builder
// ================================================================================================

pub struct TemplateAgentGrid {
    i32_grid: Vec<i32>,
    f64_grid: GridAxis,
}

impl TemplateAgentGrid {
    /// A baseline search space. Use `GridAxis` for float ranges and plain iterators for
    /// integer ranges.
    pub fn baseline() -> ChapatyResult<Self> {
        Ok(Self {
            i32_grid: (-5..5).step_by(1).collect(),
            f64_grid: GridAxis::new("1.0", "2.0", "0.1")?,
        })
    }

    /// Builds every agent in the grid. Assign a unique id with `enumerate` and filter out
    /// invalid parameter combinations before collecting.
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
/// This query has no batch indicators. To add one, put it in the `indicators` list, for
/// example `vec![BatchOhlcvIndicator::Sma(SmaWindow(20))]`. A non-empty `vec!` allocates,
/// so the function can no longer be `const fn` once you add an indicator.
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
// Every data stream is referenced by a stream id. There are two ways to get one.
//
// 1. From a query. Every query type (OhlcvFutureQuery, OhlcvSpotQuery, TradesSpotQuery,
//    EconomicCalendarQuery, and so on) implements the `QueryId` trait, which is in the
//    prelude. Call `.to_id()` on the query to get its stream id. It returns a
//    `ChapatyResult`, because it validates the broker and exchange combination. You then
//    use that id to read the stream in `act()`, for example
//    `obs.market_view.ohlcv().last_event(&ohlcv_id)`.
//
//        let ohlcv_id: OhlcvId = ohlcv_future_query().to_id()?;
//
// 2. From a batch indicator. Batch indicators do not have a `.to_id()` helper, so you
//    build their id by hand. Every batch indicator id has a `parent` field, which is the
//    OHLCV stream id the indicator was computed from, plus the same config you added to
//    the query's `indicators` list. The field names are not uniform, so read
//    `indicator/batch/event.rs` in the chapaty source to confirm them.
//
//        // If the query carried this indicator:
//        //     indicators: vec![BatchOhlcvIndicator::Sma(SmaWindow(20))]
//        // then the matching id is:
//        let sma_id = SmaId { parent: ohlcv_future_id(), length: SmaWindow(20) };
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
//        //     let session_id = OhlcvSessionId { parent: ohlcv_future_id(), cfg };
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
