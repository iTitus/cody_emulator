use crate::device::via::Via;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use winit::event::KeyEvent;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KeyboardEmulation {
    Physical,
    Logical,
}

#[derive(Debug, Clone)]
pub struct Keyboard {
    pub keyboard_emulation: KeyboardEmulation,
    pub via_device: Arc<Mutex<Via>>,
    cody_mod_keys: HashSet<u8>,
    meta_mod_keys: HashSet<u8>,
}

impl Keyboard {
    pub fn new(keyboard_emulation: KeyboardEmulation, via_device: Arc<Mutex<Via>>) -> Self {
        Self {
            keyboard_emulation,
            via_device,
            cody_mod_keys: Default::default(),
            meta_mod_keys: Default::default(),
        }
    }

    pub fn on_keyboard_event(&mut self, event: KeyEvent) {
        match self.keyboard_emulation {
            KeyboardEmulation::Physical => self.on_keyboard_input_physical(event),
            KeyboardEmulation::Logical => self.on_keyboard_input_logical(event),
        }
    }

    fn on_keyboard_input_physical(&self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state.is_pressed();
        let cody_code = match code {
            KeyCode::KeyQ => 1,
            KeyCode::KeyE => 2,
            KeyCode::KeyT => 3,
            KeyCode::KeyU => 4,
            KeyCode::KeyO => 5,
            KeyCode::KeyA => 6,
            KeyCode::KeyD => 7,
            KeyCode::KeyG => 8,
            KeyCode::KeyJ => 9,
            KeyCode::KeyL => 10,
            KeyCode::ControlLeft | KeyCode::ControlRight => 11, // cody modifier (makes numbers)
            KeyCode::KeyX => 12,
            KeyCode::KeyV => 13,
            KeyCode::KeyN => 14,
            KeyCode::AltLeft | KeyCode::AltRight => 15, // meta modifier (makes punctuation)
            KeyCode::KeyZ => 16,
            KeyCode::KeyC => 17,
            KeyCode::KeyB => 18,
            KeyCode::KeyM => 19,
            KeyCode::Enter => 20, // arrow key
            KeyCode::KeyS => 21,
            KeyCode::KeyF => 22,
            KeyCode::KeyH => 23,
            KeyCode::KeyK => 24,
            KeyCode::Space => 25,
            KeyCode::KeyW => 26,
            KeyCode::KeyR => 27,
            KeyCode::KeyY => 28,
            KeyCode::KeyI => 29,
            KeyCode::KeyP => 30,
            // gamepad emulation
            KeyCode::ArrowUp => 31,                         // up
            KeyCode::ArrowDown => 32,                       // down
            KeyCode::ArrowLeft => 33,                       // left
            KeyCode::ArrowRight => 34,                      // right
            KeyCode::ShiftLeft | KeyCode::ShiftRight => 35, // fire button
            _ => 0,
        };
        if cody_code > 0 {
            let mut via_device = self.via_device.lock().unwrap();
            via_device.set_pressed(cody_code - 1, pressed);
        }
    }

    fn on_keyboard_input_logical(&mut self, event: KeyEvent) {
        let key = event.logical_key;
        let pressed = event.state.is_pressed();
        let mut cody_mod = false;
        let mut meta_mod = false;
        let cody_code = match key {
            Key::Named(key) => match key {
                NamedKey::Control => {
                    cody_mod = true;
                    11 // cody modifier (makes numbers)
                }
                NamedKey::Alt => {
                    meta_mod = true;
                    15 // meta modifier (makes punctuation)
                }
                NamedKey::Enter => 20, // arrow key
                NamedKey::Backspace => {
                    meta_mod = true;
                    20 // with arrow key
                }
                NamedKey::Space => 25,
                // gamepad emulation
                NamedKey::ArrowUp => 31,    // up
                NamedKey::ArrowDown => 32,  // down
                NamedKey::ArrowLeft => 33,  // left
                NamedKey::ArrowRight => 34, // right
                NamedKey::Shift => 35,      // fire button
                _ => 0,
            },
            Key::Character(c) => {
                if !c.is_ascii() || c.len() != 1 {
                    0
                } else {
                    let mut c = c.chars().next().unwrap().to_ascii_lowercase();
                    match c {
                        '1' => {
                            cody_mod = true;
                            c = 'q';
                        }
                        '2' => {
                            cody_mod = true;
                            c = 'w';
                        }
                        '3' => {
                            cody_mod = true;
                            c = 'e';
                        }
                        '4' => {
                            cody_mod = true;
                            c = 'r';
                        }
                        '5' => {
                            cody_mod = true;
                            c = 't';
                        }
                        '6' => {
                            cody_mod = true;
                            c = 'y';
                        }
                        '7' => {
                            cody_mod = true;
                            c = 'u';
                        }
                        '8' => {
                            cody_mod = true;
                            c = 'i';
                        }
                        '9' => {
                            cody_mod = true;
                            c = 'o';
                        }
                        '0' => {
                            cody_mod = true;
                            c = 'p';
                        }
                        '!' => {
                            meta_mod = true;
                            c = 'q';
                        }
                        '"' => {
                            meta_mod = true;
                            c = 'w';
                        }
                        '#' => {
                            meta_mod = true;
                            c = 'e';
                        }
                        '$' => {
                            meta_mod = true;
                            c = 'r';
                        }
                        '%' => {
                            meta_mod = true;
                            c = 't';
                        }
                        '^' => {
                            meta_mod = true;
                            c = 'y';
                        }
                        '&' => {
                            meta_mod = true;
                            c = 'u';
                        }
                        '*' => {
                            meta_mod = true;
                            c = 'i';
                        }
                        '(' => {
                            meta_mod = true;
                            c = 'o';
                        }
                        ')' => {
                            meta_mod = true;
                            c = 'p';
                        }
                        '@' => {
                            meta_mod = true;
                            c = 'a';
                        }
                        '=' => {
                            meta_mod = true;
                            c = 's';
                        }
                        '-' => {
                            meta_mod = true;
                            c = 's';
                        }
                        '+' => {
                            meta_mod = true;
                            c = 'f';
                        }
                        ':' => {
                            meta_mod = true;
                            c = 'g';
                        }
                        ';' => {
                            meta_mod = true;
                            c = 'h';
                        }
                        '\'' => {
                            meta_mod = true;
                            c = 'j';
                        }
                        '[' => {
                            meta_mod = true;
                            c = 'k';
                        }
                        ']' => {
                            meta_mod = true;
                            c = 'l';
                        }
                        '\\' => {
                            meta_mod = true;
                            c = 'z';
                        }
                        '<' => {
                            meta_mod = true;
                            c = 'x';
                        }
                        '>' => {
                            meta_mod = true;
                            c = 'c';
                        }
                        ',' => {
                            meta_mod = true;
                            c = 'v';
                        }
                        '.' => {
                            meta_mod = true;
                            c = 'b';
                        }
                        '?' => {
                            meta_mod = true;
                            c = 'n';
                        }
                        '/' => {
                            meta_mod = true;
                            c = 'm';
                        }
                        _ => {}
                    }

                    match c {
                        'q' => 1,
                        'e' => 2,
                        't' => 3,
                        'u' => 4,
                        'o' => 5,
                        'a' => 6,
                        'd' => 7,
                        'g' => 8,
                        'j' => 9,
                        'l' => 10,
                        'x' => 12,
                        'v' => 13,
                        'n' => 14,
                        'z' => 16,
                        'c' => 17,
                        'b' => 18,
                        'm' => 19,
                        '\n' | '\r' => 20,
                        's' => 21,
                        'f' => 22,
                        'h' => 23,
                        'k' => 24,
                        ' ' => 25,
                        'w' => 26,
                        'r' => 27,
                        'y' => 28,
                        'i' => 29,
                        'p' => 30,
                        _ => 0,
                    }
                }
            }
            _ => 0,
        };
        if cody_code > 0 {
            let cody_code = cody_code - 1;
            if cody_mod {
                if pressed {
                    self.cody_mod_keys.insert(cody_code);
                } else {
                    self.cody_mod_keys.remove(&cody_code);
                }
            }
            if meta_mod {
                if pressed {
                    self.meta_mod_keys.insert(cody_code);
                } else {
                    self.meta_mod_keys.remove(&cody_code);
                }
            }

            let mut via_device = self.via_device.lock().unwrap();
            via_device.set_pressed(cody_code, pressed);
            via_device.set_pressed(10, !self.cody_mod_keys.is_empty());
            via_device.set_pressed(14, !self.meta_mod_keys.is_empty());
        }
    }
}
