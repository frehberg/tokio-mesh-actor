use async_trait::async_trait;

use crate::{ActorContext, ActorError};
use crate::system::SystemEvent;

/// Defines what an actor will receive as its message, and with what it should respond.
pub trait Message: Clone + Send + Sync + 'static {
    /// response an actor should give when it receives this message. If no response is
    /// required, use `()`.
    type Response: Send + Sync + 'static;
}

#[async_trait]
pub trait Actor<E: SystemEvent>: Send + Sync + 'static {
    /// Override this function if you like to perform initialization of the actor
    async fn pre_start(&mut self, _ctx: &mut ActorContext<E, Self>) -> Result<(), ActorError> {
        Ok(())
    }

    /// Override this function if you like to system events
    async fn event(&mut self, _event: &E, _ctx: &mut ActorContext<E, Self>) -> Result<(), ActorError> {
        Ok(())
    }

    /// Override this function if you like to perform work when the actor is stopped
    async fn post_stop(&mut self, _ctx: &mut ActorContext<E, Self>) {}
}

/// Defines what the actor does with a message.
#[async_trait]
pub trait Handler<E: SystemEvent, M: Message>: Actor<E> {
    async fn handle(&mut self, msg: M, ctx: &mut ActorContext<E, Self>) -> M::Response;
}
