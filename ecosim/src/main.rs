use std::{collections::HashMap, ops::RangeInclusive};

use ecosim_traits::Simulation;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Commodity {
    Corn,
}

#[derive(Debug, Clone)]
struct Order {
    commodity: Commodity,
    quantity: u32,
    price_range: RangeInclusive<u32>,
}

#[derive(Debug, Clone)]
enum MarketEvent {
    Buy(Order),
    Sell(Order),
}

#[derive(Debug, Clone)]
struct SimpleEconomy {
    market_prices: HashMap<Commodity, u32>,
    buy_orders: Vec<Order>,
    sell_orders: Vec<Order>,
}

impl Simulation for SimpleEconomy {
    type Event = MarketEvent;
    type EventError = anyhow::Error;
    type TickError = anyhow::Error;

    fn handle(&mut self, event: &Self::Event) -> Result<(), Self::EventError> {
        match event {
            MarketEvent::Buy(order) => {
                self.buy_orders.push(order.clone());
            }
            MarketEvent::Sell(order) => {
                self.sell_orders.push(order.clone());
            }
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), Self::TickError> {
        // Simple matching logic: match buy and sell orders for the same commodity
        let mut i = 0;
        while i < self.buy_orders.len() {
            let mut j = 0;
            let mut matched = false;
            while j < self.sell_orders.len() {
                if self.buy_orders[i].commodity == self.sell_orders[j].commodity {
                    let buy_order = &self.buy_orders[i];
                    let sell_order = &self.sell_orders[j];
                    if buy_order.price_range.start() <= sell_order.price_range.end() {
                        let quantity = buy_order.quantity.min(sell_order.quantity);
                        *self
                            .market_prices
                            .entry(buy_order.commodity.clone())
                            .or_insert(0) =
                            (buy_order.price_range.start() + sell_order.price_range.end()) / 2;
                        self.buy_orders[i].quantity -= quantity;
                        self.sell_orders[j].quantity -= quantity;
                        if self.sell_orders[j].quantity == 0 {
                            self.sell_orders.remove(j);
                        } else {
                            j += 1;
                        }
                        if self.buy_orders[i].quantity == 0 {
                            self.buy_orders.remove(i);
                            matched = true;
                            break;
                        }
                    } else {
                        j += 1;
                    }
                } else {
                    j += 1;
                }
            }
            if !matched {
                i += 1;
            }
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut economy = SimpleEconomy {
        market_prices: HashMap::new(),
        buy_orders: Vec::new(),
        sell_orders: Vec::new(),
    };
    economy.market_prices.insert(Commodity::Corn, 5);
    println!("Before: {:?}", economy.market_prices);
    economy.handle(&MarketEvent::Buy(Order {
        commodity: Commodity::Corn,
        quantity: 10,
        price_range: 5..=10,
    }))?;
    economy.handle(&MarketEvent::Buy(Order {
        commodity: Commodity::Corn,
        quantity: 5,
        price_range: 3..=8,
    }))?;
    economy.handle(&MarketEvent::Sell(Order {
        commodity: Commodity::Corn,
        quantity: 8,
        price_range: 4..=9,
    }))?;
    economy.handle(&MarketEvent::Sell(Order {
        commodity: Commodity::Corn,
        quantity: 2,
        price_range: 6..=12,
    }))?;
    economy.tick()?;
    println!("After: {:?}", economy.market_prices);
    Ok(())
}
