use std::{
    pin::Pin,
    task::{Context, Poll},
};

enum State {
    On,
    Off,
}

enum Event {
    SwitchOn,
    SwitchOff,
}

impl State {
    async fn transition(self, event: Event) -> Self {
        match (&self, event) {
            (State::On, Event::SwitchOff) => {
                println!("Transition to the Off state.");
                State::Off
            }
            (State::Off, Event::SwitchOn) => {
                println!("Transition to the On state.");
                State::On
            }
            _ => {
                println!("No transition, staying in the current state.");
                self
            }
        }
    }
}

struct StateFuture<F: Future, X: Future> {
    pub state: State,
    pub on_future: F,
    pub off_future: X,
}

impl<F: Future, X: Future> Future for StateFuture<F, X> {
    type Output = State;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state {
            State::On => {
                let inner = unsafe { self.map_unchecked_mut(|s| &mut s.on_future) };
                let _ = inner.poll(cx);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            State::Off => {
                let inner = unsafe { self.map_unchecked_mut(|s| &mut s.off_future) };
                let _ = inner.poll(cx);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let mut state = State::On;

    state = state.transition(Event::SwitchOff).await;
    state = state.transition(Event::SwitchOn).await;
    state.transition(Event::SwitchOn).await;
}
