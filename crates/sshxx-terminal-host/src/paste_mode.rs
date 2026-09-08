//! Bounded DEC bracketed-paste checkpoint, not a screen or scrollback emulator.

const GROUND: u8 = 0;
const ESCAPE: u8 = 1;
const CSI: u8 = 2;
const OSC: u8 = 3;
const SOFT_RESET: u8 = 4;
const STRING: u8 = 5;
const ESC_INTERMEDIATE: u8 = 6;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasteMode {
    enabled: Option<bool>,
    state: u8,
    private: bool,
    matched: bool,
    number: u16,
    digits: bool,
    invalid: bool,
    utf8_c1: bool,
}

impl PasteMode {
    pub fn fresh() -> Self {
        Self {
            enabled: Some(false),
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> Option<bool> {
        self.enabled
    }

    pub fn checkpoint(&self) -> Vec<u8> {
        vec![
            1,
            self.enabled.map_or(0, |v| if v { 2 } else { 1 }),
            self.state,
            self.private as u8,
            self.matched as u8,
            (self.number >> 8) as u8,
            self.number as u8,
            self.digits as u8,
            self.invalid as u8,
            self.utf8_c1 as u8,
        ]
    }

    pub fn restore(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 10
            || bytes[0] != 1
            || bytes[1] > 2
            || bytes[2] > ESC_INTERMEDIATE
            || [3, 4, 7, 8, 9].iter().any(|&i| bytes[i] > 1)
        {
            return None;
        }
        Some(Self {
            enabled: match bytes[1] {
                1 => Some(false),
                2 => Some(true),
                _ => None,
            },
            state: bytes[2],
            private: bytes[3] != 0,
            matched: bytes[4] != 0,
            number: u16::from_be_bytes([bytes[5], bytes[6]]),
            digits: bytes[7] != 0,
            invalid: bytes[8] != 0,
            utf8_c1: bytes[9] != 0,
        })
    }

    fn csi(&mut self) {
        self.state = CSI;
        self.private = false;
        self.matched = false;
        self.number = 0;
        self.digits = false;
        self.invalid = false;
    }

    pub fn feed(&mut self, bytes: impl IntoIterator<Item = u8>) {
        for byte in bytes {
            // xterm decodes UTF-8 before parsing C1 controls. Do not mistake
            // continuation bytes in ordinary non-ASCII text for raw C1 codes.
            let c1 = self.utf8_c1 && (0x80..=0x9f).contains(&byte);
            self.utf8_c1 = byte == 0xc2;
            if self.utf8_c1 {
                continue;
            }
            if c1 {
                match byte {
                    0x9b => self.csi(),
                    0x9d => self.state = OSC,
                    0x90 | 0x98 | 0x9e | 0x9f => self.state = STRING,
                    _ => self.state = GROUND,
                }
                continue;
            }
            // ESC terminates OSC/DCS as in xterm, and begins a new sequence.
            if byte == 0x1b {
                self.state = ESCAPE;
                continue;
            }
            if byte == 0x18 || byte == 0x1a {
                self.state = GROUND;
                continue;
            }
            if byte < 0x20 || byte == 0x7f {
                if self.state == OSC && byte == 7 {
                    self.state = GROUND;
                }
                continue;
            }
            match self.state {
                OSC | STRING => {}
                ESCAPE => match byte {
                    b'[' => self.csi(),
                    b']' => self.state = OSC,
                    b'P' | b'_' | b'^' | b'X' => self.state = STRING,
                    b'c' => {
                        self.enabled = Some(false);
                        self.state = GROUND;
                    }
                    0x20..=0x2f => self.state = ESC_INTERMEDIATE,
                    _ => self.state = GROUND,
                },
                ESC_INTERMEDIATE => {
                    if byte >= 0x30 {
                        self.state = GROUND;
                    }
                }
                SOFT_RESET => {
                    if byte == b'p' {
                        self.enabled = Some(false);
                    }
                    self.state = GROUND;
                }
                CSI => match byte {
                    b'?' if !self.private && !self.digits && !self.matched && !self.invalid => {
                        self.private = true
                    }
                    b'!' if !self.private && !self.invalid => self.state = SOFT_RESET,
                    b'0'..=b'9' => {
                        self.digits = true;
                        self.number = self
                            .number
                            .saturating_mul(10)
                            .saturating_add((byte - b'0') as u16);
                    }
                    b';' => {
                        self.invalid |= !self.private;
                        self.matched |= self.number == 2004 && self.digits;
                        self.number = 0;
                        self.digits = false;
                    }
                    0x40..=0x7e => {
                        if self.private
                            && !self.invalid
                            && (self.matched || self.number == 2004)
                            && (byte == b'h' || byte == b'l')
                        {
                            self.enabled = Some(byte == b'h');
                        }
                        self.state = GROUND;
                    }
                    _ => self.invalid = true,
                },
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_match_xterm_negotiation_across_every_byte_split() {
        let cases: &[(&[u8], bool)] = &[
            (b"\x1b[?1;2004h", true),
            (b"\x1b[?2004h\x1b]title [?2004l\x07", true),
            (b"\x1b[?2004h\x1bPdata [?2004l\x1b\\", true),
            (b"\x1b[?2004h\x1b]title \x1b[?2004l\x07", false),
            (b"\x1b[?2004h\x1bP\x1b[?2004l\x1b\\", false),
            (b"\x1b[?2004h\x1b[!p", false),
            (b"\x1b[?2004h\x1bc", false),
            (b"\xc2\x9b?2004h", true),
            (b"\x1b[?999999;2004h", true),
            (b"\x1b[;?2004h", false),
        ];
        for &(bytes, expected) in cases {
            for split in 0..=bytes.len() {
                let mut mode = PasteMode::fresh();
                mode.feed(bytes[..split].iter().copied());
                let mut restored = PasteMode::restore(&mode.checkpoint()).unwrap();
                restored.feed(bytes[split..].iter().copied());
                assert_eq!(
                    restored.enabled(),
                    Some(expected),
                    "{bytes:?} split {split}"
                );
            }
        }
        assert!(PasteMode::restore(&[1; 8]).is_none());
        let mut unknown = PasteMode::default();
        unknown.feed(b"ordinary text".iter().copied());
        assert_eq!(unknown.enabled(), None);
    }
}
