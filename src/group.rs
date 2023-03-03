use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::Gate;
use crate::actor::{Actor, Handler, Message};
use crate::runner::ActorRunner;
use crate::system::{ActorSystem, SystemEvent};

pub struct ActorGroup<E: SystemEvent> {
    cancel_token: CancellationToken,
    system: ActorSystem<E>,
}

impl<E: SystemEvent> ActorGroup<E>
{
    pub fn new(cancel_token: CancellationToken, system: ActorSystem<E>) -> ActorGroup<E> {
        ActorGroup { cancel_token, system }
    }

    pub fn terminate(&self) {
        self.cancel_token.cancel();
    }

    /// Create actor in new group
    pub fn single_gated_actor<M: Message, A: Actor<E> + Handler<E, M>>(
        &self, actor: A, buffer: usize) -> (JoinHandle<A>, Gate<M>) {
        let event_receiver = self.system.events();
        let cancel_token = self.cancel_token.clone();
        let (runner, gate) =
            ActorRunner::create(actor,
                                event_receiver,
                                cancel_token,
                                buffer);
        let system_clone = self.system.clone();
        let join_handle = tokio::spawn(async move {
            let rv = runner.start(system_clone).await;
            rv
        });

        return (join_handle, gate);
    }
}

/// Drop is terminating the actor group
impl<E: SystemEvent> Drop for ActorGroup<E> {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
