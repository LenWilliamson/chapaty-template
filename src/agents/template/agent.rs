//! # Template Agent Module
//!
//! This modul contains example building blocks for creating a trading strategy
//! with chapaty. The pre configured structs, namings, etc. are just ideas and
//! can be used if applicable. If not needed they can be dropped, fields can be
//! removed or replaced. If needed they can be extended or new building blocks
//! such as custom trading indicator implementations, enums, types etc. can be
//! created freely to build the trading agent according to the chapaty framwork.
//!
//! Only serialize copy variables if we have sma period for streaming sma than
//! add this as a parameter and don't seralize the Streaming SMA itself it is
//! too much data
//!
//! Everything is just an example also the implementation of the act function
//! maybe a AgentPhase is not needed at all and complicates the implementqtion

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result};
use chapaty::prelude::*;
use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

/// Represents the agent's state during the current trading session. All
/// parameters should be `#[serde(skip)]` except the ones used for grid search.
/// The parameters used for grid search are exactly those parameters
/// that are configurations for the trading agents behaviour.
///
/// For each Simulation Data stream ID one should add a last_processed_ts to
/// handle idempotency. So for ohlcv_id_2 we would have last_processed_ts_2.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateAgent {
    // === Simulation Data Stream IDs to access data streams (always `#[serde(skip)]`) ===
    #[serde(skip)]
    ohlcv_future_id: OhlcvId,

    // === Agent / Grid Search Parameters (must be serialized) ===
    param_i32: i32,
    param_f64: f64,

    // === Streaming Indicators (always `#[serde(skip)]`) ===

    // === Trading State (always `#[serde(skip)]`) ===
    #[serde(skip)]
    state: AgentState,
    #[serde(skip)]
    trade_counter: i64,

    // === Idemptency Parameters (always `#[serde(skip)]`) ===
    #[serde(skip)]
    last_processed_ts: Option<DateTime<Utc>>,
    #[serde(skip)]
    agent_id: AgentIdentifier,
}

impl TemplateAgent {
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

    /// Creates a new agent utilizing the defaults defined in the specification.
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

    /// Overrides the `param_i32` field with the given value. Useful when
    /// generating a grid search of parameters.
    pub fn with_param_i32(self, param_i32: i32) -> Self {
        Self { param_i32, ..self }
    }

    /// Overrides the `param_f64` field with the given value. Useful when
    /// generating a grid search of parameters.
    pub fn with_param_f64(self, param_f64: f64) -> Self {
        Self { param_f64, ..self }
    }
}

impl Agent for TemplateAgent {
    fn identifier(&self) -> AgentIdentifier {
        self.agent_id.clone()
    }

    fn reset(&mut self) {
        // Call `.reset()` here on every configured `StreamingIndicator`
        self.state = AgentState::default();
        self.trade_counter = 0;
        self.last_processed_ts = None;
    }

    fn act(&mut self, obs: Observation) -> ChapatyResult<Actions> {
        let market_view = &obs.market_view;

        // 1. Fetch the latest candle safely
        let Some(candle) = market_view.ohlcv().last_event(&self.ohlcv_future_id) else {
            return Ok(Actions::no_op()); // No data available, wait for next observation
        };

        // 2. Update Internal State (Idempotency check)
        if self.last_processed_ts != Some(candle.close_timestamp) {
            // update internal state variables
            self.last_processed_ts = Some(candle.close_timestamp);
        }

        // 3. Entry Logic / Trade Management / Exit Logic for the current state
        let actions = match self.state {
            AgentState::PreTrade { .. } => Actions::no_op(),
            AgentState::InTrade { .. } => Actions::no_op(),
            AgentState::PostTrade => Actions::no_op(),
        };

        // 4. Return the actions to execute
        Ok(actions)
    }
}

impl TemplateAgent {
    #[allow(
        dead_code,
        reason = "Optional open market order execution helper provided as template building block for an agent implementation. Remove this function if not needed."
    )]
    fn open_market(&mut self, trade_type: TradeKind) -> Action {
        self.trade_counter += 1;
        Action::Open(OpenCmd {
            agent_id: self.identifier(),
            trade_id: TradeId(self.trade_counter),
            trade_type,
            quantity: Quantity(1.0),
            entry_price: None, // Market Order
            stop_loss: None,
            take_profit: None,
        })
    }

    #[allow(
        dead_code,
        reason = "Optional close market order execution helper provided as template building block for an agent implementation. Remove this function if not needed."
    )]
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

/// Represents the exact phase the agent is in during the current trading
/// session.
#[allow(
    dead_code,
    reason = "Optional state enum to create an internal state machine. In many cases an `Agent` is a state machine over time `t`. It can simplify the implementation. Remove this enum if not needed."
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
/// Data for a trade setup that is currently active, or waiting for a
/// confirmation to be activated.
#[allow(
    dead_code,
    reason = "Optional active trade setup metadata struct provided for signal tracking. Remove this struct if not needed."
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
    pub fn baseline() -> ChapatyResult<Self> {
        Ok(Self {
            i32_grid: (-5..5).step_by(1).collect(),
            f64_grid: GridAxis::new("1.0", "2.0", "0.1")?,
        })
    }

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
// ================================================================================================

fn ohlcv_future_id() -> OhlcvId {
    ohlcv_future_query()
        .to_id()
        .expect("OhlcvFutureQuery should always yield a valid OhlcvId")
}
