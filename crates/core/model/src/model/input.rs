use std::fmt;

/// Keyboard virtual-key code used by backend keyboard polling capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyCode(u16);

impl KeyCode {
    pub const LBUTTON: Self = Self(0x01);
    pub const RBUTTON: Self = Self(0x02);
    pub const CANCEL: Self = Self(0x03);
    pub const MBUTTON: Self = Self(0x04);
    pub const XBUTTON1: Self = Self(0x05);
    pub const XBUTTON2: Self = Self(0x06);

    pub const BACKSPACE: Self = Self(0x08);
    pub const TAB: Self = Self(0x09);
    pub const CLEAR: Self = Self(0x0c);
    pub const ENTER: Self = Self(0x0d);
    pub const SHIFT: Self = Self(0x10);
    pub const CONTROL: Self = Self(0x11);
    pub const ALT: Self = Self(0x12);
    pub const PAUSE: Self = Self(0x13);
    pub const CAPS_LOCK: Self = Self(0x14);
    pub const KANA: Self = Self(0x15);
    pub const HANGEUL: Self = Self(0x15);
    pub const HANGUL: Self = Self(0x15);
    pub const JUNJA: Self = Self(0x17);
    pub const FINAL: Self = Self(0x18);
    pub const HANJA: Self = Self(0x19);
    pub const KANJI: Self = Self(0x19);
    pub const ESCAPE: Self = Self(0x1b);
    pub const CONVERT: Self = Self(0x1c);
    pub const NONCONVERT: Self = Self(0x1d);
    pub const ACCEPT: Self = Self(0x1e);
    pub const MODECHANGE: Self = Self(0x1f);
    pub const SPACE: Self = Self(0x20);
    pub const PAGE_UP: Self = Self(0x21);
    pub const PAGE_DOWN: Self = Self(0x22);
    pub const END: Self = Self(0x23);
    pub const HOME: Self = Self(0x24);
    pub const LEFT: Self = Self(0x25);
    pub const UP: Self = Self(0x26);
    pub const RIGHT: Self = Self(0x27);
    pub const DOWN: Self = Self(0x28);
    pub const SELECT: Self = Self(0x29);
    pub const PRINT: Self = Self(0x2a);
    pub const EXECUTE: Self = Self(0x2b);
    pub const PRINT_SCREEN: Self = Self(0x2c);
    pub const INSERT: Self = Self(0x2d);
    pub const DELETE: Self = Self(0x2e);
    pub const HELP: Self = Self(0x2f);

    pub const A: Self = Self(0x41);
    pub const B: Self = Self(0x42);
    pub const C: Self = Self(0x43);
    pub const D: Self = Self(0x44);
    pub const E: Self = Self(0x45);
    pub const F: Self = Self(0x46);
    pub const G: Self = Self(0x47);
    pub const H: Self = Self(0x48);
    pub const I: Self = Self(0x49);
    pub const J: Self = Self(0x4a);
    pub const K: Self = Self(0x4b);
    pub const L: Self = Self(0x4c);
    pub const M: Self = Self(0x4d);
    pub const N: Self = Self(0x4e);
    pub const O: Self = Self(0x4f);
    pub const P: Self = Self(0x50);
    pub const Q: Self = Self(0x51);
    pub const R: Self = Self(0x52);
    pub const S: Self = Self(0x53);
    pub const T: Self = Self(0x54);
    pub const U: Self = Self(0x55);
    pub const V: Self = Self(0x56);
    pub const W: Self = Self(0x57);
    pub const X: Self = Self(0x58);
    pub const Y: Self = Self(0x59);
    pub const Z: Self = Self(0x5a);

    pub const NUM_0: Self = Self(0x30);
    pub const NUM_1: Self = Self(0x31);
    pub const NUM_2: Self = Self(0x32);
    pub const NUM_3: Self = Self(0x33);
    pub const NUM_4: Self = Self(0x34);
    pub const NUM_5: Self = Self(0x35);
    pub const NUM_6: Self = Self(0x36);
    pub const NUM_7: Self = Self(0x37);
    pub const NUM_8: Self = Self(0x38);
    pub const NUM_9: Self = Self(0x39);

    pub const LWIN: Self = Self(0x5b);
    pub const RWIN: Self = Self(0x5c);
    pub const APPS: Self = Self(0x5d);
    pub const SLEEP: Self = Self(0x5f);

    pub const NUMPAD0: Self = Self(0x60);
    pub const NUMPAD1: Self = Self(0x61);
    pub const NUMPAD2: Self = Self(0x62);
    pub const NUMPAD3: Self = Self(0x63);
    pub const NUMPAD4: Self = Self(0x64);
    pub const NUMPAD5: Self = Self(0x65);
    pub const NUMPAD6: Self = Self(0x66);
    pub const NUMPAD7: Self = Self(0x67);
    pub const NUMPAD8: Self = Self(0x68);
    pub const NUMPAD9: Self = Self(0x69);
    pub const MULTIPLY: Self = Self(0x6a);
    pub const ADD: Self = Self(0x6b);
    pub const SEPARATOR: Self = Self(0x6c);
    pub const SUBTRACT: Self = Self(0x6d);
    pub const DECIMAL: Self = Self(0x6e);
    pub const DIVIDE: Self = Self(0x6f);

    pub const F1: Self = Self(0x70);
    pub const F2: Self = Self(0x71);
    pub const F3: Self = Self(0x72);
    pub const F4: Self = Self(0x73);
    pub const F5: Self = Self(0x74);
    pub const F6: Self = Self(0x75);
    pub const F7: Self = Self(0x76);
    pub const F8: Self = Self(0x77);
    pub const F9: Self = Self(0x78);
    pub const F10: Self = Self(0x79);
    pub const F11: Self = Self(0x7a);
    pub const F12: Self = Self(0x7b);
    pub const F13: Self = Self(0x7c);
    pub const F14: Self = Self(0x7d);
    pub const F15: Self = Self(0x7e);
    pub const F16: Self = Self(0x7f);
    pub const F17: Self = Self(0x80);
    pub const F18: Self = Self(0x81);
    pub const F19: Self = Self(0x82);
    pub const F20: Self = Self(0x83);
    pub const F21: Self = Self(0x84);
    pub const F22: Self = Self(0x85);
    pub const F23: Self = Self(0x86);
    pub const F24: Self = Self(0x87);

    // AvZ2 also conditionally names Windows 10+ navigation/gamepad VKs. They are intentionally
    // omitted here until a backend or script API needs them.

    pub const NUM_LOCK: Self = Self(0x90);
    pub const SCROLL_LOCK: Self = Self(0x91);
    pub const LSHIFT: Self = Self(0xa0);
    pub const RSHIFT: Self = Self(0xa1);
    pub const LCONTROL: Self = Self(0xa2);
    pub const RCONTROL: Self = Self(0xa3);
    pub const LALT: Self = Self(0xa4);
    pub const RALT: Self = Self(0xa5);
    pub const LMENU: Self = Self(0xa4);
    pub const RMENU: Self = Self(0xa5);

    pub const BROWSER_BACK: Self = Self(0xa6);
    pub const BROWSER_FORWARD: Self = Self(0xa7);
    pub const BROWSER_REFRESH: Self = Self(0xa8);
    pub const BROWSER_STOP: Self = Self(0xa9);
    pub const BROWSER_SEARCH: Self = Self(0xaa);
    pub const BROWSER_FAVORITES: Self = Self(0xab);
    pub const BROWSER_HOME: Self = Self(0xac);
    pub const VOLUME_MUTE: Self = Self(0xad);
    pub const VOLUME_DOWN: Self = Self(0xae);
    pub const VOLUME_UP: Self = Self(0xaf);
    pub const MEDIA_NEXT_TRACK: Self = Self(0xb0);
    pub const MEDIA_PREV_TRACK: Self = Self(0xb1);
    pub const MEDIA_STOP: Self = Self(0xb2);
    pub const MEDIA_PLAY_PAUSE: Self = Self(0xb3);
    pub const LAUNCH_MAIL: Self = Self(0xb4);
    pub const LAUNCH_MEDIA_SELECT: Self = Self(0xb5);
    pub const LAUNCH_APP1: Self = Self(0xb6);
    pub const LAUNCH_APP2: Self = Self(0xb7);

    pub const OEM_1: Self = Self(0xba);
    pub const OEM_2: Self = Self(0xbf);
    pub const OEM_3: Self = Self(0xc0);
    pub const OEM_4: Self = Self(0xdb);
    pub const OEM_5: Self = Self(0xdc);
    pub const OEM_6: Self = Self(0xdd);
    pub const OEM_7: Self = Self(0xde);
    pub const OEM_8: Self = Self(0xdf);
    pub const PROCESSKEY: Self = Self(0xe5);
    pub const PACKET: Self = Self(0xe7);
    pub const ATTN: Self = Self(0xf6);
    pub const CRSEL: Self = Self(0xf7);
    pub const EXSEL: Self = Self(0xf8);
    pub const EREOF: Self = Self(0xf9);
    pub const PLAY: Self = Self(0xfa);
    pub const ZOOM: Self = Self(0xfb);
    pub const NONAME: Self = Self(0xfc);
    pub const PA1: Self = Self(0xfd);
    pub const OEM_CLEAR: Self = Self(0xfe);

    /// Returns the Win32 virtual-key value used by this key.
    #[must_use]
    pub const fn raw_virtual_key(self) -> u16 {
        self.0
    }

    /// Creates a key from a raw Win32 virtual-key value.
    pub const fn try_from_raw_virtual_key(raw: u16) -> Result<Self, KeyCodeError> {
        if raw == 0 || raw > 0xfe {
            return Err(KeyCodeError::InvalidRawVirtualKey { raw });
        }
        Ok(Self(raw))
    }

    /// Creates a key only if the raw Win32 virtual-key value is in AvZ2's known key map.
    pub const fn try_from_known_virtual_key(raw: u16) -> Result<Self, KeyCodeError> {
        if raw == 0 || raw > 0xfe {
            return Err(KeyCodeError::InvalidRawVirtualKey { raw });
        }
        let key = Self(raw);
        if key.is_known_virtual_key() {
            Ok(key)
        } else {
            Err(KeyCodeError::UnknownVirtualKey { raw })
        }
    }

    /// Returns whether this key is in AvZ2's registered virtual-key name map.
    #[must_use]
    pub const fn is_known_virtual_key(self) -> bool {
        self.virtual_key_name().is_some()
    }

    /// Returns this key's AvZ2-compatible virtual-key name when known.
    #[must_use]
    pub const fn virtual_key_name(self) -> Option<&'static str> {
        match self.0 {
            0x01 => Some("LBUTTON"),
            0x02 => Some("RBUTTON"),
            0x03 => Some("CANCEL"),
            0x04 => Some("MBUTTON"),
            0x05 => Some("XBUTTON1"),
            0x06 => Some("XBUTTON2"),
            0x08 => Some("BACKSPACE"),
            0x09 => Some("TAB"),
            0x0c => Some("CLEAR"),
            0x0d => Some("ENTER"),
            0x10 => Some("SHIFT"),
            0x11 => Some("CONTROL"),
            0x12 => Some("ALT"),
            0x13 => Some("PAUSE"),
            0x14 => Some("CAPS_LOCK"),
            0x15 => Some("KANA"),
            0x17 => Some("JUNJA"),
            0x18 => Some("FINAL"),
            0x19 => Some("HANJA"),
            0x1b => Some("ESCAPE"),
            0x1c => Some("CONVERT"),
            0x1d => Some("NONCONVERT"),
            0x1e => Some("ACCEPT"),
            0x1f => Some("MODECHANGE"),
            0x20 => Some("SPACE"),
            0x21 => Some("PAGE_UP"),
            0x22 => Some("PAGE_DOWN"),
            0x23 => Some("END"),
            0x24 => Some("HOME"),
            0x25 => Some("LEFT"),
            0x26 => Some("UP"),
            0x27 => Some("RIGHT"),
            0x28 => Some("DOWN"),
            0x29 => Some("SELECT"),
            0x2a => Some("PRINT"),
            0x2b => Some("EXECUTE"),
            0x2c => Some("PRINT_SCREEN"),
            0x2d => Some("INSERT"),
            0x2e => Some("DELETE"),
            0x2f => Some("HELP"),
            0x30 => Some("0"),
            0x31 => Some("1"),
            0x32 => Some("2"),
            0x33 => Some("3"),
            0x34 => Some("4"),
            0x35 => Some("5"),
            0x36 => Some("6"),
            0x37 => Some("7"),
            0x38 => Some("8"),
            0x39 => Some("9"),
            0x41 => Some("A"),
            0x42 => Some("B"),
            0x43 => Some("C"),
            0x44 => Some("D"),
            0x45 => Some("E"),
            0x46 => Some("F"),
            0x47 => Some("G"),
            0x48 => Some("H"),
            0x49 => Some("I"),
            0x4a => Some("J"),
            0x4b => Some("K"),
            0x4c => Some("L"),
            0x4d => Some("M"),
            0x4e => Some("N"),
            0x4f => Some("O"),
            0x50 => Some("P"),
            0x51 => Some("Q"),
            0x52 => Some("R"),
            0x53 => Some("S"),
            0x54 => Some("T"),
            0x55 => Some("U"),
            0x56 => Some("V"),
            0x57 => Some("W"),
            0x58 => Some("X"),
            0x59 => Some("Y"),
            0x5a => Some("Z"),
            0x5b => Some("LWIN"),
            0x5c => Some("RWIN"),
            0x5d => Some("APPS"),
            0x5f => Some("SLEEP"),
            0x60 => Some("NUMPAD0"),
            0x61 => Some("NUMPAD1"),
            0x62 => Some("NUMPAD2"),
            0x63 => Some("NUMPAD3"),
            0x64 => Some("NUMPAD4"),
            0x65 => Some("NUMPAD5"),
            0x66 => Some("NUMPAD6"),
            0x67 => Some("NUMPAD7"),
            0x68 => Some("NUMPAD8"),
            0x69 => Some("NUMPAD9"),
            0x6a => Some("MULTIPLY"),
            0x6b => Some("ADD"),
            0x6c => Some("SEPARATOR"),
            0x6d => Some("SUBTRACT"),
            0x6e => Some("DECIMAL"),
            0x6f => Some("DIVIDE"),
            0x70 => Some("F1"),
            0x71 => Some("F2"),
            0x72 => Some("F3"),
            0x73 => Some("F4"),
            0x74 => Some("F5"),
            0x75 => Some("F6"),
            0x76 => Some("F7"),
            0x77 => Some("F8"),
            0x78 => Some("F9"),
            0x79 => Some("F10"),
            0x7a => Some("F11"),
            0x7b => Some("F12"),
            0x7c => Some("F13"),
            0x7d => Some("F14"),
            0x7e => Some("F15"),
            0x7f => Some("F16"),
            0x80 => Some("F17"),
            0x81 => Some("F18"),
            0x82 => Some("F19"),
            0x83 => Some("F20"),
            0x84 => Some("F21"),
            0x85 => Some("F22"),
            0x86 => Some("F23"),
            0x87 => Some("F24"),
            0x90 => Some("NUM_LOCK"),
            0x91 => Some("SCROLL_LOCK"),
            0xa0 => Some("LSHIFT"),
            0xa1 => Some("RSHIFT"),
            0xa2 => Some("LCONTROL"),
            0xa3 => Some("RCONTROL"),
            0xa4 => Some("LALT"),
            0xa5 => Some("RALT"),
            0xa6 => Some("BROWSER_BACK"),
            0xa7 => Some("BROWSER_FORWARD"),
            0xa8 => Some("BROWSER_REFRESH"),
            0xa9 => Some("BROWSER_STOP"),
            0xaa => Some("BROWSER_SEARCH"),
            0xab => Some("BROWSER_FAVORITES"),
            0xac => Some("BROWSER_HOME"),
            0xad => Some("VOLUME_MUTE"),
            0xae => Some("VOLUME_DOWN"),
            0xaf => Some("VOLUME_UP"),
            0xb0 => Some("MEDIA_NEXT_TRACK"),
            0xb1 => Some("MEDIA_PREV_TRACK"),
            0xb2 => Some("MEDIA_STOP"),
            0xb3 => Some("MEDIA_PLAY_PAUSE"),
            0xb4 => Some("LAUNCH_MAIL"),
            0xb5 => Some("LAUNCH_MEDIA_SELECT"),
            0xb6 => Some("LAUNCH_APP1"),
            0xb7 => Some("LAUNCH_APP2"),
            0xba => Some("OEM_1"),
            0xbf => Some("OEM_2"),
            0xc0 => Some("OEM_3"),
            0xdb => Some("OEM_4"),
            0xdc => Some("OEM_5"),
            0xdd => Some("OEM_6"),
            0xde => Some("OEM_7"),
            0xdf => Some("OEM_8"),
            0xe5 => Some("PROCESSKEY"),
            0xe7 => Some("PACKET"),
            0xf6 => Some("ATTN"),
            0xf7 => Some("CRSEL"),
            0xf8 => Some("EXSEL"),
            0xf9 => Some("EREOF"),
            0xfa => Some("PLAY"),
            0xfb => Some("ZOOM"),
            0xfc => Some("NONAME"),
            0xfd => Some("PA1"),
            0xfe => Some("OEM_CLEAR"),
            _ => None,
        }
    }

    /// Converts an ASCII letter or digit into a virtual-key code.
    pub const fn try_from_char(ch: char) -> Result<Self, KeyCodeError> {
        if ch.is_ascii_alphabetic() {
            return Ok(Self(ch.to_ascii_uppercase() as u16));
        }
        if ch.is_ascii_digit() {
            return Ok(Self(ch as u16));
        }
        Err(KeyCodeError::UnsupportedChar { ch })
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.virtual_key_name() {
            f.write_str(name)
        } else {
            write!(f, "VK({:#04x})", self.raw_virtual_key())
        }
    }
}

impl TryFrom<char> for KeyCode {
    type Error = KeyCodeError;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        Self::try_from_char(value)
    }
}

impl TryFrom<u16> for KeyCode {
    type Error = KeyCodeError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_from_raw_virtual_key(value)
    }
}

/// Keyboard key conversion error.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyCodeError {
    #[error("unsupported key character {ch:?}; only ASCII letters and digits convert directly")]
    UnsupportedChar { ch: char },
    #[error("invalid raw virtual-key value {raw:#x}")]
    InvalidRawVirtualKey { raw: u16 },
    #[error("unknown virtual-key value {raw:#x}")]
    UnknownVirtualKey { raw: u16 },
}

#[cfg(test)]
mod tests {
    use super::{KeyCode, KeyCodeError};

    #[test]
    fn lowercase_letters_convert_to_uppercase_virtual_key() {
        assert_eq!(KeyCode::try_from_char('q'), Ok(KeyCode::Q));
        assert_eq!(KeyCode::try_from_char('Q'), Ok(KeyCode::Q));
    }

    #[test]
    fn digit_characters_convert_to_digit_virtual_key() {
        assert_eq!(KeyCode::try_from_char('7'), Ok(KeyCode::NUM_7));
    }

    #[test]
    fn unsupported_characters_fail_conversion() {
        assert_eq!(
            KeyCode::try_from_char('中'),
            Err(KeyCodeError::UnsupportedChar { ch: '中' })
        );
        assert_eq!(
            KeyCode::try_from_char('@'),
            Err(KeyCodeError::UnsupportedChar { ch: '@' })
        );
    }

    #[test]
    fn raw_virtual_key_escape_hatch_rejects_invalid_values() {
        assert_eq!(
            KeyCode::try_from_raw_virtual_key(0),
            Err(KeyCodeError::InvalidRawVirtualKey { raw: 0 })
        );
        assert_eq!(
            KeyCode::try_from_raw_virtual_key(0xff),
            Err(KeyCodeError::InvalidRawVirtualKey { raw: 0xff })
        );
        assert_eq!(KeyCode::try_from_raw_virtual_key(0x51), Ok(KeyCode::Q));
    }

    #[test]
    fn known_virtual_key_names_cover_avz2_representatives() {
        assert_eq!(KeyCode::LBUTTON.virtual_key_name(), Some("LBUTTON"));
        assert_eq!(KeyCode::Q.virtual_key_name(), Some("Q"));
        assert_eq!(KeyCode::NUMPAD0.virtual_key_name(), Some("NUMPAD0"));
        assert_eq!(KeyCode::F24.virtual_key_name(), Some("F24"));
        assert_eq!(KeyCode::OEM_1.virtual_key_name(), Some("OEM_1"));
        assert_eq!(KeyCode::LALT.virtual_key_name(), Some("LALT"));
    }

    #[test]
    fn known_virtual_key_conversion_accepts_only_known_map_entries() {
        assert_eq!(KeyCode::try_from_known_virtual_key(0x01), Ok(KeyCode::LBUTTON));
        assert_eq!(KeyCode::try_from_known_virtual_key(0x51), Ok(KeyCode::Q));
        assert_eq!(KeyCode::try_from_known_virtual_key(0xfe), Ok(KeyCode::OEM_CLEAR));

        assert_eq!(
            KeyCode::try_from_known_virtual_key(0x07),
            Err(KeyCodeError::UnknownVirtualKey { raw: 0x07 })
        );
        assert_eq!(
            KeyCode::try_from_known_virtual_key(0x0e),
            Err(KeyCodeError::UnknownVirtualKey { raw: 0x0e })
        );
        assert_eq!(
            KeyCode::try_from_known_virtual_key(0x3a),
            Err(KeyCodeError::UnknownVirtualKey { raw: 0x3a })
        );
        assert_eq!(
            KeyCode::try_from_known_virtual_key(0xff),
            Err(KeyCodeError::InvalidRawVirtualKey { raw: 0xff })
        );
    }

    #[test]
    fn raw_virtual_key_escape_hatch_stays_permissive_for_unknown_values() {
        let key = KeyCode::try_from_raw_virtual_key(0x07).expect("raw escape hatch accepts range");

        assert_eq!(key.raw_virtual_key(), 0x07);
        assert!(!key.is_known_virtual_key());
        assert_eq!(key.to_string(), "VK(0x07)");
    }
}
