use ecosim_traits::{PredictableSimulation, Simulation};

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

    let economy = SimplePredictableEconomy { population: 100 };
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
    println!(
        "Error after simulating with excessive deaths: {}",
        economy
            .simulate_with_events(10, &[EconomyEvent::Births(3), EconomyEvent::Deaths(16)])
            .unwrap_err()
    );
    Ok(())
}
