//! Following a browser's wheel and touch events for a view drawn in an
//! element, like [Ratzilla]'s, with [`Gestures`].
//!
//! [Ratzilla]: https://github.com/orhun/ratzilla

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{AddEventListenerOptions, Element, EventTarget, TouchEvent, WheelEvent};

use super::{Gestures, Step};

/// Starts following the wheel and touches over the view drawn in `screen`,
/// with cells `cell` pixels wide and tall, handing each [`Step`] to `step`,
/// which carries it out and returns whether it moved. The view takes these
/// events over from the browser's own scrolling, and follows them for as
/// long as the page lives.
///
/// ```ignore
/// let (width, height) = terminal.backend().cell_size_css_px();
/// let cell = (f64::from(width), f64::from(height));
/// ratmark::gesture::web::listen(&screen, cell, move |step| {
///     step.apply(&mut state.borrow_mut())
/// })?;
/// ```
pub fn listen(
    screen: &Element,
    cell: (f64, f64),
    step: impl FnMut(Step) -> bool + 'static,
) -> Result<(), JsValue> {
    let listener = Rc::new(RefCell::new(Listener {
        gestures: Gestures::new(cell.0, cell.1),
        step: Box::new(step),
        screen: screen.clone(),
        line: cell.1,
        gliding: false,
    }));
    listen_wheel(&listener)?;
    listen_touches(&listener)
}

type Shared = Rc<RefCell<Listener>>;

struct Listener {
    gestures: Gestures,
    step: Box<dyn FnMut(Step) -> bool>,
    screen: Element,
    /// How tall a line is, in pixels, for wheels that go by lines.
    line: f64,
    /// Whether an animation frame is waiting to keep a flick going.
    gliding: bool,
}

impl Listener {
    /// Where `x`, `y` pixels in the window are in the view.
    fn in_view(&self, x: i32, y: i32) -> (f64, f64) {
        let rect = self.screen.get_bounding_client_rect();
        (f64::from(x) - rect.left(), f64::from(y) - rect.top())
    }

    /// How far a wheel event goes, sideways and down, in pixels.
    fn wheel_pixels(&self, e: &WheelEvent) -> (f64, f64) {
        let pixels = |delta: f64| match e.delta_mode() {
            WheelEvent::DOM_DELTA_LINE => delta * self.line,
            WheelEvent::DOM_DELTA_PAGE => delta * f64::from(self.screen.client_height()),
            _ => delta,
        };
        (pixels(e.delta_x()), pixels(e.delta_y()))
    }
}

fn listen_wheel(listener: &Shared) -> Result<(), JsValue> {
    let shared = listener.clone();
    let on_wheel = Closure::<dyn FnMut(WheelEvent)>::new(move |e: WheelEvent| {
        e.prevent_default();
        let listener = &mut *shared.borrow_mut();
        let at = listener.in_view(e.client_x(), e.client_y());
        let delta = listener.wheel_pixels(&e);
        let step = &mut listener.step;
        listener.gestures.wheel(at, delta, e.time_stamp(), step);
    });
    listener
        .borrow()
        .screen
        .add_event_listener_with_callback_and_add_event_listener_options(
            "wheel",
            on_wheel.as_ref().unchecked_ref(),
            &not_passive(),
        )?;
    on_wheel.forget();
    Ok(())
}

/// A finger drags the page along, and a flick keeps it gliding.
fn listen_touches(listener: &Shared) -> Result<(), JsValue> {
    let shared = listener.clone();
    let on_move = Closure::<dyn FnMut(TouchEvent)>::new(move |e: TouchEvent| {
        e.prevent_default();
        let listener = &mut *shared.borrow_mut();
        let Some(at) = first_touch(listener, &e) else {
            return;
        };
        let step = &mut listener.step;
        listener.gestures.touch_move(at, e.time_stamp(), step);
    })
    .into_js_value();

    let shared = listener.clone();
    let on_end = Closure::<dyn FnMut(TouchEvent)>::new(move |_: TouchEvent| {
        let mut listener = shared.borrow_mut();
        if listener.gestures.touch_end() && !listener.gliding {
            listener.gliding = true;
            drop(listener);
            glide(shared.clone());
        }
    })
    .into_js_value();

    // A touch's events all go to the element it started on, even once a
    // redraw has taken that element out of the page, when they no longer
    // bubble up to the view. So the rest of the touch is followed on the
    // element itself, the innermost one, which may be inside a shadow root.
    let shared = listener.clone();
    let on_start = Closure::<dyn FnMut(TouchEvent)>::new(move |e: TouchEvent| {
        {
            let listener = &mut *shared.borrow_mut();
            if let Some(at) = first_touch(listener, &e) {
                listener.gestures.touch_start(at, e.time_stamp());
            }
        }
        let Ok(target) = e.composed_path().get(0).dyn_into::<EventTarget>() else {
            return;
        };
        // Adding the same listener to an element again does nothing.
        let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
            "touchmove",
            on_move.unchecked_ref(),
            &not_passive(),
        );
        for kind in ["touchend", "touchcancel"] {
            let _ = target.add_event_listener_with_callback(kind, on_end.unchecked_ref());
        }
    });
    listener
        .borrow()
        .screen
        .add_event_listener_with_callback("touchstart", on_start.as_ref().unchecked_ref())?;
    on_start.forget();
    Ok(())
}

/// Keeps a flick going a frame at a time, until it stops.
fn glide(shared: Shared) {
    let Some(window) = web_sys::window() else {
        shared.borrow_mut().gliding = false;
        return;
    };
    let looping = shared.clone();
    let frame = Closure::once_into_js(move |now: f64| {
        let more = {
            let listener = &mut *looping.borrow_mut();
            let step = &mut listener.step;
            let more = listener.gestures.glide(now, step);
            listener.gliding = more;
            more
        };
        if more {
            glide(looping);
        }
    });
    if window
        .request_animation_frame(frame.unchecked_ref())
        .is_err()
    {
        shared.borrow_mut().gliding = false;
    }
}

/// Where the first finger of a touch is in the view, in pixels.
fn first_touch(listener: &Listener, e: &TouchEvent) -> Option<(f64, f64)> {
    let touch = e.touches().get(0)?;
    Some(listener.in_view(touch.client_x(), touch.client_y()))
}

/// Listener options that let the view take the event over from the
/// browser's own scrolling.
fn not_passive() -> AddEventListenerOptions {
    let options = AddEventListenerOptions::new();
    options.set_passive(false);
    options
}
