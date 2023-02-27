use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
pub use tokio_util::sync::CancellationToken;

use crate::{ActorContext, Gate};
use crate::actor::{Actor, Handler, Message};
use crate::bus::EventReceiver;
use crate::handler::ActorMessage;
use crate::handler::MessageHandler;
use crate::system::{ActorSystem, SystemEvent};

pub(crate) struct ActorRunner<E: SystemEvent, A: Actor<E>, M: Message> {
    actor: A,
    cancel_token : CancellationToken,
    event_receiver: EventReceiver<E>,
    receiver: Receiver<Box<ActorMessage<M>>>,
}

impl<E: SystemEvent, A, M: Message> ActorRunner<E, A, M>
  where A: Actor<E> +Handler<E, M> {
    pub fn create( actor: A,
                   event_receiver: EventReceiver<E>,
                   cancel_token: CancellationToken,
                   buffer: usize) -> (Self, Gate<M>) {
        let (sender, receiver)
            : (crate::Sender<M>, crate::Receiver<M>) =
            mpsc::channel(buffer);

        let runner = ActorRunner {
            actor,
            cancel_token,
            event_receiver,
            receiver,
        };
        let gate = Gate::new(sender);
        (runner, gate)
    }


    pub async fn start(mut self, system: ActorSystem<E>) -> A {
        log::debug!("Starting actor '{:p}'...", &self);

        let mut ctx = ActorContext {
            system: system.clone(),
        };

        // Start the actor
        let start_error = self.actor.pre_start(&mut ctx).await.err();

        // Did we encounter an error at startup? If yes, initiate supervision strategy
        if start_error.is_some() {
            log::error!("Actor '{:p}' failed to start!", &self);
        }
        // No errors encountered at startup, so let's run the actor...
        if start_error.is_none() {
            log::debug!("Actor '{:p}' has started successfully.", &self);
            loop {
                tokio::select! {
                   _ = self.cancel_token.cancelled() => {
                       log::debug!("Actor '{:p}' cancelled", &self);
                       break;
                   }
                   Ok(event) = self.event_receiver.recv() => {
                       if let Err(_error) = self.actor.event(&event, &mut ctx).await {
                            log::debug!("Actor '{:p}' failed event handling", &self);
                            break;
                        }
                   }
                   Some(mut msg) = self.receiver.recv() => {
                       msg.handle(&mut self.actor, &mut ctx).await;
                   }
                }
            }

            // Actor receiver has closed, so stop the actor
            self.actor.post_stop(&mut ctx).await;

            log::debug!("Actor '{:p}' stopped.", &self);
        }

        self.receiver.close();

        self.actor
    }
}