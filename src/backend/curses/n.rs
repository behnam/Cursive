extern crate ncurses;

use self::super::find_closest;
use backend;
use event::{Event, Key, MouseButton, MouseEvent};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use theme::{Color, ColorPair, Effect};
use utf8;
use vec::Vec2;

pub struct Concrete {
    current_style: Cell<ColorPair>,

    pairs: RefCell<HashMap<ColorPair, i16>>,

    last_mouse_button: Option<MouseButton>,
    event_queue: Vec<Event>,
}

impl Concrete {
    /// Save a new color pair.
    fn insert_color(
        &self, pairs: &mut HashMap<ColorPair, i16>, pair: ColorPair
    ) -> i16 {
        let n = 1 + pairs.len() as i16;
        let target = if ncurses::COLOR_PAIRS() > i32::from(n) {
            // We still have plenty of space for everyone.
            n
        } else {
            // The world is too small for both of us.
            let target = n - 1;
            // Remove the mapping to n-1
            pairs.retain(|_, &mut v| v != target);
            target
        };
        pairs.insert(pair, target);
        ncurses::init_pair(
            target,
            find_closest(&pair.front),
            find_closest(&pair.back),
        );
        target
    }

    /// Checks the pair in the cache, or re-define a color if needed.
    fn get_or_create(&self, pair: ColorPair) -> i16 {
        let mut pairs = self.pairs.borrow_mut();

        // Find if we have this color in stock
        if pairs.contains_key(&pair) {
            // We got it!
            pairs[&pair]
        } else {
            self.insert_color(&mut *pairs, pair)
        }
    }

    fn set_colors(&self, pair: ColorPair) {
        let i = self.get_or_create(pair);

        self.current_style.set(pair);
        let style = ncurses::COLOR_PAIR(i);
        ncurses::attron(style);
    }

    fn parse_mouse_event(&mut self) -> Event {
        let mut mevent = ncurses::MEVENT {
            id: 0,
            x: 0,
            y: 0,
            z: 0,
            bstate: 0,
        };
        if ncurses::getmouse(&mut mevent as *mut ncurses::MEVENT)
            == ncurses::OK
        {
            // eprintln!("{:032b}", mevent.bstate);
            // Currently unused
            let _shift = (mevent.bstate
                & ncurses::BUTTON_SHIFT as ncurses::mmask_t)
                != 0;
            let _alt =
                (mevent.bstate & ncurses::BUTTON_ALT as ncurses::mmask_t) != 0;
            let _ctrl = (mevent.bstate
                & ncurses::BUTTON_CTRL as ncurses::mmask_t)
                != 0;

            mevent.bstate &= !(ncurses::BUTTON_SHIFT | ncurses::BUTTON_ALT
                | ncurses::BUTTON_CTRL)
                as ncurses::mmask_t;

            let make_event = |event| {
                Event::Mouse {
                    offset: Vec2::zero(),
                    position: Vec2::new(mevent.x as usize, mevent.y as usize),
                    event: event,
                }
            };

            if mevent.bstate
                == ncurses::REPORT_MOUSE_POSITION as ncurses::mmask_t
            {
                // The event is either a mouse drag event,
                // or a weird double-release event. :S
                self.last_mouse_button
                    .map(MouseEvent::Hold)
                    .map(&make_event)
                    .unwrap_or_else(|| Event::Unknown(vec![]))
            } else {
                // Identify the button
                let mut bare_event = mevent.bstate & ((1 << 25) - 1);

                let mut event = None;
                while bare_event != 0 {
                    let single_event = 1 << bare_event.trailing_zeros();
                    bare_event ^= single_event;

                    // Process single_event
                    get_event(single_event as i32, |e| if event.is_none() {
                        event = Some(e);
                    } else {
                        self.event_queue.push(make_event(e));
                    });
                }
                if let Some(event) = event {
                    if let Some(btn) = event.button() {
                        self.last_mouse_button = Some(btn);
                    }
                    make_event(event)
                } else {
                    debug!("No event parsed?...");
                    Event::Unknown(vec![])
                }
            }
        } else {
            debug!("Ncurses event not recognized.");
            Event::Unknown(vec![])
        }
    }

    fn parse_ncurses_char(&mut self, ch: i32) -> Event {
        match ch {
            // Value sent by ncurses when nothing happens
            -1 => Event::Refresh,

            // Values under 256 are chars and control values
            //
            // Tab is '\t'
            9 => Event::Key(Key::Tab),
            // Treat '\n' and the numpad Enter the same
            10 | ncurses::KEY_ENTER => Event::Key(Key::Enter),
            // This is the escape key when pressed by itself.
            // When used for control sequences,
            // it should have been caught earlier.
            27 => Event::Key(Key::Esc),
            // `Backspace` sends 127, but Ctrl-H sends `Backspace`
            127 | ncurses::KEY_BACKSPACE => Event::Key(Key::Backspace),

            410 => Event::WindowResize,

            // Values 512 and above are probably extensions
            // Those keys don't seem to be documented...
            520 => Event::Alt(Key::Del),
            521 => Event::AltShift(Key::Del),
            522 => Event::Ctrl(Key::Del),
            523 => Event::CtrlShift(Key::Del),
            //
            // 524?
            526 => Event::Alt(Key::Down),
            527 => Event::AltShift(Key::Down),
            528 => Event::Ctrl(Key::Down),
            529 => Event::CtrlShift(Key::Down),
            530 => Event::CtrlAlt(Key::Down),

            531 => Event::Alt(Key::End),
            532 => Event::AltShift(Key::End),
            533 => Event::Ctrl(Key::End),
            534 => Event::CtrlShift(Key::End),
            535 => Event::CtrlAlt(Key::End),

            536 => Event::Alt(Key::Home),
            537 => Event::AltShift(Key::Home),
            538 => Event::Ctrl(Key::Home),
            539 => Event::CtrlShift(Key::Home),
            540 => Event::CtrlAlt(Key::Home),

            541 => Event::Alt(Key::Ins),
            542 => Event::AltShift(Key::Ins),
            543 => Event::Ctrl(Key::Ins),
            // 544: CtrlShiftIns?
            545 => Event::CtrlAlt(Key::Ins),

            546 => Event::Alt(Key::Left),
            547 => Event::AltShift(Key::Left),
            548 => Event::Ctrl(Key::Left),
            549 => Event::CtrlShift(Key::Left),
            550 => Event::CtrlAlt(Key::Left),

            551 => Event::Alt(Key::PageDown),
            552 => Event::AltShift(Key::PageDown),
            553 => Event::Ctrl(Key::PageDown),
            554 => Event::CtrlShift(Key::PageDown),
            555 => Event::CtrlAlt(Key::PageDown),

            556 => Event::Alt(Key::PageUp),
            557 => Event::AltShift(Key::PageUp),
            558 => Event::Ctrl(Key::PageUp),
            559 => Event::CtrlShift(Key::PageUp),
            560 => Event::CtrlAlt(Key::PageUp),

            561 => Event::Alt(Key::Right),
            562 => Event::AltShift(Key::Right),
            563 => Event::Ctrl(Key::Right),
            564 => Event::CtrlShift(Key::Right),
            565 => Event::CtrlAlt(Key::Right),
            // 566?
            567 => Event::Alt(Key::Up),
            568 => Event::AltShift(Key::Up),
            569 => Event::Ctrl(Key::Up),
            570 => Event::CtrlShift(Key::Up),
            571 => Event::CtrlAlt(Key::Up),

            ncurses::KEY_MOUSE => self.parse_mouse_event(),
            ncurses::KEY_B2 => Event::Key(Key::NumpadCenter),
            ncurses::KEY_DC => Event::Key(Key::Del),
            ncurses::KEY_IC => Event::Key(Key::Ins),
            ncurses::KEY_BTAB => Event::Shift(Key::Tab),
            ncurses::KEY_SLEFT => Event::Shift(Key::Left),
            ncurses::KEY_SRIGHT => Event::Shift(Key::Right),
            ncurses::KEY_LEFT => Event::Key(Key::Left),
            ncurses::KEY_RIGHT => Event::Key(Key::Right),
            ncurses::KEY_UP => Event::Key(Key::Up),
            ncurses::KEY_DOWN => Event::Key(Key::Down),
            ncurses::KEY_SR => Event::Shift(Key::Up),
            ncurses::KEY_SF => Event::Shift(Key::Down),
            ncurses::KEY_PPAGE => Event::Key(Key::PageUp),
            ncurses::KEY_NPAGE => Event::Key(Key::PageDown),
            ncurses::KEY_HOME => Event::Key(Key::Home),
            ncurses::KEY_END => Event::Key(Key::End),
            ncurses::KEY_SHOME => Event::Shift(Key::Home),
            ncurses::KEY_SEND => Event::Shift(Key::End),
            ncurses::KEY_SDC => Event::Shift(Key::Del),
            ncurses::KEY_SNEXT => Event::Shift(Key::PageDown),
            ncurses::KEY_SPREVIOUS => Event::Shift(Key::PageUp),
            // All Fn keys use the same enum with associated number
            f @ ncurses::KEY_F1...ncurses::KEY_F12 => {
                Event::Key(Key::from_f((f - ncurses::KEY_F0) as u8))
            }
            f @ 277...288 => Event::Shift(Key::from_f((f - 276) as u8)),
            f @ 289...300 => Event::Ctrl(Key::from_f((f - 288) as u8)),
            f @ 301...312 => Event::CtrlShift(Key::from_f((f - 300) as u8)),
            f @ 313...324 => Event::Alt(Key::from_f((f - 312) as u8)),
            // Values 8-10 (H,I,J) are used by other commands,
            // so we probably won't receive them. Meh~
            c @ 1...25 => Event::CtrlChar((b'a' + (c - 1) as u8) as char),
            other => {
                // Split the i32 into 4 bytes
                Event::Unknown(
                    (0..4)
                        .map(|i| ((other >> (8 * i)) & 0xFF) as u8)
                        .collect(),
                )
            }
        }
    }
}

impl backend::Backend for Concrete {
    fn init() -> Self {
        // Change the locale.
        // For some reasons it's mandatory to get some UTF-8 support.
        ncurses::setlocale(ncurses::LcCategory::all, "");

        // The delay is the time ncurses wait after pressing ESC
        // to see if it's an escape sequence.
        // Default delay is way too long. 25 is imperceptible yet works fine.
        ::std::env::set_var("ESCDELAY", "25");

        ncurses::initscr();
        ncurses::keypad(ncurses::stdscr(), true);

        // This disables mouse click detection,
        // and provides 0-delay access to mouse presses.
        ncurses::mouseinterval(0);
        // Listen to all mouse events.
        ncurses::mousemask(
            (ncurses::ALL_MOUSE_EVENTS | ncurses::REPORT_MOUSE_POSITION)
                as ncurses::mmask_t,
            None,
        );
        ncurses::noecho();
        ncurses::cbreak();
        ncurses::start_color();
        // Pick up background and text color from the terminal theme.
        ncurses::use_default_colors();
        // No cursor
        ncurses::curs_set(ncurses::CURSOR_VISIBILITY::CURSOR_INVISIBLE);

        // This asks the terminal to provide us with mouse drag events
        // (Mouse move when a button is pressed).
        // Replacing 1002 with 1003 would give us ANY mouse move.
        println!("\x1B[?1002h");

        Concrete {
            current_style: Cell::new(ColorPair::from_256colors(0, 0)),
            pairs: RefCell::new(HashMap::new()),

            last_mouse_button: None,
            event_queue: Vec::new(),
        }
    }

    fn screen_size(&self) -> (usize, usize) {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        ncurses::getmaxyx(ncurses::stdscr(), &mut y, &mut x);
        (x as usize, y as usize)
    }

    fn has_colors(&self) -> bool {
        ncurses::has_colors()
    }

    fn finish(&mut self) {
        println!("\x1B[?1002l");
        ncurses::endwin();
    }


    fn with_color<F: FnOnce()>(&self, colors: ColorPair, f: F) {
        let current = self.current_style.get();
        if current != colors {
            self.set_colors(colors);
        }

        f();

        if current != colors {
            self.set_colors(current);
        }
    }

    fn with_effect<F: FnOnce()>(&self, effect: Effect, f: F) {
        let style = match effect {
            Effect::Reverse => ncurses::A_REVERSE(),
            Effect::Simple => ncurses::A_NORMAL(),
        };
        ncurses::attron(style);
        f();
        ncurses::attroff(style);
    }

    fn clear(&self, color: Color) {
        let id = self.get_or_create(ColorPair {
            front: color,
            back: color,
        });
        ncurses::wbkgd(ncurses::stdscr(), ncurses::COLOR_PAIR(id));

        ncurses::clear();
    }

    fn refresh(&mut self) {
        ncurses::refresh();
    }

    fn print_at(&self, (x, y): (usize, usize), text: &str) {
        ncurses::mvaddstr(y as i32, x as i32, text);
    }

    fn poll_event(&mut self) -> Event {
        self.event_queue.pop().unwrap_or_else(|| {
            let ch: i32 = ncurses::getch();

            // Is it a UTF-8 starting point?
            if 32 <= ch && ch <= 255 && ch != 127 {
                Event::Char(
                    utf8::read_char(ch as u8, || Some(ncurses::getch() as u8))
                        .unwrap(),
                )
            } else {
                self.parse_ncurses_char(ch)
            }
        })
    }

    fn set_refresh_rate(&mut self, fps: u32) {
        if fps == 0 {
            ncurses::timeout(-1);
        } else {
            ncurses::timeout(1000 / fps as i32);
        }
    }
}

/// Returns the Key enum corresponding to the given ncurses event.
fn get_button(bare_event: i32) -> MouseButton {
    match bare_event {
        ncurses::BUTTON1_RELEASED |
        ncurses::BUTTON1_PRESSED |
        ncurses::BUTTON1_CLICKED |
        ncurses::BUTTON1_DOUBLE_CLICKED |
        ncurses::BUTTON1_TRIPLE_CLICKED => MouseButton::Left,
        ncurses::BUTTON2_RELEASED |
        ncurses::BUTTON2_PRESSED |
        ncurses::BUTTON2_CLICKED |
        ncurses::BUTTON2_DOUBLE_CLICKED |
        ncurses::BUTTON2_TRIPLE_CLICKED => MouseButton::Middle,
        ncurses::BUTTON3_RELEASED |
        ncurses::BUTTON3_PRESSED |
        ncurses::BUTTON3_CLICKED |
        ncurses::BUTTON3_DOUBLE_CLICKED |
        ncurses::BUTTON3_TRIPLE_CLICKED => MouseButton::Right,
        ncurses::BUTTON4_RELEASED |
        ncurses::BUTTON4_PRESSED |
        ncurses::BUTTON4_CLICKED |
        ncurses::BUTTON4_DOUBLE_CLICKED |
        ncurses::BUTTON4_TRIPLE_CLICKED => MouseButton::Button4,
        ncurses::BUTTON5_RELEASED |
        ncurses::BUTTON5_PRESSED |
        ncurses::BUTTON5_CLICKED |
        ncurses::BUTTON5_DOUBLE_CLICKED |
        ncurses::BUTTON5_TRIPLE_CLICKED => MouseButton::Button5,
        _ => MouseButton::Other,
    }
}

/// Parse the given code into one or more event.
///
/// If the given event code should expend into multiple events
/// (for instance click expends into PRESS + RELEASE),
/// the returned Vec will include those queued events.
///
/// The main event is returned separately to avoid allocation in most cases.
fn get_event<F>(bare_event: i32, mut f: F)
where
    F: FnMut(MouseEvent),
{
    let button = get_button(bare_event);
    match bare_event {
        ncurses::BUTTON4_PRESSED => f(MouseEvent::WheelUp),
        ncurses::BUTTON5_PRESSED => f(MouseEvent::WheelDown),
        ncurses::BUTTON1_RELEASED |
        ncurses::BUTTON2_RELEASED |
        ncurses::BUTTON3_RELEASED |
        ncurses::BUTTON4_RELEASED |
        ncurses::BUTTON5_RELEASED => f(MouseEvent::Release(button)),
        ncurses::BUTTON1_PRESSED |
        ncurses::BUTTON2_PRESSED |
        ncurses::BUTTON3_PRESSED => f(MouseEvent::Press(button)),
        ncurses::BUTTON1_CLICKED |
        ncurses::BUTTON2_CLICKED |
        ncurses::BUTTON3_CLICKED |
        ncurses::BUTTON4_CLICKED |
        ncurses::BUTTON5_CLICKED => {
            f(MouseEvent::Press(button));
            f(MouseEvent::Release(button));
        }
        // Well, we disabled click detection
        ncurses::BUTTON1_DOUBLE_CLICKED |
        ncurses::BUTTON2_DOUBLE_CLICKED |
        ncurses::BUTTON3_DOUBLE_CLICKED |
        ncurses::BUTTON4_DOUBLE_CLICKED |
        ncurses::BUTTON5_DOUBLE_CLICKED => for _ in 0..2 {
            f(MouseEvent::Press(button));
            f(MouseEvent::Release(button));
        },
        ncurses::BUTTON1_TRIPLE_CLICKED |
        ncurses::BUTTON2_TRIPLE_CLICKED |
        ncurses::BUTTON3_TRIPLE_CLICKED |
        ncurses::BUTTON4_TRIPLE_CLICKED |
        ncurses::BUTTON5_TRIPLE_CLICKED => for _ in 0..3 {
            f(MouseEvent::Press(button));
            f(MouseEvent::Release(button));
        },
        _ => debug!("Unknown event: {:032b}", bare_event),
    }
}
