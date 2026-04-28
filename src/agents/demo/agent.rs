use std::sync::Arc;

use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

use chapaty::{gym::GridAxis, prelude::*};

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
        }
    }
}

impl Agent for DemoAgent {
    fn identifier(&self) -> AgentIdentifier {
        AgentIdentifier::Named(Arc::new("DemoAgent".to_string()))
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

        // 5. Signal Logic (Stop-and-Reverse)
        if fast > slow {
            // Bullish Trend
            if let Some((_, state)) = active_trade {
                if state.trade_type() == &TradeType::Short {
                    // Close the Short
                    let close_cmd = MarketCloseCmd {
                        agent_id: agent_id.clone(),
                        trade_id: state.trade_id(),
                        quantity: None,
                    };
                    actions.add(market_id, Action::MarketClose(close_cmd));

                    // Open a Long
                    self.trade_counter += 1;
                    let open_cmd = OpenCmd {
                        agent_id,
                        trade_id: TradeId(self.trade_counter),
                        trade_type: TradeType::Long,
                        quantity: Quantity(1.0),
                        entry_price: None,
                        stop_loss: None,
                        take_profit: None,
                    };
                    actions.add(market_id, Action::Open(open_cmd));
                }
            } else {
                // Flat -> Open Long
                self.trade_counter += 1;
                let open_cmd = OpenCmd {
                    agent_id,
                    trade_id: TradeId(self.trade_counter),
                    trade_type: TradeType::Long,
                    quantity: Quantity(1.0),
                    entry_price: None,
                    stop_loss: None,
                    take_profit: None,
                };
                actions.add(market_id, Action::Open(open_cmd));
            }
        } else if fast < slow {
            // Bearish Trend
            if let Some((_, state)) = active_trade {
                if state.trade_type() == &TradeType::Long {
                    // Close the Long
                    let close_cmd = MarketCloseCmd {
                        agent_id: agent_id.clone(),
                        trade_id: state.trade_id(),
                        quantity: None,
                    };
                    actions.add(market_id, Action::MarketClose(close_cmd));

                    // Open a Short
                    self.trade_counter += 1;
                    let open_cmd = OpenCmd {
                        agent_id,
                        trade_id: TradeId(self.trade_counter),
                        trade_type: TradeType::Short,
                        quantity: Quantity(1.0),
                        entry_price: None,
                        stop_loss: None,
                        take_profit: None,
                    };
                    actions.add(market_id, Action::Open(open_cmd));
                }
            } else {
                // Flat -> Open Short
                self.trade_counter += 1;
                let open_cmd = OpenCmd {
                    agent_id,
                    trade_id: TradeId(self.trade_counter),
                    trade_type: TradeType::Short,
                    quantity: Quantity(1.0),
                    entry_price: None,
                    stop_loss: None,
                    take_profit: None,
                };
                actions.add(market_id, Action::Open(open_cmd));
            }
        }

        Ok(actions)
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
            fast_period: GridAxis::new("10", "30", "5")?,
            slow_period: GridAxis::new("40", "60", "5")?,
        })
    }

    pub fn build(self) -> (usize, Vec<(usize, DemoAgent)>) {
        let fasts = self.fast_period.generate();
        let slows = self.slow_period.generate();

        // 1. Eagerly collect valid combinations into a flat Vector
        let valid_args = iproduct!(fasts, slows)
            // Example filter: Fast must be less than Slow
            .filter(|(f, s)| f < s)
            .collect::<Vec<_>>();

        let total_combinations = valid_args.len();
        let ohlcv_id = self.ohlcv_id;

        // 2. Map to Agent instances
        let agents = valid_args
            .into_iter()
            .enumerate()
            .map(|(uid, (fast, slow))| (uid, DemoAgent::new(ohlcv_id, fast as u16, slow as u16)))
            .collect::<Vec<_>>();

        (total_combinations, agents)
    }
}
