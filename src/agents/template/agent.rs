//! # Template Agent
//!
//! This module is a starting point for building a trading strategy with
//! chapaty. Every struct, name, and helper here is an example. Keep what fits
//! your strategy, remove what you do not need, and add new building blocks such
//! as custom indicators, enums, or helper types whenever the strategy calls for
//! them.
//!
//! For a complete, minimal working strategy, read the demo agent that ships
//! alongside this template. For advanced, production-grade references, see
//! [chapaty-zoo](https://github.com/LenWilliamson/chapaty-zoo).
//!
//! ## How this template maps to the specification
//!
//! The layout of this file follows the layout of the specification, so you can
//! check the implementation against the specification one section at a time.
//! The section markers below point at the place where each part of the
//! specification belongs.
//!
//! * Section 2, Environment, belongs in `env()`.
//! * Section 3, Observation Inputs, belongs in steps 1 to 3 of `act()`.
//! * Section 4, Entry Logic, belongs in the `AgentState::PreTrade` arm of
//!   `act()`.
//! * Section 5, Exit Logic, belongs in the `AgentState::InTrade` arm of
//!   `act()`.
//! * Section 6, Parameters, belongs in the serialized fields, the `with_*`
//!   methods, and `TemplateAgentGrid`.
//!
//! ## What to serialize
//!
//! Serialize only the grid search parameters, which are the values that
//! configure the agent's behaviour and that you sweep over during a grid
//! search. These are the fields that show up in the leaderboard. Mark
//! everything else with `#[serde(skip)]`: stream ids, streaming indicators,
//! trading state, counters, and idempotency timestamps.
//!
//! ## Every parameter needs a call site
//!
//! A parameter is only real once it is used. Each one travels the same path:
//! you declare it as a serialized field, you set it in a `with_*` method, you
//! sweep it in `TemplateAgentGrid`, and you read it somewhere in `act()` or in
//! a helper. Both example parameters below travel that full path. If you add a
//! parameter and never read it, the grid search will produce many rows that
//! behave identically, which is easy to miss because the results still look
//! plausible.
//!
//! ## Streaming indicators
//!
//! Store the indicator's configuration as a serialized parameter, not the
//! indicator itself. For example, store the SMA period as a grid parameter and
//! keep the `StreamingSma` in a `#[serde(skip)]` field. Serializing the
//! indicator would write a large amount of internal state you do not need.
//! Rebuild the indicator from its parameter inside the matching `with_*`
//! method, and reset it in `reset()`.
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
//! Many agents are naturally a state machine over time, so this template drives
//! `act()` with an `AgentState` enum. If your strategy is simpler, delete the
//! enum and act directly on the observation.

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

    // ------------------------------------------------------------------------------------------
    // Specification section 6: Parameters
    //
    // Grid search parameters. These configure the agent's behaviour and are the only fields that
    // are serialized. Every parameter here is read somewhere in the logic below, and replacing
    // them with the parameters from your specification is the first change to make.
    // ------------------------------------------------------------------------------------------
    /// How long a trade may stay open before the exit rule closes it. Read in
    /// the `AgentState::InTrade` arm of `act()`.
    max_holding_minutes: i64,

    /// How many contracts or units each trade opens with. Read in
    /// `open_market()`.
    position_size: f64,

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
    // ==========================================================================================
    // Specification section 2: Environment
    // ==========================================================================================

    /// Builds the trading environment for this agent.
    ///
    /// This example builds the environment from scratch with `chapaty::make`,
    /// which fetches data from the configured `DataSource`.
    /// `DataSource::Hosted` reads `CHAPATY_CREDENTIAL` from the
    /// environment. If you only need a ready-made dataset, use a preset
    /// instead with `chapaty::load(EnvPreset::..., &io_cfg)`.
    ///
    /// Pick an episode length that is longer than any streaming indicator's
    /// warmup. The agent resets at every episode boundary, so a short
    /// episode can stop an indicator from ever warming up.
    /// `EpisodeLength::Infinite` never resets during the run and is
    /// the safe default.
    ///
    /// Read the note on `AgentState::PostTrade` in `act()` before you change
    /// this value. The episode length and the way your state machine leaves
    /// `PostTrade` decide how often the agent is allowed to trade, and the
    /// two settings have to agree.
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
            max_holding_minutes: 30,
            position_size: 1.0,
            state: AgentState::default(),
            trade_counter: 0,
            last_processed_ts: None,
            agent_id: AgentIdentifier::Named(Arc::new("TemplateAgent".to_string())),
        }
    }

    /// Overrides `max_holding_minutes`. Use this when building a grid of
    /// parameters.
    pub fn with_max_holding_minutes(self, max_holding_minutes: i64) -> Self {
        Self {
            max_holding_minutes,
            ..self
        }
    }

    /// Overrides `position_size`. Use this when building a grid of parameters.
    ///
    /// If a parameter configures a streaming indicator, rebuild the indicator
    /// here from the new value, for example `sma:
    /// StreamingSma::new(SmaWindow(period))`.
    pub fn with_position_size(self, position_size: f64) -> Self {
        Self {
            position_size,
            ..self
        }
    }
}

impl Default for TemplateAgent {
    fn default() -> Self {
        Self::new()
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

        // ======================================================================================
        // Specification section 3: Observation Inputs
        // ======================================================================================

        // 1. Read the latest bar. Wait for the next call if the stream has no data yet.
        let Some(candle) = market_view.ohlcv().last_event(&self.ohlcv_future_id) else {
            return Ok(Actions::no_op());
        };

        // If you need the live market price for a calculation or an order, read it
        // safely. Never use `?` or `.unwrap()` here, because the price can be
        // missing when the market has not printed a tick yet, and that would
        // crash the run.
        //
        //     let price = match
        // market_view.try_resolved_close_price(self.ohlcv_future_id.symbol) {
        //         Ok(p) => p.0,
        //         Err(_) => return Ok(Actions::no_op()),
        //     };

        // 2. Advance internal state once per new bar (idempotency check). `act()` may
        //    run several times for the same bar, so only move forward on a new close
        //    timestamp. Feed any streaming indicator here, for example
        //    `self.current_sma = self.sma.update(candle.close.0)`. While an indicator
        //    is warming up it returns `None`, so return `Actions::no_op()` until it is
        //    ready.
        if self.last_processed_ts != Some(candle.close_timestamp) {
            self.last_processed_ts = Some(candle.close_timestamp);
        }

        // 3. Read the current position. Entry and exit logic branch on whether a trade
        //    is open, and closing a trade needs its `trade_id`. Your own `self.state`
        //    and the engine's view of the position can disagree, because the engine
        //    closes a trade on its stop loss or take profit without telling the state
        //    machine. Always treat the engine as the source of truth for whether a
        //    trade is still open.
        let active_trade = obs.states.find_active_trade_for_agent(&self.identifier());
        let market_id: MarketId = self.ohlcv_future_id.into();

        // 4. Drive the state machine. Replace the placeholder decisions with your
        //    logic.
        let actions = match self.state {
            // ==================================================================================
            // Specification section 4: Entry Logic
            // ==================================================================================
            AgentState::PreTrade => {
                // Evaluate your entry condition here, for example from a streaming
                // indicator or the candle, and produce `EntrySignal::Enter(direction)`
                // when it triggers. Replace the single line below and leave the rest of
                // this arm as it is, because it already wires up the state transition and
                // the order.
                let signal = EntrySignal::Stay; // TODO: replace with your entry condition.

                match signal {
                    EntrySignal::Enter(direction) => {
                        self.state = AgentState::InTrade {
                            entry_time: market_view.current_timestamp(),
                        };
                        Actions::from((market_id, self.open_market(direction)))
                    }
                    EntrySignal::Stay => Actions::no_op(),
                }
            }

            // ==================================================================================
            // Specification section 5: Exit Logic
            // ==================================================================================
            AgentState::InTrade { entry_time } => {
                // The engine handles the stop loss and take profit you set on the order.
                // Add discretionary exits here. This example closes the trade once it has
                // been open for longer than the `max_holding_minutes` parameter. Replace
                // the rule with your own, and keep reading the limit from a parameter
                // instead of writing a number here, so that the grid search can sweep it.
                let held_minutes = market_view
                    .current_timestamp()
                    .signed_duration_since(entry_time)
                    .num_minutes();
                let decision = if held_minutes >= self.max_holding_minutes {
                    ExitDecision::Close
                } else {
                    ExitDecision::Hold
                };

                match (active_trade, decision) {
                    // The trade is still open and our exit rule fired: close it.
                    (Some((_, state)), ExitDecision::Close) => {
                        self.state = AgentState::PostTrade;
                        Actions::from((market_id, self.close_market(state.trade_id())))
                    }
                    // The engine already closed the trade on its stop loss or take profit.
                    (None, _) => {
                        self.state = AgentState::PostTrade;
                        Actions::no_op()
                    }
                    // The trade is still open: keep holding.
                    (Some(_), ExitDecision::Hold) => Actions::no_op(),
                }
            }

            AgentState::PostTrade => {
                // The previous trade is finished. This template goes straight back to
                // `PreTrade`, so the agent can look for the next setup on the following
                // bar.
                //
                // Think carefully before you change this, because this arm and the episode
                // length together decide how often the agent trades. `env()` uses
                // `EpisodeLength::Infinite`, which never resets the agent during a run. If
                // you make `PostTrade` a final state under an infinite episode, the agent
                // takes one single trade over the whole dataset and then does nothing for
                // the rest of the run. The grid search still completes and the leaderboard
                // still fills up, so the mistake is easy to miss.
                //
                // Keep this transition for a strategy that trades repeatedly. Remove it
                // only if your strategy takes one trade per episode, and then set a finite
                // episode length in `env()` as well, for example one episode per trading
                // day.
                self.state = AgentState::PreTrade;
                Actions::no_op()
            }
        };

        // 5. Return the actions for the engine to execute.
        Ok(actions)
    }
}

impl TemplateAgent {
    /// Opens a market order and assigns it the next unique trade id.
    ///
    /// A market order fills right away, so the trade is active on the next call
    /// and `find_active_trade_for_agent` finds it. If you switch
    /// `entry_price` to `Some(price)`, the order starts out pending
    /// instead, and that lookup will not find it until it fills. The exit
    /// arm would then treat the unfilled order as an already closed trade.
    /// If you use limit orders, also check `find_pending_trade_for_agent`,
    /// and cancel a pending order with `Action::Cancel` rather than
    /// `Action::MarketClose`.
    fn open_market(&mut self, trade_kind: TradeKind) -> Action {
        self.trade_counter += 1;
        Action::Open(OpenCmd {
            agent_id: self.identifier(),
            trade_id: TradeId(self.trade_counter),
            trade_kind,
            quantity: Quantity(self.position_size),
            entry_price: None, // None means a market order. Some(Price(x)) is a limit.
            stop_loss: None,
            take_profit: None,
        })
    }

    /// Closes the given trade at market. `quantity: None` closes the full
    /// position.
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
/// A state can also carry data, the way `InTrade` carries `entry_time`. The
/// real strategy examples use this to remember pending orders or an active
/// setup between calls.
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

/// The outcome of evaluating the entry condition in the `PreTrade` state.
#[expect(
    dead_code,
    reason = "Template placeholder. The Enter variant is only constructed once you implement the entry condition, so remove this attribute as soon as you do. The compiler will remind you, because an expectation that never fires is itself reported."
)]
#[derive(Debug, Clone, Copy)]
enum EntrySignal {
    /// Open a new trade in the given direction.
    Enter(TradeKind),
    /// No entry condition met. Stay flat.
    Stay,
}

/// The outcome of evaluating the exit rule for an open trade in the `InTrade`
/// state.
#[derive(Debug, Clone, Copy)]
enum ExitDecision {
    /// Close the open trade now.
    Close,
    /// Keep the open trade.
    Hold,
}

// ================================================================================================
// Grid Search Builder
//
// This is the second half of specification section 6. Every parameter you add
// to the agent needs an axis here, otherwise the grid search always uses its
// default value.
// ================================================================================================

pub struct TemplateAgentGrid {
    max_holding_minutes_grid: Vec<i64>,
    position_size_grid: GridAxis,
}

impl TemplateAgentGrid {
    /// A baseline search space. Use `GridAxis` for float ranges and plain
    /// iterators for integer ranges.
    pub fn baseline() -> ChapatyResult<Self> {
        Ok(Self {
            max_holding_minutes_grid: (30_i64..=240).step_by(30).collect(),
            position_size_grid: GridAxis::new("1.0", "3.0", "1.0")?,
        })
    }

    /// Builds every agent in the grid. Assign a unique id with `enumerate` and
    /// filter out invalid parameter combinations before collecting.
    pub fn build(self) -> Vec<(usize, TemplateAgent)> {
        let position_sizes = self.position_size_grid.generate();

        iproduct!(self.max_holding_minutes_grid, position_sizes)
            .enumerate()
            .map(|(uid, (max_holding_minutes, position_size))| {
                (
                    uid,
                    TemplateAgent::new()
                        .with_max_holding_minutes(max_holding_minutes)
                        .with_position_size(position_size),
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
//        let sma_id = SmaId {parent: ohlcv_future_id(), length: SmaWindow(20)};
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
//        //     let id = OhlcvSessionId { parent: ohlcv_future_id(), cfg };
//        //     obs.market_view.ohlcv_session().last_event(&id)
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
