//! Sending a window to the back where a platform can raise another app's window but never
//! lower one, as on macOS: the windows that show over it are raised in core's steps, and a
//! step is awaited in the platform's window list before the next where core says
//! (`Raises::awaits`). The waits leave the engine free meanwhile, so events and further
//! presses go on. A send out of time, or cancelled, stops waiting but still sends its
//! raises: stopping partway would leave the windows raised so far over windows that were in
//! front of them.

use std::rc::Rc;
use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::placement::bounds::CancelToken;
use snappy_core::send_to_back::{RaiseStep, Raises, SendToBack, StackedWindow, Step};
use snappy_core::window::{WindowId, WindowSnapshot};
use snappy_platform::Platform;
use tokio::time::Instant;

use super::sends::SentBy;
use crate::ops::Shared;

/// Longest a send waits altogether, from reading the windows to the last raise, however
/// many windows it raises, so a hung app holds the queue up no longer.
const SEND_LIMIT: Duration = Duration::from_millis(800);
/// Longest wait for one step to show in the platform's window list before the next goes
/// ahead anyway: a raise lands within a frame or two.
const RAISE_LIMIT: Duration = Duration::from_millis(250);
/// How often the window list is read while a step is awaited: about once a frame.
const RAISE_POLL: Duration = Duration::from_millis(8);

const FOCUS_NOT_SHOWN: &str = "The window behind didn't come to the front. Try again.";

/// Raise the windows that show over the window `plan` sends back, `stacked` listing the
/// windows frontmost first, for a send that started at `started`, until its run is
/// `cancel`led.
pub(super) async fn raise_over<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    plan: &SendToBack,
    stacked: &[StackedWindow],
    started: Instant,
    cancel: &CancelToken,
) -> Result<SentBy, WindowError> {
    let raises = Raises::plan(plan, stacked);
    let time = SendTime {
        cancel,
        deadline: started + SEND_LIMIT,
    };
    let reached = raise_steps(shared, plan, &raises, time).await?;
    Ok(SentBy::Raising {
        steps: raises.steps().len(),
        reached,
    })
}

/// How long a send waits for its raises to show: until its deadline, unless its run is
/// cancelled first.
#[derive(Clone, Copy)]
struct SendTime<'a> {
    cancel: &'a CancelToken,
    deadline: Instant,
}

impl SendTime<'_> {
    /// Whether the send is done waiting, so its remaining raises go out at once.
    fn is_up(self) -> bool {
        self.cancel.is_cancelled() || Instant::now() >= self.deadline
    }
}

/// Take `raises`' steps in turn, a step awaited before a step that core says waits for it,
/// and the last one at the end, until the send's `time` is up. A window that can't be
/// raised, as one just closed or a hung app's can't, leaves out every window of its app from
/// then on, as each would hold the engine up as long again. Returns whether the window list
/// showed the last order.
async fn raise_steps<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    plan: &SendToBack,
    raises: &Raises,
    time: SendTime<'_>,
) -> Result<bool, WindowError> {
    let mut left_out = Vec::new();
    let mut unawaited: Option<&RaiseStep> = None;
    let mut reached = true;
    for step in raises.steps() {
        if left_out.contains(&step.request.window()) {
            continue;
        }
        if let Some(before) = unawaited.take_if(|before| raises.awaits(before, step)) {
            reached = await_step(shared, plan, before, &left_out, time).await?;
        }
        if request(shared, step.request).is_err() {
            left_out.extend(raises.windows_of_its_app(step.request.window()));
            continue;
        }
        unawaited = Some(step);
    }
    if let Some(last) = unawaited {
        reached = await_step(shared, plan, last, &left_out, time).await?;
    }
    Ok(reached)
}

/// Await `step`: the window a focus raises in front of the window sent back first
/// (`await_focus`), then the step's order (`await_order`). Returns whether the window list
/// showed the order.
async fn await_step<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    plan: &SendToBack,
    step: &RaiseStep,
    left_out: &[WindowId],
    time: SendTime<'_>,
) -> Result<bool, WindowError> {
    if matches!(step.request, Step::Focus(_)) {
        await_focus(shared, plan, time).await?;
    }
    Ok(await_order(shared, step, left_out, time).await)
}

/// Await the window focused showing in front of the window sent back. It's the first step,
/// so nothing else has been raised yet, and if the window list never shows it, the send
/// fails there. A send out of time or cancelled goes on without it.
async fn await_focus<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    plan: &SendToBack,
    time: SendTime<'_>,
) -> Result<(), WindowError> {
    if shown_in_order(shared, &plan.focus_in_front(), time).await || time.is_up() {
        return Ok(());
    }
    Err(WindowError::unavailable(FOCUS_NOT_SHOWN))
}

/// Await `step`'s order, without the windows `left_out`, until the send's `time` is up.
/// Returns whether the window list showed it.
async fn await_order<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    step: &RaiseStep,
    left_out: &[WindowId],
    time: SendTime<'_>,
) -> bool {
    if time.is_up() {
        return false;
    }
    let order: Vec<WindowSnapshot> = step
        .order_after
        .iter()
        .filter(|window| !left_out.contains(&window.id))
        .copied()
        .collect();
    shown_in_order(shared, &order, time).await
}

/// Focus or raise the window, as `step` says.
fn request<P: Platform>(shared: &Shared<P>, step: Step) -> Result<(), WindowError> {
    let platform = &mut shared.core.borrow_mut().platform;
    match step {
        Step::Focus(window) => {
            platform.focus(window);
            Ok(())
        }
        Step::Raise(window) => platform.raise(window),
    }
}

/// Read the window list until it shows `order`, front to back, for at most `RAISE_LIMIT`,
/// and no longer once the send's `time` is up. Returns whether it did.
async fn shown_in_order<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    order: &[WindowSnapshot],
    time: SendTime<'_>,
) -> bool {
    let until = Instant::now() + RAISE_LIMIT;
    loop {
        if shared.core.borrow_mut().platform.shows_in_order(order) {
            return true;
        }
        if time.is_up() || Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(RAISE_POLL).await;
    }
}
