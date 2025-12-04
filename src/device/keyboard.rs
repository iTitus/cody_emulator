use crate::device::via::{CodyKeyCode, KeyState};
use std::cell::RefCell;
use std::rc::Rc;
use strum::EnumCount;
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
    pub key_state: Rc<RefCell<KeyState>>,
}

impl Keyboard {
    pub fn new(keyboard_emulation: KeyboardEmulation, key_state: Rc<RefCell<KeyState>>) -> Self {
        Self {
            keyboard_emulation,
            key_state,
        }
    }

    pub fn update(&mut self, input: &WinitInputHelper) {
        match self.keyboard_emulation {
            KeyboardEmulation::Physical => self.update_physical(input),
            KeyboardEmulation::Logical => self.update_logical(input),
        }
    }

    fn update_physical(&self, input: &WinitInputHelper) {
        const MAPPING: [(KeyCode, CodyKeyCode); 38] = [
            (KeyCode::KeyQ, CodyKeyCode::KeyQ),
            (KeyCode::KeyE, CodyKeyCode::KeyE),
            (KeyCode::KeyT, CodyKeyCode::KeyT),
            (KeyCode::KeyU, CodyKeyCode::KeyU),
            (KeyCode::KeyO, CodyKeyCode::KeyO),
            (KeyCode::KeyA, CodyKeyCode::KeyA),
            (KeyCode::KeyD, CodyKeyCode::KeyD),
            (KeyCode::KeyG, CodyKeyCode::KeyG),
            (KeyCode::KeyJ, CodyKeyCode::KeyJ),
            (KeyCode::KeyL, CodyKeyCode::KeyL),
            (KeyCode::ControlLeft, CodyKeyCode::Cody), // cody modifier (makes numbers)
            (KeyCode::ControlRight, CodyKeyCode::Cody), // cody modifier (makes numbers)
            (KeyCode::KeyX, CodyKeyCode::KeyX),
            (KeyCode::KeyV, CodyKeyCode::KeyV),
            (KeyCode::KeyN, CodyKeyCode::KeyN),
            (KeyCode::AltLeft, CodyKeyCode::Meta), // meta modifier (makes punctuation)
            (KeyCode::AltRight, CodyKeyCode::Meta), // meta modifier (makes punctuation)
            (KeyCode::KeyZ, CodyKeyCode::KeyZ),
            (KeyCode::KeyC, CodyKeyCode::KeyC),
            (KeyCode::KeyB, CodyKeyCode::KeyB),
            (KeyCode::KeyM, CodyKeyCode::KeyM),
            (KeyCode::Enter, CodyKeyCode::Enter), // arrow key
            (KeyCode::KeyS, CodyKeyCode::KeyS),
            (KeyCode::KeyF, CodyKeyCode::KeyF),
            (KeyCode::KeyH, CodyKeyCode::KeyH),
            (KeyCode::KeyK, CodyKeyCode::KeyK),
            (KeyCode::Space, CodyKeyCode::Space),
            (KeyCode::KeyW, CodyKeyCode::KeyW),
            (KeyCode::KeyR, CodyKeyCode::KeyR),
            (KeyCode::KeyY, CodyKeyCode::KeyY),
            (KeyCode::KeyI, CodyKeyCode::KeyI),
            (KeyCode::KeyP, CodyKeyCode::KeyP),
            // joystick emulation
            (KeyCode::ArrowUp, CodyKeyCode::Joystick1Up), // up
            (KeyCode::ArrowDown, CodyKeyCode::Joystick1Down), // down
            (KeyCode::ArrowLeft, CodyKeyCode::Joystick1Left), // left
            (KeyCode::ArrowRight, CodyKeyCode::Joystick1Right), // right
            (KeyCode::ShiftLeft, CodyKeyCode::Joystick1Fire), // fire button
            (KeyCode::ShiftRight, CodyKeyCode::Joystick1Fire), // fire button
        ];

        let mut state = [false; CodyKeyCode::COUNT];
        for (keycode, code) in MAPPING {
            state[code as usize] |= input.key_held(keycode);
        }

        let mut key_state = self.key_state.borrow_mut();
        for (code, pressed) in state.into_iter().enumerate() {
            key_state.set_pressed((code as u8).try_into().unwrap(), pressed);
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

        let mut state = [false; CodyKeyCode::COUNT];
        // TODO: use key_held_logical instead, this is not working
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

        let mut key_state = self.key_state.borrow_mut();
        for (code, pressed) in state.into_iter().enumerate() {
            key_state.set_pressed((code as u8).try_into().unwrap(), pressed);
        }
    }
}
