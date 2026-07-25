use std::sync::Arc;

use anyhow::{Context, Result};
use chapaty::prelude::*;
use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DemoAgent {
    #[serde(skip)]
    ohlcv_id: OhlcvId,

    fast_period: u16,
    slow_period: u16,

    #[serde(skip)]
    fast_sma: StreamingSma,
    #[serde(skip)]
    slow_sma: StreamingSma,

    #[serde(skip)]
    current_fast: Option<f64>,
    #[serde(skip)]
    current_slow: Option<f64>,

    #[serde(skip)]
    trade_counter: i64,

    #[serde(skip)]
    last_processed_ts: Option<DateTime<Utc>>,

    #[serde(skip)]
    agent_id: AgentIdentifier,
}

impl DemoAgent {
    pub async fn env() -> Result<Environment> {
        let preset = EnvPreset::BinanceBtcUsdt1d;
        let file_stem = preset.to_string();

        let loc = StorageLocation::HuggingFace { version: None };
        let cfg = IoConfig::new(loc).with_file_stem(&file_stem);

        chapaty::load(preset, &cfg)
            .await
            .context("Failed to load trading environment")
    }

    pub fn new() -> Self {
        Self {
            ohlcv_id: ohlcv_id(),
            fast_period: 20,
            slow_period: 50,
            fast_sma: StreamingSma::new(SmaWindow(20)),
            slow_sma: StreamingSma::new(SmaWindow(50)),
            trade_counter: 0,
            current_fast: None,
            current_slow: None,
            last_processed_ts: None,
            agent_id: AgentIdentifier::Named(Arc::new("DemoAgent".to_string())),
        }
    }

    pub fn with_fast_period(self, fast_period: u16) -> Self {
        Self {
            fast_period,
            fast_sma: StreamingSma::new(SmaWindow(fast_period)),
            ..self
        }
    }

    pub fn with_slow_period(self, slow_period: u16) -> Self {
        Self {
            slow_period,
            slow_sma: StreamingSma::new(SmaWindow(slow_period)),
            ..self
        }
    }
}

impl Agent for DemoAgent {
    fn identifier(&self) -> AgentIdentifier {
        self.agent_id.clone()
    }

    fn reset(&mut self) {
        self.fast_sma.reset();
        self.slow_sma.reset();
        self.trade_counter = 0;
        self.current_fast = None;
        self.current_slow = None;
        self.last_processed_ts = None;
    }

    fn act(&mut self, obs: Observation) -> ChapatyResult<Actions> {
        let market_view = &obs.market_view;

        // 1. Fetch the latest candle safely
        let Some(candle) = market_view.ohlcv().last_event(&self.ohlcv_id) else {
            return Ok(Actions::no_op());
        };

        // 2. Update Internal State (Idempotency check)
        if self.last_processed_ts != Some(candle.close_timestamp) {
            self.current_fast = self.fast_sma.update(candle.close.0);
            self.current_slow = self.slow_sma.update(candle.close.0);
            self.last_processed_ts = Some(candle.close_timestamp);
        }

        // 3. Check Signal Validity
        let (Some(fast), Some(slow)) = (self.current_fast, self.current_slow) else {
            return Ok(Actions::no_op()); // SMAs are still warming up
        };

        // 4. Determine Position Status
        let agent_id = self.identifier();
        let active_trade = obs.states.find_active_trade_for_agent(&agent_id);
        let market_id: MarketId = self.ohlcv_id.into();

        let mut actions = Actions::new();

        // 5a. Determine the Target State
        let desired_dir = if fast > slow {
            Some(TradeKind::Long)
        } else if fast < slow {
            Some(TradeKind::Short)
        } else {
            None // fast == slow, no clear signal
        };

        // 5b. Determine the Current State
        let current_dir = active_trade.map(|(_, state)| state.trade_kind());

        // 5c. Bridge the Gap
        if current_dir != desired_dir {
            // 1. Clear the old state if it exists
            if let Some((_, state)) = active_trade {
                actions.add(market_id, self.close_market(state.trade_id()));
            }

            // 2. Enter the new state if there is a signal
            if let Some(dir) = desired_dir {
                actions.add(market_id, self.open(dir));
            }
        }

        Ok(actions)
    }
}

impl DemoAgent {
    fn open(&mut self, trade_kind: TradeKind) -> Action {
        self.trade_counter += 1;
        Action::Open(OpenCmd {
            agent_id: self.identifier(),
            trade_id: TradeId(self.trade_counter),
            trade_kind,
            quantity: Quantity(1.0),
            entry_price: None, // Market Order
            stop_loss: None,
            take_profit: None,
        })
    }

    fn close_market(&self, trade_id: TradeId) -> Action {
        Action::MarketClose(MarketCloseCmd {
            agent_id: self.identifier(),
            trade_id,
            quantity: None,
        })
    }
}

pub struct DemoAgentGrid {
    fast_period: Vec<u16>,
    slow_period: Vec<u16>,
}

impl DemoAgentGrid {
    pub fn baseline() -> Self {
        Self {
            fast_period: (10..30).step_by(1).collect(),
            slow_period: (40..60).step_by(1).collect(),
        }
    }

    pub fn build(self) -> Vec<(usize, DemoAgent)> {
        iproduct!(self.fast_period, self.slow_period)
            .filter(|(f, s)| f < s)
            .enumerate()
            .map(|(uid, (fast, slow))| {
                (
                    uid,
                    DemoAgent::new()
                        .with_fast_period(fast)
                        .with_slow_period(slow),
                )
            })
            .collect()
    }
}

const fn ohlcv_id() -> OhlcvId {
    OhlcvId {
        broker: DataBroker::Binance,
        exchange: Exchange::Binance,
        symbol: Symbol::Spot(SpotPair::BtcUsdt),
        period: Period::Day(1),
    }
}
