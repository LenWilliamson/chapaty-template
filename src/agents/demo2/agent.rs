use chapaty::prelude::*;
use chrono::{DateTime, Utc};
use itertools::iproduct;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct BreakoutAgent {
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
}

impl BreakoutAgent {
    pub fn new(
        ohlcv_id: OhlcvId,
        volmalen: usize,
        volmult: f64,
        sl_pct: f64,
        tp_crv: f64,
        trade_qty: f64,
    ) -> Self {
        Self {
            ohlcv_id,
            volmalen,
            volmult,
            sl_pct,
            tp_crv,
            trade_qty,
            sma: StreamingSma::new(volmalen as u16),
            current_volma: None,
            trade_counter: 0,
            last_processed_ts: None,
        }
    }
}

impl Agent for BreakoutAgent {
    fn identifier(&self) -> AgentIdentifier {
        AgentIdentifier::Named(Arc::new("BreakoutAgent".to_string()))
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
        let current_price = match market_view.try_resolved_close_price(&self.ohlcv_id.symbol) {
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

        let signal_dir = if candle.close.0 > candle.open.0 {
            TradeType::Long
        } else {
            TradeType::Short
        };

        // 6. Manage Positions
        let agent_id = self.identifier();
        let active_trades: Vec<_> = obs
            .states
            .iter_live()
            .filter(|state| state.agent_id() == &agent_id)
            .collect();

        let market_id: MarketId = self.ohlcv_id.into();
        let mut actions = Actions::new();

        if active_trades.is_empty() {
            actions.add(market_id, self.open(signal_dir, range, current_price));
        } else {
            let current_dir = *active_trades[0].trade_type();

            if current_dir == signal_dir {
                // Pyramiding: same direction
                actions.add(market_id, self.open(signal_dir, range, current_price));
            } else {
                // Counter signal: close all, don't open new (cool-down achieved naturally)
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
            }
        }

        Ok(actions)
    }
}

impl BreakoutAgent {
    fn open(&mut self, trade_type: TradeType, range: f64, current_price: f64) -> Action {
        self.trade_counter += 1;

        let sl_dist = range * self.sl_pct;
        let tp_dist = sl_dist * self.tp_crv;

        let (sl, tp) = if trade_type == TradeType::Long {
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

pub struct BreakoutAgentGrid {
    ohlcv_id: OhlcvId,
    volmalen: GridAxis,
    volmult: GridAxis,
    sl_pct: GridAxis,
    tp_crv: GridAxis,
}

impl BreakoutAgentGrid {
    pub fn baseline(ohlcv_id: OhlcvId) -> ChapatyResult<Self> {
        Ok(Self {
            ohlcv_id,
            volmalen: GridAxis::new("10", "30", "1")?,
            volmult: GridAxis::new("1.5", "2.5", "0.1")?,
            sl_pct: GridAxis::new("0.5", "1.0", "0.1")?,
            tp_crv: GridAxis::new("1.5", "3.0", "0.1")?,
        })
    }

    pub fn build(self) -> Vec<(usize, BreakoutAgent)> {
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
                    BreakoutAgent::new(ohlcv_id, len as usize, mult, sl, tp, 1.0),
                )
            })
            .collect()
    }
}
