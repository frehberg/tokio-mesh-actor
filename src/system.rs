pub use tokio_util::sync::CancellationToken;

use crate::{bus::{EventBus, EventReceiver}};
use crate::group::ActorGroup;

/// Events that this actor system will send
pub trait SystemEvent: Clone + Send + Sync + 'static {}

#[derive(Clone)]
pub struct ActorSystem<E: SystemEvent> {
    bus: EventBus<E>,
}

impl<E: SystemEvent> ActorSystem<E> {
    /// Publish an event on the actor system's event bus. These events can be
    /// received by other actors in the same actor system.
    pub fn publish(&self, event: E) {
        self.bus.send(event).unwrap_or_else(|error| {
            log::warn!(
                "No listeners active on event bus. Dropping event: {:?}",
                &error.to_string(),
            );
            0
        });
    }

    /// Subscribe to events of this actor system.
    pub fn events(&self) -> EventReceiver<E> {
        self.bus.subscribe()
    }

    /// Create a new actor system on which you can create actors.
    pub fn new(bus: EventBus<E>) -> Self {
        ActorSystem { bus }
    }

    pub fn create_group(&self) -> ActorGroup<E> {
        let cancel_token = CancellationToken::new();
        let system = self.clone();

        ActorGroup::new(cancel_token, system)
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    enum TestSystemEvent {
        None
    }

    impl SystemEvent for TestSystemEvent {}

    #[tokio::test]
    async fn create_system() {
        let bus = EventBus::new(200);
        let system: ActorSystem<TestSystemEvent> = ActorSystem::new(bus);
        let events = system.events();
        system.publish(TestSystemEvent::None);
        assert_eq!(1, events.len());
    }
}
