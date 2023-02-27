use async_trait::async_trait;
use tokio::sync::oneshot;
use crate::{ActorContext,
    actor::{Handler, Message},
    system::SystemEvent,
};
use crate::actor::Actor;

#[async_trait]
pub trait MessageHandler<E: SystemEvent, A: Actor<E>>: Send + Sync {
    async fn handle(&mut self, actor: &mut A, ctx: &mut ActorContext<E>);
}

//
pub struct ActorMessage<M>
    where
        M: Message,
{
    payload: M,
    rsvp: Option<oneshot::Sender<M::Response>>,
}


impl<M> ActorMessage<M>
    where
        M: Message,
{
    pub fn new(msg: M, rsvp: Option<oneshot::Sender<M::Response>>) -> Self {
        ActorMessage {
            payload: msg,
            rsvp,
        }
    }

    pub fn get(self) -> M {
        self.payload
    }
}


#[async_trait]
impl<M, E, A> MessageHandler<E, A> for ActorMessage<M>
    where
        M: Message,
        E: SystemEvent,
        A: Handler<E, M>,
{
    // TODO: change to (self) consuming the ActorMessage
    async fn handle(&mut self, actor: &mut A, ctx: &mut ActorContext<E>) {
        let result = actor.handle(self.payload.clone(), ctx).await;

        if let Some(rsvp) = std::mem::replace(&mut self.rsvp, None) {
            rsvp.send(result).unwrap_or_else(|_failed| {
                log::error!("Failed to send back response!");
            })
        }
    }
}