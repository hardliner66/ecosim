use std::collections::{HashMap, HashSet};

use clap::Parser;
use ecosim_traits::Simulation;
use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};
use thiserror::Error;
use tracing::{debug, info};
use uuid::Uuid;

const FARMER_COUNT_RANGE: std::ops::RangeInclusive<u32> = 2..=5;
const BAKER_COUNT_RANGE: std::ops::RangeInclusive<u32> = 2..=5;
const FARMER_STOCK_RANGE: std::ops::RangeInclusive<u32> = 60..=140;
const FARMER_SELL_PRICE_RANGE: std::ops::RangeInclusive<u32> = 6..=10;
const BAKER_CASH_RANGE: std::ops::RangeInclusive<u64> = 800..=1_200;
const BAKER_BUY_PRICE_RANGE: std::ops::RangeInclusive<u32> = 10..=14;
const ORDER_QUANTITY_RANGE: std::ops::RangeInclusive<u32> = 1..=8;

#[derive(Parser, Debug)]
struct Cli {
    #[arg(short, long, default_value_t = 20)]
    steps: usize,
    #[arg(long)]
    seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Commodity {
    Corn,
}

#[derive(Debug, Clone)]
struct Order {
    participant_id: Uuid,
    commodity: Commodity,
    quantity: u32,
    /// max price the participant is willing to pay for buy orders
    /// or min price the participant will accept for sell orders
    price: u32,
}

#[derive(Debug, Clone)]
enum MarketEvent {
    Buy(Order),
    Sell(Order),
    Join {
        participant_id: Uuid,
        participant: Participant,
    },
    Leave(Uuid),
}

/// Hard errors only: malformed input that should never happen in a well-formed
/// simulation. Ordinary supply/demand shortfalls (e.g. not enough stock or cash
/// to fully satisfy an order) are resolved during `tick`, not treated as errors.
#[derive(Error, Debug)]
enum MarketError {
    #[error("Unknown participant with ID {0}")]
    UnknownParticipant(Uuid),
    #[error("Participant with ID {0} already joined")]
    ParticipantAlreadyExists(Uuid),
    #[error("Order has zero quantity")]
    ZeroQuantity,
}

#[derive(Debug, Clone)]
struct Participant {
    name: String,
    cash: u64,
    stock: HashMap<Commodity, u32>,
    expected_buy_prices: HashMap<Commodity, u32>,
    expected_sell_prices: HashMap<Commodity, u32>,
}

struct Market {
    rng: Box<dyn Rng>,
    participants: HashMap<Uuid, Participant>,
    buy_orders: Vec<Order>,
    sell_orders: Vec<Order>,
    leavers: HashSet<Uuid>,
}

impl Market {
    /// Generates buy/sell orders for each participant based on their stock levels
    /// and price expectations. Buyers bid when low on stock, sellers offer when flush.
    /// Order quantities are randomized via `rng` for more varied market activity.
    fn generate_orders(&mut self, _step: usize) -> Vec<MarketEvent> {
        const LOW_STOCK_THRESHOLD: u32 = 10;
        const HIGH_STOCK_THRESHOLD: u32 = 20;

        let mut events = Vec::new();
        for (&participant_id, participant) in &self.participants {
            for commodity in [Commodity::Corn] {
                let stock = *participant.stock.get(&commodity).unwrap_or(&0);

                if stock < LOW_STOCK_THRESHOLD {
                    if let Some(&price) = participant.expected_buy_prices.get(&commodity) {
                        events.push(MarketEvent::Buy(Order {
                            participant_id,
                            commodity: commodity.clone(),
                            quantity: self.rng.random_range(ORDER_QUANTITY_RANGE),
                            price,
                        }));
                    }
                }

                if stock > HIGH_STOCK_THRESHOLD {
                    if let Some(&price) = participant.expected_sell_prices.get(&commodity) {
                        events.push(MarketEvent::Sell(Order {
                            participant_id,
                            commodity,
                            quantity: self.rng.random_range(ORDER_QUANTITY_RANGE),
                            price,
                        }));
                    }
                }
            }
        }
        events
    }

    /// Matches buy/sell orders for a single commodity, settling trades at the
    /// midpoint between the highest bid and lowest ask (classic price discovery).
    /// Trade quantities are capped by what the buyer can actually afford and what
    /// the seller actually has in stock at the moment of the trade - wanting more
    /// than is available is normal and simply results in a partial (or no) fill.
    fn clear_commodity(&mut self, commodity: &Commodity) {
        let mut buys: Vec<Order> = self
            .buy_orders
            .iter()
            .filter(|o| &o.commodity == commodity)
            .cloned()
            .collect();
        let mut sells: Vec<Order> = self
            .sell_orders
            .iter()
            .filter(|o| &o.commodity == commodity)
            .cloned()
            .collect();

        // highest bidders and lowest askers trade first
        buys.sort_by(|a, b| b.price.cmp(&a.price));
        sells.sort_by(|a, b| a.price.cmp(&b.price));

        let buy_requested: Vec<u32> = buys.iter().map(|o| o.quantity).collect();
        let sell_requested: Vec<u32> = sells.iter().map(|o| o.quantity).collect();
        let mut buy_filled = vec![0u32; buys.len()];
        let mut sell_filled = vec![0u32; sells.len()];

        let mut bi = 0;
        let mut si = 0;

        while bi < buys.len() && si < sells.len() {
            if buys[bi].quantity == 0 {
                bi += 1;
                continue;
            }
            if sells[si].quantity == 0 {
                si += 1;
                continue;
            }
            if buys[bi].price < sells[si].price {
                break;
            }

            let trade_price = (buys[bi].price as u64 + sells[si].price as u64) / 2;
            let buyer_id = buys[bi].participant_id;
            let seller_id = sells[si].participant_id;

            let affordable = self
                .participants
                .get(&buyer_id)
                .map(|p| {
                    if trade_price == 0 {
                        u32::MAX
                    } else {
                        (p.cash / trade_price) as u32
                    }
                })
                .unwrap_or(0);
            if affordable == 0 {
                // buyer can't afford this price at all; their order goes unfilled
                buys[bi].quantity = 0;
                bi += 1;
                continue;
            }

            let available_stock = self
                .participants
                .get(&seller_id)
                .map(|p| *p.stock.get(commodity).unwrap_or(&0))
                .unwrap_or(0);
            if available_stock == 0 {
                // seller has nothing to actually deliver; their order goes unfilled
                sells[si].quantity = 0;
                si += 1;
                continue;
            }

            let trade_qty = buys[bi]
                .quantity
                .min(sells[si].quantity)
                .min(affordable)
                .min(available_stock);
            let total = trade_price * trade_qty as u64;

            if let Some(buyer) = self.participants.get_mut(&buyer_id) {
                buyer.cash -= total;
                *buyer.stock.entry(commodity.clone()).or_insert(0) += trade_qty;
            }
            if let Some(seller) = self.participants.get_mut(&seller_id) {
                seller.cash += total;
                if let Some(s) = seller.stock.get_mut(commodity) {
                    *s -= trade_qty;
                }
            }

            debug!(
                ?commodity,
                ?buyer_id,
                ?seller_id,
                trade_qty,
                trade_price,
                "Trade executed"
            );

            buys[bi].quantity -= trade_qty;
            sells[si].quantity -= trade_qty;
            buy_filled[bi] += trade_qty;
            sell_filled[si] += trade_qty;

            if buys[bi].quantity == 0 {
                bi += 1;
            }
            if sells[si].quantity == 0 {
                si += 1;
            }
        }

        for (i, order) in buys.iter().enumerate() {
            self.adjust_buy_expectation(
                order.participant_id,
                commodity,
                buy_filled[i],
                buy_requested[i],
            );
        }
        for (i, order) in sells.iter().enumerate() {
            self.adjust_sell_expectation(
                order.participant_id,
                commodity,
                sell_filled[i],
                sell_requested[i],
            );
        }

        // orders only ever live for the day they were placed on
        self.buy_orders.retain(|o| &o.commodity != commodity);
        self.sell_orders.retain(|o| &o.commodity != commodity);
    }

    /// Adapts a buyer's expectation for tomorrow: bid higher after an unmet order,
    /// bid lower after an easy fill.
    fn adjust_buy_expectation(
        &mut self,
        participant_id: Uuid,
        commodity: &Commodity,
        filled: u32,
        requested: u32,
    ) {
        let Some(price) = self
            .participants
            .get_mut(&participant_id)
            .and_then(|p| p.expected_buy_prices.get_mut(commodity))
        else {
            return;
        };
        if filled < requested {
            *price = price.saturating_add((*price / 10).max(1));
            debug!(
                ?participant_id,
                ?commodity,
                new_price = *price,
                "Raised expected buy price after unmet demand"
            );
        } else if requested > 0 {
            *price = price.saturating_sub((*price / 20).max(1)).max(1);
            debug!(
                ?participant_id,
                ?commodity,
                new_price = *price,
                "Lowered expected buy price after easy fill"
            );
        }
    }

    /// Adapts a seller's expectation for tomorrow: ask lower after an unmet order,
    /// ask higher after an easy sale.
    fn adjust_sell_expectation(
        &mut self,
        participant_id: Uuid,
        commodity: &Commodity,
        filled: u32,
        requested: u32,
    ) {
        let Some(price) = self
            .participants
            .get_mut(&participant_id)
            .and_then(|p| p.expected_sell_prices.get_mut(commodity))
        else {
            return;
        };
        if filled < requested {
            *price = price.saturating_sub((*price / 10).max(1)).max(1);
            debug!(
                ?participant_id,
                ?commodity,
                new_price = *price,
                "Lowered expected sell price after unmet supply"
            );
        } else if requested > 0 {
            *price = price.saturating_add((*price / 20).max(1));
            debug!(
                ?participant_id,
                ?commodity,
                new_price = *price,
                "Raised expected sell price after easy sale"
            );
        }
    }

    /// Estimates the current market price per commodity as the midpoint between
    /// the average expected buy price and the average expected sell price across
    /// all participants.
    fn get_market_prices(&self) -> HashMap<Commodity, u32> {
        let mut buy_totals: HashMap<Commodity, (u64, u32)> = HashMap::new();
        let mut sell_totals: HashMap<Commodity, (u64, u32)> = HashMap::new();

        for participant in self.participants.values() {
            for (commodity, &price) in &participant.expected_buy_prices {
                let entry = buy_totals.entry(commodity.clone()).or_insert((0, 0));
                entry.0 += price as u64;
                entry.1 += 1;
            }
            for (commodity, &price) in &participant.expected_sell_prices {
                let entry = sell_totals.entry(commodity.clone()).or_insert((0, 0));
                entry.0 += price as u64;
                entry.1 += 1;
            }
        }

        let commodities: HashSet<Commodity> = buy_totals
            .keys()
            .chain(sell_totals.keys())
            .cloned()
            .collect();

        commodities
            .into_iter()
            .filter_map(|commodity| {
                let avg_buy = buy_totals
                    .get(&commodity)
                    .map(|(sum, count)| sum / *count as u64);
                let avg_sell = sell_totals
                    .get(&commodity)
                    .map(|(sum, count)| sum / *count as u64);
                let price = match (avg_buy, avg_sell) {
                    (Some(b), Some(s)) => (b + s) / 2,
                    (Some(b), None) => b,
                    (None, Some(s)) => s,
                    (None, None) => return None,
                };
                Some((commodity, price as u32))
            })
            .collect()
    }
}

impl Simulation for Market {
    type Event = MarketEvent;
    type EventError = MarketError;
    type TickError = ();

    /// Just ingests an order/join/leave for today; matching and settlement happens
    /// in `tick`. Only malformed input (unknown participant, duplicate join, zero
    /// quantity) is rejected here.
    fn handle(&mut self, event: &Self::Event) -> Result<(), Self::EventError> {
        debug!("Handling event: {:?}", event);
        match event {
            MarketEvent::Buy(order) | MarketEvent::Sell(order) => {
                if order.quantity == 0 {
                    return Err(MarketError::ZeroQuantity);
                }
                if !self.participants.contains_key(&order.participant_id) {
                    return Err(MarketError::UnknownParticipant(order.participant_id));
                }
                match event {
                    MarketEvent::Buy(order) => self.buy_orders.push(order.clone()),
                    MarketEvent::Sell(order) => self.sell_orders.push(order.clone()),
                    _ => unreachable!(),
                }
            }
            MarketEvent::Join {
                participant_id,
                participant,
            } => {
                if self.participants.contains_key(participant_id) {
                    return Err(MarketError::ParticipantAlreadyExists(*participant_id));
                }
                self.participants
                    .insert(*participant_id, participant.clone());
            }
            MarketEvent::Leave(participant_id) => {
                if !self.participants.contains_key(participant_id) {
                    return Err(MarketError::UnknownParticipant(*participant_id));
                }
                self.leavers.insert(*participant_id);
            }
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), Self::TickError> {
        let commodities: HashSet<Commodity> = self
            .buy_orders
            .iter()
            .chain(self.sell_orders.iter())
            .map(|o| o.commodity.clone())
            .collect();

        debug!(
            buy_orders = self.buy_orders.len(),
            sell_orders = self.sell_orders.len(),
            "Ticking market with {} commodities in play",
            commodities.len()
        );
        for commodity in commodities {
            self.clear_commodity(&commodity);
        }
        for leaver in self.leavers.drain() {
            self.participants.remove(&leaver);
        }
        Ok(())
    }
}

fn print_market_state(market: &Market) {
    let mut participants: Vec<_> = market.participants.iter().collect();
    participants.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
    for (_id, participant) in participants {
        info!(
            "{} - cash: {}, stock: {:?}, expected buy: {:?}, expected sell: {:?}",
            participant.name,
            participant.cash,
            participant.stock,
            participant.expected_buy_prices,
            participant.expected_sell_prices
        );
    }
    info!("Market prices: {:?}", market.get_market_prices());
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .without_time()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let Cli { seed, steps } = Cli::parse();

    let seed = seed.unwrap_or_else(|| rand::rng().random());
    info!("Seeding RNG (pass '--seed {seed}' to reproduce this run)");
    let mut rng = StdRng::seed_from_u64(seed);

    let mut participants = HashMap::new();

    let farmer_count = rng.random_range(FARMER_COUNT_RANGE);
    let farmer_ids: Vec<Uuid> = (0..farmer_count)
        .map(|i| {
            let id = Uuid::new_v4();
            participants.insert(
                id,
                Participant {
                    name: format!("Farmer {}", i + 1),
                    cash: 0,
                    stock: HashMap::from([(Commodity::Corn, rng.random_range(FARMER_STOCK_RANGE))]),
                    expected_buy_prices: HashMap::new(),
                    expected_sell_prices: HashMap::from([(
                        Commodity::Corn,
                        rng.random_range(FARMER_SELL_PRICE_RANGE),
                    )]),
                },
            );
            id
        })
        .collect();

    let baker_count = rng.random_range(BAKER_COUNT_RANGE);
    for i in 0..baker_count {
        let id = Uuid::new_v4();
        participants.insert(
            id,
            Participant {
                name: format!("Baker {}", i + 1),
                cash: rng.random_range(BAKER_CASH_RANGE),
                stock: HashMap::new(),
                expected_buy_prices: HashMap::from([(
                    Commodity::Corn,
                    rng.random_range(BAKER_BUY_PRICE_RANGE),
                )]),
                expected_sell_prices: HashMap::new(),
            },
        );
    }

    // used later to show the Join/Leave events in action
    let latecomer_id = Uuid::new_v4();
    let departing_farmer_id = farmer_ids[rng.random_range(0..farmer_ids.len())];

    let mut market = Market {
        rng: Box::new(rng),
        participants,
        buy_orders: Vec::new(),
        sell_orders: Vec::new(),
        leavers: HashSet::new(),
    };

    print_market_state(&market);

    info!("Starting market simulation");
    market
        .simulate_with_fn(steps, |m, step| {
            let mut events = m.generate_orders(step);
            if step == 5 {
                events.push(MarketEvent::Join {
                    participant_id: latecomer_id,
                    participant: Participant {
                        name: "Trader".into(),
                        cash: 500,
                        stock: HashMap::new(),
                        expected_buy_prices: HashMap::from([(Commodity::Corn, 9)]),
                        expected_sell_prices: HashMap::new(),
                    },
                });
            }
            if step == 15 {
                events.push(MarketEvent::Leave(departing_farmer_id));
            }
            events
        })
        .map_err(|e| anyhow::anyhow!("simulation failed: {e:?}"))?;
    info!("Simulation finished");

    print_market_state(&market);

    Ok(())
}
