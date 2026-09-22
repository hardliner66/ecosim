use std::fmt::Debug;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SimulationError<E, T> {
    #[error("Event error: {0:?}")]
    Event(E),
    #[error("Tick error: {0:?}")]
    Tick(T),
}

pub trait Simulation {
    type Event: Clone;
    type EventError: Debug;
    type TickError: Debug;
    fn handle(&mut self, event: &Self::Event) -> Result<(), Self::EventError>;
    fn tick(&mut self) -> Result<(), Self::TickError>;

    fn simulate_with_fn(
        &mut self,
        steps: usize,
        next_event: impl Fn(&mut Self, usize) -> Vec<Self::Event>,
    ) -> Result<(), SimulationError<Self::EventError, Self::TickError>> {
        for i in 0..steps {
            let events = next_event(self, i);
            for event in events {
                self.handle(&event).map_err(SimulationError::Event)?;
            }
            self.tick().map_err(SimulationError::Tick)?;
        }
        Ok(())
    }
    fn simulate_with_events(
        &mut self,
        steps: usize,
        events: &[Self::Event],
    ) -> Result<(), SimulationError<Self::EventError, Self::TickError>> {
        self.simulate_with_fn(steps, |_, _| events.to_vec())
    }
    fn simulate(
        &mut self,
        steps: usize,
    ) -> Result<(), SimulationError<Self::EventError, Self::TickError>> {
        self.simulate_with_events(steps, &[])
    }
}

pub trait PredictableSimulation: Simulation {
    type State: ToOwned;
    fn predict_with_fn(
        &mut self,
        steps: usize,
        update: impl Fn(&mut Self::State, usize) -> Vec<Self::Event>,
    ) -> Result<Self::State, SimulationError<Self::EventError, Self::TickError>>;

    fn predict_with_events(
        &mut self,
        steps: usize,
        events: &[Self::Event],
    ) -> Result<Self::State, SimulationError<Self::EventError, Self::TickError>>;

    fn predict(
        &mut self,
        steps: usize,
    ) -> Result<Self::State, SimulationError<Self::EventError, Self::TickError>>;
}

impl<PE: Simulation + Clone> ReversibleSimulation for PE {
    fn revert_with_fn(
        &mut self,
        steps: usize,
        update: impl Fn(&mut Self, usize) -> Vec<Self::Event>,
    ) {
        let mut clone = self.clone();
        clone.simulate_with_fn(steps, update).unwrap();
    }

    fn revert_with_events(&mut self, steps: usize, events: &[Self::Event]) {
        self.revert_with_fn(steps, |_, _| events.to_vec());
    }

    fn revert(&mut self, steps: usize) {
        self.revert_with_events(steps, &[]);
    }
}

pub trait ReversibleSimulation: Simulation {
    fn revert_with_fn(
        &mut self,
        steps: usize,
        update: impl Fn(&mut Self, usize) -> Vec<Self::Event>,
    );
    fn revert_with_events(&mut self, steps: usize, events: &[Self::Event]);
    fn revert(&mut self, steps: usize);
}

impl<PE: ReversibleSimulation + ToOwned<Owned = PE>> PredictableSimulation for PE {
    type State = Self;
    fn predict_with_fn(
        &mut self,
        steps: usize,
        update: impl Fn(&mut Self, usize) -> Vec<Self::Event>,
    ) -> Result<Self::State, SimulationError<Self::EventError, Self::TickError>> {
        self.simulate_with_fn(steps, &update)?;
        let simulated = self.to_owned();
        self.revert_with_fn(steps, &update);
        Ok(simulated)
    }

    fn predict_with_events(
        &mut self,
        steps: usize,
        events: &[Self::Event],
    ) -> Result<Self, SimulationError<Self::EventError, Self::TickError>> {
        self.predict_with_fn(steps, |_, _| events.to_vec())
    }

    fn predict(
        &mut self,
        steps: usize,
    ) -> Result<
        Self,
        SimulationError<<Self as Simulation>::EventError, <Self as Simulation>::TickError>,
    > {
        self.predict_with_events(steps, &[])
    }
}

struct SimpleEconomy {
    pub population: usize,
}

impl Simulation for SimpleEconomy {
    type Event = ();
    type EventError = ();
    type TickError = ();

    fn handle(&mut self, _event: &Self::Event) -> Result<(), Self::EventError> {
        // No-op for this simple economy
        Ok(())
    }

    fn tick(&mut self) -> Result<(), Self::TickError> {
        self.population += 1; // Simple growth model
        Ok(())
    }
}

#[derive(Clone)]
struct SimplePredictableEconomy {
    pub population: usize,
}

impl Simulation for SimplePredictableEconomy {
    type Event = ();
    type EventError = ();
    type TickError = ();

    fn handle(&mut self, _event: &Self::Event) -> Result<(), Self::EventError> {
        // No-op for this simple economy
        Ok(())
    }

    fn tick(&mut self) -> Result<(), Self::TickError> {
        self.population += 1; // Simple growth model
        Ok(())
    }
}

#[derive(Clone)]
struct EventEconomy {
    pub population: usize,
    pub births: usize,
    pub deaths: usize,
}

#[derive(Clone)]
enum EconomyEvent {
    Births(usize),
    Deaths(usize),
}

#[derive(Debug)]
enum CustomSimulationError {
    PopulationUnderflow,
}

impl Simulation for EventEconomy {
    type Event = EconomyEvent;
    type EventError = ();
    type TickError = CustomSimulationError;
    fn handle(&mut self, event: &Self::Event) -> Result<(), Self::EventError> {
        match event {
            EconomyEvent::Births(n) => self.births += n,
            EconomyEvent::Deaths(n) => self.deaths += n,
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), Self::TickError> {
        self.population = (self.population + self.births)
            .checked_sub(self.deaths)
            .ok_or(CustomSimulationError::PopulationUnderflow)?;
        self.births = 0;
        self.deaths = 0;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut economy = SimpleEconomy { population: 100 };
    economy.simulate_with_events(10, &[])?;
    println!(
        "Simulated population after 10 steps: {}",
        economy.population
    );

    let mut economy = SimplePredictableEconomy { population: 100 };
    println!(
        "Predicted population after 10 steps: {}",
        economy.predict(10)?.population
    );
    println!("Original population: {}", economy.population);

    let mut economy = EventEconomy {
        population: 100,
        births: 0,
        deaths: 0,
    };
    economy.simulate_with_events(10, &[EconomyEvent::Births(5), EconomyEvent::Deaths(3)])?;
    println!(
        "Event economy population after 10 steps: {}",
        economy.population
    );
    economy.simulate_with_events(10, &[EconomyEvent::Births(3), EconomyEvent::Deaths(16)])?;
    println!(
        "Event economy population after 10 steps: {}",
        economy.population
    );
    Ok(())
}
