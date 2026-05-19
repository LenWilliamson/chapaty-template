use std::sync::Arc;

use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

use chapaty::prelude::*;

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
    pub fn new(ohlcv_id: OhlcvId, fast_period: u16, slow_period: u16) -> Self {
        Self {
            ohlcv_id,
            fast_period,
            slow_period,
            fast_sma: StreamingSma::new(fast_period),
            slow_sma: StreamingSma::new(slow_period),
            trade_counter: 0,
            current_fast: None,
            current_slow: None,
            last_processed_ts: None,
            agent_id: AgentIdentifier::Named(Arc::new("DemoAgent".to_string())),
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
            Some(TradeType::Long)
        } else if fast < slow {
            Some(TradeType::Short)
        } else {
            None // fast == slow, no clear signal
        };

        // 5b. Determine the Current State
        let current_dir = active_trade.map(|(_, state)| *state.trade_type());

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
    fn open(&mut self, trade_type: TradeType) -> Action {
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

    fn close_market(&self, trade_id: TradeId) -> Action {
        Action::MarketClose(MarketCloseCmd {
            agent_id: self.identifier(),
            trade_id,
            quantity: None,
        })
    }
}

pub struct DemoAgentGrid {
    ohlcv_id: OhlcvId,
    fast_period: GridAxis,
    slow_period: GridAxis,
}

impl DemoAgentGrid {
    pub fn baseline(ohlcv_id: OhlcvId) -> ChapatyResult<Self> {
        Ok(Self {
            ohlcv_id,
            fast_period: GridAxis::new("10", "30", "1")?,
            slow_period: GridAxis::new("40", "60", "1")?,
        })
    }

    pub fn build(self) -> Vec<(usize, DemoAgent)> {
        let fasts = self.fast_period.generate();
        let slows = self.slow_period.generate();
        let ohlcv_id = self.ohlcv_id;

        iproduct!(fasts, slows)
            .filter(|(f, s)| f < s)
            .enumerate()
            .map(|(uid, (fast, slow))| (uid, DemoAgent::new(ohlcv_id, fast as u16, slow as u16)))
            .collect()
    }
}
