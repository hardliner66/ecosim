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
    type EventError;
    type TickError;
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

pub trait PredictableSimulation: Simulation + Clone {
    fn predict_with_fn(
        &self,
        steps: usize,
        update: impl Fn(&mut Self, usize) -> Vec<Self::Event>,
    ) -> Result<Self, SimulationError<Self::EventError, Self::TickError>> {
        let mut simulated = self.clone();
        simulated.simulate_with_fn(steps, &update)?;
        Ok(simulated)
    }

    fn predict_with_events(
        &self,
        steps: usize,
        events: &[Self::Event],
    ) -> Result<Self, SimulationError<Self::EventError, Self::TickError>> {
        self.predict_with_fn(steps, |_, _| events.to_vec())
    }

    fn predict(
        &self,
        steps: usize,
    ) -> Result<
        Self,
        SimulationError<<Self as Simulation>::EventError, <Self as Simulation>::TickError>,
    > {
        self.predict_with_events(steps, &[])
    }
}

impl<PE: Simulation + Clone> PredictableSimulation for PE {}
