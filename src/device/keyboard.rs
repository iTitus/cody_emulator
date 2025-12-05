use crate::device::via::{CodyKeyCode, CodyModifier, KeyState};
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
        const MAPPING: [(Key<&'static str>, CodyKeyCode, Option<CodyModifier>); 72] = [
            (Key::Character("q"), CodyKeyCode::KeyQ, None),
            (Key::Character("e"), CodyKeyCode::KeyE, None),
            (Key::Character("t"), CodyKeyCode::KeyT, None),
            (Key::Character("u"), CodyKeyCode::KeyU, None),
            (Key::Character("o"), CodyKeyCode::KeyO, None),
            (Key::Character("a"), CodyKeyCode::KeyA, None),
            (Key::Character("d"), CodyKeyCode::KeyD, None),
            (Key::Character("g"), CodyKeyCode::KeyG, None),
            (Key::Character("j"), CodyKeyCode::KeyJ, None),
            (Key::Character("l"), CodyKeyCode::KeyL, None),
            (
                Key::Named(NamedKey::Control),
                CodyKeyCode::Cody,
                Some(CodyModifier::Cody),
            ),
            (Key::Character("x"), CodyKeyCode::KeyX, None),
            (Key::Character("v"), CodyKeyCode::KeyV, None),
            (Key::Character("n"), CodyKeyCode::KeyN, None),
            (
                Key::Named(NamedKey::Alt),
                CodyKeyCode::Meta,
                Some(CodyModifier::Meta),
            ),
            (Key::Character("z"), CodyKeyCode::KeyZ, None),
            (Key::Character("c"), CodyKeyCode::KeyC, None),
            (Key::Character("b"), CodyKeyCode::KeyB, None),
            (Key::Character("m"), CodyKeyCode::KeyM, None),
            (Key::Named(NamedKey::Enter), CodyKeyCode::Enter, None),
            (Key::Character("s"), CodyKeyCode::KeyS, None),
            (Key::Character("f"), CodyKeyCode::KeyF, None),
            (Key::Character("h"), CodyKeyCode::KeyH, None),
            (Key::Character("k"), CodyKeyCode::KeyK, None),
            (Key::Named(NamedKey::Space), CodyKeyCode::Space, None),
            (Key::Character("w"), CodyKeyCode::KeyW, None),
            (Key::Character("r"), CodyKeyCode::KeyR, None),
            (Key::Character("y"), CodyKeyCode::KeyY, None),
            (Key::Character("i"), CodyKeyCode::KeyI, None),
            (Key::Character("p"), CodyKeyCode::KeyP, None),
            (
                Key::Named(NamedKey::ArrowUp),
                CodyKeyCode::Joystick1Up,
                None,
            ),
            (
                Key::Named(NamedKey::ArrowDown),
                CodyKeyCode::Joystick1Down,
                None,
            ),
            (
                Key::Named(NamedKey::ArrowLeft),
                CodyKeyCode::Joystick1Left,
                None,
            ),
            (
                Key::Named(NamedKey::ArrowRight),
                CodyKeyCode::Joystick1Right,
                None,
            ),
            (
                Key::Named(NamedKey::Shift),
                CodyKeyCode::Joystick1Fire,
                None,
            ),
            (
                Key::Character("1"),
                CodyKeyCode::KeyQ,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("2"),
                CodyKeyCode::KeyW,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("3"),
                CodyKeyCode::KeyE,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("4"),
                CodyKeyCode::KeyR,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("5"),
                CodyKeyCode::KeyT,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("6"),
                CodyKeyCode::KeyY,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("7"),
                CodyKeyCode::KeyU,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("8"),
                CodyKeyCode::KeyI,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("9"),
                CodyKeyCode::KeyO,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Character("0"),
                CodyKeyCode::KeyP,
                Some(CodyModifier::Cody),
            ),
            (
                Key::Named(NamedKey::Backspace),
                CodyKeyCode::Enter,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("!"),
                CodyKeyCode::KeyQ,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("\""),
                CodyKeyCode::KeyW,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("#"),
                CodyKeyCode::KeyE,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("$"),
                CodyKeyCode::KeyR,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("%"),
                CodyKeyCode::KeyT,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("^"),
                CodyKeyCode::KeyY,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("&"),
                CodyKeyCode::KeyU,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("*"),
                CodyKeyCode::KeyI,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("("),
                CodyKeyCode::KeyO,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character(")"),
                CodyKeyCode::KeyP,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("@"),
                CodyKeyCode::KeyA,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("="),
                CodyKeyCode::KeyS,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("-"),
                CodyKeyCode::KeyD,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("+"),
                CodyKeyCode::KeyF,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character(":"),
                CodyKeyCode::KeyG,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character(";"),
                CodyKeyCode::KeyH,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("'"),
                CodyKeyCode::KeyJ,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("["),
                CodyKeyCode::KeyK,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("]"),
                CodyKeyCode::KeyL,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("\\"),
                CodyKeyCode::KeyZ,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("<"),
                CodyKeyCode::KeyX,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character(">"),
                CodyKeyCode::KeyC,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character(","),
                CodyKeyCode::KeyV,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("."),
                CodyKeyCode::KeyB,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("?"),
                CodyKeyCode::KeyN,
                Some(CodyModifier::Meta),
            ),
            (
                Key::Character("/"),
                CodyKeyCode::KeyM,
                Some(CodyModifier::Meta),
            ),
        ];

        let mut state = [false; CodyKeyCode::COUNT];
        for (key, code, modifier) in MAPPING {
            if input.key_held_logical(key) {
                match modifier {
                    Some(CodyModifier::Cody) => state[CodyKeyCode::Cody as usize] |= true,
                    Some(CodyModifier::Meta) => state[CodyKeyCode::Meta as usize] |= true,
                    _ => {}
                }

                state[code as usize] |= true;
            }
        }

        let mut key_state = self.key_state.borrow_mut();
        for (code, pressed) in state.into_iter().enumerate() {
            key_state.set_pressed((code as u8).try_into().unwrap(), pressed);
        }
    }
}
