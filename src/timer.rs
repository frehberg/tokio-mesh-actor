use derivative::Derivative;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::{JoinError, JoinHandle};
use tokio::time::{Duration, sleep};
pub use tokio_util::sync::CancellationToken;

use crate::actor::{Actor, Handler, Message};
use crate::handler::{ActorMessage, BoxedMessageHandler};
use crate::system::SystemEvent;

pub struct TimerRef(JoinHandle<()>);

impl TimerRef {
    pub fn cancel(&self) {
        self.0.abort();
    }

    pub async fn sync(self) -> Result<(), JoinError> {
        self.0.await
    }
}


#[derive(Debug, Clone)]
pub struct Timer<E: SystemEvent, A: Actor<E>> (UnboundedSender<BoxedMessageHandler<E, A>>);

impl<E: SystemEvent, A: Actor<E>> Timer<E, A>
{
    pub fn new(sender: UnboundedSender<BoxedMessageHandler<E, A>>) -> Timer<E, A> {
        Timer(sender)
    }

    pub fn oneshot_timer<M: Message>(&mut self, duration: Duration, message: M) -> TimerRef
        where
            M: Message,
            A: Handler<E, M>,
    {
        let timer_tx = self.0.clone();
        let join_handle =
            tokio::spawn(async move {
                sleep(duration).await;
                let message = ActorMessage::<M>::new(message, None);
                if let Err(error) = timer_tx.send(Box::new(message)) {
                    let msg = error.to_string();
                    log::error!("Failed to tell message! {}", error.to_string());
                }
                ()
            });
        TimerRef(join_handle)
    }
}


#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use tokio::time::timeout;

    use crate::ActorContext;
    use crate::actor::{Actor, Handler};
    use crate::bus::EventBus;
    use crate::system::ActorSystem;

    use super::*;

    #[derive(Debug, Clone)]
    enum TestSystemEvent {
        None
    }

    impl SystemEvent for TestSystemEvent {}

    #[derive(Derivative)]
    #[derivative(Debug, PartialEq)]
    struct TestActor1 {
        pub count_down: usize,
        #[derivative(PartialEq = "ignore")]
        #[derivative(Debug = "ignore")]
        pub continue_test: CancellationToken,
    }

    #[async_trait]
    impl Actor<TestSystemEvent> for TestActor1 {}

    #[derive(Clone, Debug)]
    struct TestTimerMessage(pub String);

    impl Message for TestTimerMessage {
        type Response = ();
    }

    #[async_trait]
    impl Handler<TestSystemEvent, TestTimerMessage> for TestActor1 {
        async fn handle(&mut self, _msg: TestTimerMessage, _ctx: &mut ActorContext<TestSystemEvent, Self>) -> () {
            if self.count_down - 1 == 0 {
                self.count_down -= 1;
                self.continue_test.cancel();
            } else {
                self.count_down -= 1;
            }
            ()
        }
    }

    #[derive(Clone, Debug)]
    struct TestAddTimer(usize);


    impl Message for TestAddTimer {
        type Response = ();
    }

    #[async_trait]
    impl Handler<TestSystemEvent, TestAddTimer> for TestActor1 {
        async fn handle(&mut self, msg: TestAddTimer, ctx: &mut ActorContext<TestSystemEvent, Self>) -> () {
            let _ = ctx.timer().oneshot_timer(
                Duration::from_millis(msg.0 as u64),
                TestTimerMessage("Timer Works".to_string()));
            ()
        }
    }

    #[tokio::test]
    async fn test_timer_trigger() {
        let bus = EventBus::new(200);
        let system: ActorSystem<TestSystemEvent> = ActorSystem::new(bus);
        let continue_test = CancellationToken::new();

        let count_down = 2;
        let delay = 200;
        let actor1 = TestActor1 { count_down, continue_test: continue_test.clone() };
        let group = system.create_group();
        let (join_handle1, gate1) =
            group.single_gated_actor(actor1, 200);

        // add timer and wait before adding the next timer
        for _i in 0..count_down {
            let _ = gate1.tell(TestAddTimer(delay)).await.expect("timer error");
            tokio::time::sleep(Duration::from_millis(delay as u64)).await;
        }

        // reaching zero, the actor shall terminate the test, or fail after timeout
        let total_time_max = 2 * count_down * delay;
        let all_timers_counted = timeout(Duration::from_millis(total_time_max as u64),
                                         continue_test.cancelled()).await;
        assert!(all_timers_counted.is_ok());

        // gates of same type may be managed in containers
        group.terminate();

        // wait for terminated actor1 and actor2
        let terminated = join_handle1.await;
        assert!(terminated.is_ok());
        assert_eq!(terminated.unwrap().count_down, 0);
    }


    #[tokio::test]
    async fn test_unbound_channel() {}
}


#[cfg(test)]
mod demo {
    use async_trait::async_trait;
    use tokio::time::timeout;

    use crate::ActorContext;
    use crate::actor::{Actor, Handler};
    use crate::bus::EventBus;
    use crate::system::ActorSystem;

    use super::*;

    #[derive(Debug, Clone)]
    enum DemoSysEvent {
        None
    }

    impl SystemEvent for DemoSysEvent {}

    #[derive(Derivative)]
    #[derivative(Debug, PartialEq)]
    struct DemoActor;

    #[async_trait]
    impl Actor<DemoSysEvent> for DemoActor {}

    #[derive(Clone, Debug)]
    enum TimerEvent { REPEAT_ACTION }

    impl Message for TimerEvent {
        type Response = ();
    }

    #[async_trait]
    impl Handler<DemoSysEvent, TimerEvent> for DemoActor {
        async fn handle(&mut self, event: TimerEvent, ctx: &mut ActorContext<DemoSysEvent, Self>) -> () {
            /* do something and finally trigger timer to repeat the task */

            let _ = ctx.timer().oneshot_timer(
                Duration::from_millis(200 /* delay */ ),
                TimerEvent::REPEAT_ACTION);
            ()
        }
    }

    #[tokio::test]
    async fn test_timer_trigger() {
        let bus = EventBus::new(200);
        let system: ActorSystem<DemoSysEvent> = ActorSystem::new(bus);
        let continue_test = CancellationToken::new();

        let count_down = 2;
        let delay = 200;
        let actor = DemoActor;
        let group = system.create_group();
        let (join_handle1, gate1) =
            group.single_gated_actor(actor, 200);

        // gates of same type may be managed in containers
        group.terminate();

    }


    #[tokio::test]
    async fn test_unbound_channel() {}
}
