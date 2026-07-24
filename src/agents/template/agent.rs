use std::sync::Arc;

use chapaty::prelude::*;
use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TemplateAgent {
    #[serde(skip)]
    ohlcv_id: OhlcvId,

    volmalen: usize,
    volmult: f64,
    sl_pct: f64,
    tp_crv: f64,
    trade_qty: f64,

    #[serde(skip)]
    sma: StreamingSma,
    #[serde(skip)]
    current_volma: Option<f64>,
    #[serde(skip)]
    trade_counter: i64,
    #[serde(skip)]
    last_processed_ts: Option<DateTime<Utc>>,
    #[serde(skip)]
    agent_id: AgentIdentifier, // Pre-computed to save allocations
}

impl TemplateAgent {
    /// Creates a new agent utilizing the defaults defined in the specification.
    pub fn new(ohlcv_id: OhlcvId) -> Self {
        let default_volmalen = 20;
        Self {
            ohlcv_id,
            volmalen: default_volmalen,
            volmult: 2.0,
            sl_pct: 0.75,
            tp_crv: 2.0,
            trade_qty: 1.0,
            sma: StreamingSma::new(SmaWindow(default_volmalen as u16)),
            current_volma: None,
            trade_counter: 0,
            last_processed_ts: None,
            agent_id: AgentIdentifier::Named(Arc::new("Demo2Agent".to_string())),
        }
    }

    pub fn with_volmalen(self, volmalen: usize) -> Self {
        Self {
            sma: StreamingSma::new(SmaWindow(volmalen as u16)),
            volmalen,
            ..self
        }
    }

    pub fn with_volmult(self, volmult: f64) -> Self {
        Self { volmult, ..self }
    }

    pub fn with_sl_pct(self, sl_pct: f64) -> Self {
        Self { sl_pct, ..self }
    }

    pub fn with_tp_crv(self, tp_crv: f64) -> Self {
        Self { tp_crv, ..self }
    }

    pub fn with_trade_qty(self, trade_qty: f64) -> Self {
        Self { trade_qty, ..self }
    }
}

impl Agent for TemplateAgent {
    fn identifier(&self) -> AgentIdentifier {
        self.agent_id.clone()
    }

    fn reset(&mut self) {
        self.sma.reset();
        self.current_volma = None;
        self.trade_counter = 0;
        self.last_processed_ts = None;
    }

    fn act(&mut self, obs: Observation) -> ChapatyResult<Actions> {
        let market_view = &obs.market_view;

        // 1. Safe fetch of current price for entry/stops
        let current_price = match market_view.try_resolved_close_price(self.ohlcv_id.symbol) {
            Ok(price) => price.0,
            Err(_) => return Ok(Actions::no_op()),
        };

        // 2. Fetch the latest candle safely
        let Some(candle) = market_view.ohlcv().last_event(&self.ohlcv_id) else {
            return Ok(Actions::no_op());
        };

        // 3. Update Internal State (Idempotency check)
        let range = (candle.close.0 - candle.open.0).abs();
        if self.last_processed_ts != Some(candle.close_timestamp) {
            self.current_volma = self.sma.update(range);
            self.last_processed_ts = Some(candle.close_timestamp);
        }

        // 4. Check Signal Validity
        let Some(volma) = self.current_volma else {
            return Ok(Actions::no_op()); // SMA is still warming up
        };

        // 5. Breakout Logic
        let is_breakout = range > (volma * self.volmult);
        if !is_breakout {
            return Ok(Actions::no_op());
        }

        let signal_dir = match candle.direction() {
            CandleDirection::Bullish => TradeKind::Long,
            CandleDirection::Bearish => TradeKind::Short,
            CandleDirection::Doji => return Ok(Actions::no_op()),
        };

        // 6. Manage Positions (Zero-allocation using peekable)
        let mut active_trades = obs
            .states
            .iter_live()
            .filter(|state| state.agent_id() == &self.agent_id)
            .peekable();

        let market_id: MarketId = self.ohlcv_id.into();
        let mut actions = Actions::new();

        let is_counter_signal = active_trades
            .peek()
            .is_some_and(|first| *first.trade_type() != signal_dir);

        if is_counter_signal {
            // Counter signal: close all, don't open new
            for state in active_trades {
                actions.add(
                    market_id,
                    Action::MarketClose(MarketCloseCmd {
                        agent_id: self.identifier(),
                        trade_id: state.trade_id(),
                        quantity: None,
                    }),
                );
            }
        } else {
            // Covers both "No active trades" AND "Pyramiding (same direction)"
            actions.add(market_id, self.open(signal_dir, range, current_price));
        }

        Ok(actions)
    }
}

impl TemplateAgent {
    fn open(&mut self, trade_type: TradeKind, range: f64, current_price: f64) -> Action {
        self.trade_counter += 1;

        let sl_dist = range * self.sl_pct;
        let tp_dist = sl_dist * self.tp_crv;

        let (sl, tp) = if trade_type == TradeKind::Long {
            (
                self.ohlcv_id
                    .symbol
                    .normalize_price(current_price - sl_dist),
                self.ohlcv_id
                    .symbol
                    .normalize_price(current_price + tp_dist),
            )
        } else {
            (
                self.ohlcv_id
                    .symbol
                    .normalize_price(current_price + sl_dist),
                self.ohlcv_id
                    .symbol
                    .normalize_price(current_price - tp_dist),
            )
        };

        Action::Open(OpenCmd {
            agent_id: self.identifier(),
            trade_id: TradeId(self.trade_counter),
            trade_type,
            quantity: Quantity(self.trade_qty),
            entry_price: None,
            stop_loss: Some(Price(sl)),
            take_profit: Some(Price(tp)),
        })
    }
}

pub struct TemplateAgentGrid {
    ohlcv_id: OhlcvId,
    volmalen: GridAxis,
    volmult: GridAxis,
    sl_pct: GridAxis,
    tp_crv: GridAxis,
}

impl TemplateAgentGrid {
    pub fn baseline(ohlcv_id: OhlcvId) -> ChapatyResult<Self> {
        Ok(Self {
            ohlcv_id,
            volmalen: GridAxis::new("10", "30", "1")?,
            volmult: GridAxis::new("1.5", "2.5", "0.1")?,
            sl_pct: GridAxis::new("0.5", "1.0", "0.1")?,
            tp_crv: GridAxis::new("1.5", "3.0", "0.1")?,
        })
    }

    pub fn build(self) -> Vec<(usize, TemplateAgent)> {
        let lens = self.volmalen.generate();
        let mults = self.volmult.generate();
        let sls = self.sl_pct.generate();
        let tps = self.tp_crv.generate();

        let ohlcv_id = self.ohlcv_id;

        iproduct!(lens, mults, sls, tps)
            .enumerate()
            .map(|(uid, (len, mult, sl, tp))| {
                (
                    uid,
                    TemplateAgent::new(ohlcv_id)
                        .with_volmalen(len as usize)
                        .with_volmult(mult)
                        .with_sl_pct(sl)
                        .with_tp_crv(tp)
                        .with_trade_qty(1.0),
                )
            })
            .collect()
    }
}
