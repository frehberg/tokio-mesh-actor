# Tokio Mesh Actor Framework

Status: This is Work in Progress
# Design Goals

* [x] Message oriented actor framework
* [x] Gates granting access to certain Message-API (property)
* [x] Gates just depend on message type, are independend of actor type
* [x] Gates of same message type but different actor types may stored in containers. 
* [x] Organizing actors in groups
* [x] Termination of group will stop the corresponding actors
* [x] SystemEvent bus
* [x] Pre-Start and Post-Stop handlers
* [x] JonHandles to synchronize with terminated actors
* [ ] Support for timers and cyclic actions
* [ ] DualGate Actors
* [ ] UDP message entity
* [ ] TCP stream tokenizer entity

# Introduction

In contrast to other actor frameworks, the 
`Tokio Mesh Actor Framework` is message 
oriented; defining channels of messages between entities; 
the iplementation type of the actor is not reflected in external 
handles.

More or less, this is a framework around the `select!` statement
and bounded channels `tokio::sync::channel`

Currently an actor is receiving data from a single `Gate<M>` only,
but design will permit multiple gates in future. This way gates
may be used as property to access privileged API of the actor.

Actors are managed in groups. When the group is terminated or
dropped, all associated actors are terminated.

Each actor may create another actor in same group 
or in  newly created group; the groups will form a hierarchy.

JoinHandles may be used to synchronize with terminating 
actors and read their final state.

```rust
// test code demonstrating usage
let bus = EventBus::new(200);
let system: ActorSystem<TestSystemEvent> = ActorSystem::new(bus);
system.publish(TestSystemEvent::None);

let actor1 = TestActor1 { num: 0 };
let actor2 = TestActor2 { num: 0 };
let group = system.create_group();
let (join_handle1, gate1) = group.single_gated_actor(actor1, 200);
let (join_handle2, gate2) = group.single_gated_actor(actor2, 200);
// gates of same type may be managed in containers
for gate in vec![gate1, gate2].iter() {
  gate.tell(TestMessage("Hallo".to_string())).await;
}
group.terminate();

// wait for terminated actor1 and actor2
let terminated = join_handle1.await;
assert!(terminated.is_ok());
let terminated = join_handle2.await;
assert!(terminated.is_ok()); 
// dropping hte group, all corresponding actors would be terminated anyway 
```
