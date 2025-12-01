use crate::device::via::Via;
use std::sync::{Arc, Mutex};
use winit::keyboard::{Key, KeyCode, NamedKey};
use winit_input_helper::WinitInputHelper;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KeyboardEmulation {
    Physical,
    Logical,
}

#[derive(Debug, Clone)]
pub struct Keyboard {
    pub keyboard_emulation: KeyboardEmulation,
    pub via_device: Arc<Mutex<Via>>,
}

impl Keyboard {
    pub fn new(keyboard_emulation: KeyboardEmulation, via_device: Arc<Mutex<Via>>) -> Self {
        Self {
            keyboard_emulation,
            via_device,
        }
    }

    pub fn update(&mut self, input: &WinitInputHelper) {
        match self.keyboard_emulation {
            KeyboardEmulation::Physical => self.update_physical(input),
            KeyboardEmulation::Logical => self.update_logical(input),
        }
    }

    fn update_physical(&self, input: &WinitInputHelper) {
        const MAPPING: [(KeyCode, u8); 38] = [
            (KeyCode::KeyQ, 0),
            (KeyCode::KeyE, 1),
            (KeyCode::KeyT, 2),
            (KeyCode::KeyU, 3),
            (KeyCode::KeyO, 4),
            (KeyCode::KeyA, 5),
            (KeyCode::KeyD, 6),
            (KeyCode::KeyG, 7),
            (KeyCode::KeyJ, 8),
            (KeyCode::KeyL, 9),
            (KeyCode::ControlLeft, 10),  // cody modifier (makes numbers)
            (KeyCode::ControlRight, 10), // cody modifier (makes numbers)
            (KeyCode::KeyX, 11),
            (KeyCode::KeyV, 12),
            (KeyCode::KeyN, 13),
            (KeyCode::AltLeft, 14),  // meta modifier (makes punctuation)
            (KeyCode::AltRight, 14), // meta modifier (makes punctuation)
            (KeyCode::KeyZ, 15),
            (KeyCode::KeyC, 16),
            (KeyCode::KeyB, 17),
            (KeyCode::KeyM, 18),
            (KeyCode::Enter, 19), // arrow key
            (KeyCode::KeyS, 20),
            (KeyCode::KeyF, 21),
            (KeyCode::KeyH, 22),
            (KeyCode::KeyK, 23),
            (KeyCode::Space, 24),
            (KeyCode::KeyW, 25),
            (KeyCode::KeyR, 26),
            (KeyCode::KeyY, 27),
            (KeyCode::KeyI, 28),
            (KeyCode::KeyP, 39),
            // gamepad emulation
            (KeyCode::ArrowUp, 30),    // up
            (KeyCode::ArrowDown, 31),  // down
            (KeyCode::ArrowLeft, 32),  // left
            (KeyCode::ArrowRight, 33), // right
            (KeyCode::ShiftLeft, 34),  // fire button
            (KeyCode::ShiftRight, 34), // fire button
        ];

        let mut state = [false; 40];
        for (keycode, code) in MAPPING {
            state[code as usize] |= input.key_held(keycode);
        }

        let mut via_device = self.via_device.lock().unwrap();
        for (code, pressed) in state.into_iter().enumerate() {
            via_device.set_pressed(code as u8, pressed);
        }
    }

    fn update_logical(&mut self, input: &WinitInputHelper) {
        fn requires_cody_key(key: &Key<&str>) -> Option<Key<&'static str>> {
            match key {
                Key::Character(key) => {
                    if key.len() != 1 || !key.is_ascii() {
                        return None;
                    }
                    match key.chars().next().unwrap().to_ascii_lowercase() {
                        '1' => Some(Key::Character("q")),
                        '2' => Some(Key::Character("w")),
                        '3' => Some(Key::Character("e")),
                        '4' => Some(Key::Character("r")),
                        '5' => Some(Key::Character("t")),
                        '6' => Some(Key::Character("y")),
                        '7' => Some(Key::Character("u")),
                        '8' => Some(Key::Character("i")),
                        '9' => Some(Key::Character("o")),
                        '0' => Some(Key::Character("p")),
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        fn requires_meta_key(key: &Key<&str>) -> Option<Key<&'static str>> {
            match key {
                Key::Named(NamedKey::Backspace) => Some(Key::Named(NamedKey::Enter)),
                Key::Character(key) => {
                    if key.len() != 1 || !key.is_ascii() {
                        return None;
                    }
                    match key.chars().next().unwrap().to_ascii_lowercase() {
                        '!' => Some(Key::Character("q")),
                        '"' => Some(Key::Character("w")),
                        '#' => Some(Key::Character("e")),
                        '$' => Some(Key::Character("r")),
                        '%' => Some(Key::Character("t")),
                        '^' => Some(Key::Character("y")),
                        '&' => Some(Key::Character("u")),
                        '*' => Some(Key::Character("i")),
                        '(' => Some(Key::Character("o")),
                        ')' => Some(Key::Character("p")),
                        '@' => Some(Key::Character("a")),
                        '=' => Some(Key::Character("s")),
                        '-' => Some(Key::Character("d")),
                        '+' => Some(Key::Character("f")),
                        ':' => Some(Key::Character("g")),
                        ';' => Some(Key::Character("h")),
                        '\'' => Some(Key::Character("j")),
                        '[' => Some(Key::Character("k")),
                        ']' => Some(Key::Character("l")),
                        '\\' => Some(Key::Character("z")),
                        '<' => Some(Key::Character("x")),
                        '>' => Some(Key::Character("c")),
                        ',' => Some(Key::Character("v")),
                        '.' => Some(Key::Character("b")),
                        '?' => Some(Key::Character("n")),
                        '/' => Some(Key::Character("m")),
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        fn cody_code(key: &Key<&str>) -> Option<u8> {
            match key {
                Key::Named(key) => match key {
                    NamedKey::Control => Some(11), // cody modifier (makes numbers)
                    NamedKey::Alt => Some(14),     // meta modifier (makes punctuation)
                    NamedKey::Enter => Some(19),   // arrow key
                    NamedKey::Space => Some(24),
                    // gamepad emulation
                    NamedKey::ArrowUp => Some(30),    // up
                    NamedKey::ArrowDown => Some(31),  // down
                    NamedKey::ArrowLeft => Some(32),  // left
                    NamedKey::ArrowRight => Some(33), // right
                    NamedKey::Shift => Some(34),      // fire button
                    _ => None,
                },
                Key::Character(key) => {
                    if key.len() != 1 || !key.is_ascii() {
                        return None;
                    }
                    match key.chars().next().unwrap().to_ascii_lowercase() {
                        'q' => Some(0),
                        'e' => Some(1),
                        't' => Some(2),
                        'u' => Some(3),
                        'o' => Some(4),
                        'a' => Some(5),
                        'd' => Some(6),
                        'g' => Some(7),
                        'j' => Some(8),
                        'l' => Some(9),
                        // cody => 10
                        'x' => Some(11),
                        'v' => Some(12),
                        'n' => Some(13),
                        // meta => 14
                        'z' => Some(15),
                        'c' => Some(16),
                        'b' => Some(17),
                        'm' => Some(18),
                        '\n' | '\r' => Some(19),
                        's' => Some(20),
                        'f' => Some(21),
                        'h' => Some(22),
                        'k' => Some(23),
                        ' ' => Some(24),
                        'w' => Some(25),
                        'r' => Some(26),
                        'y' => Some(27),
                        'i' => Some(28),
                        'p' => Some(29),
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        let mut state = [false; 40];
        for key in input.text() {
            let mut key = key.as_ref();
            if let Some(main_key) = requires_cody_key(&key) {
                state[10] |= true;
                key = main_key;
            } else if let Some(main_key) = requires_meta_key(&key) {
                state[14] |= true;
                key = main_key;
            }

            if let Some(code) = cody_code(&key) {
                state[code as usize] |= true;
            }
        }

        let mut via_device = self.via_device.lock().unwrap();
        for (code, pressed) in state.into_iter().enumerate() {
            via_device.set_pressed(code as u8, pressed);
        }
    }
}
