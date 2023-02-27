use async_trait::async_trait;
use tokio::time::Duration;

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
    async fn pre_start(&mut self, _ctx: &mut ActorContext<E>) -> Result<(), ActorError> {
        Ok(())
    }

    /// request
    fn cyclic_interval() -> Option<Duration> {
        None
    }

    /// Override this function if you like to perform initialization of the actor
    async fn cyclic_action(&mut self, _ctx: &mut ActorContext<E>) -> Result<(), ActorError> {
        Ok(())
    }

    /// Override this function if you like to perform initialization of the actor
    async fn event(&mut self, _event: &E, _ctx: &mut ActorContext<E>) -> Result<(), ActorError> {
        Ok(())
    }

    /// Override this function if you like to perform work when the actor is stopped
    async fn post_stop(&mut self, _ctx: &mut ActorContext<E>) {}
}

/// Defines what the actor does with a message.
#[async_trait]
pub trait Handler<E: SystemEvent, M: Message>: Actor<E> {
    async fn handle(&mut self, msg: M, ctx: &mut ActorContext<E>) -> M::Response;
}
