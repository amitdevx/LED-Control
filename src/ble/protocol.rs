// 9-byte packet encoder for LEDDMX controllers.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorOrder {
    Rgb,
    Rbg,
    Grb,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrder {
    pub fn reorder(&self, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        match self {
            ColorOrder::Rgb => (r, g, b),
            ColorOrder::Rbg => (r, b, g),
            ColorOrder::Grb => (g, r, b),
            ColorOrder::Gbr => (g, b, r),
            ColorOrder::Brg => (b, r, g),
            ColorOrder::Bgr => (b, g, r),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ColorOrder::Rgb => "RGB",
            ColorOrder::Rbg => "RBG (One94Store Default)",
            ColorOrder::Grb => "GRB",
            ColorOrder::Gbr => "GBR",
            ColorOrder::Brg => "BRG",
            ColorOrder::Bgr => "BGR",
        }
    }

    pub fn from_str_name(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "RGB" => ColorOrder::Rgb,
            "GRB" => ColorOrder::Grb,
            "GBR" => ColorOrder::Gbr,
            "BRG" => ColorOrder::Brg,
            "BGR" => ColorOrder::Bgr,
            _ => ColorOrder::Rbg,
        }
    }

    pub fn all() -> &'static [ColorOrder] {
        &[
            ColorOrder::Rbg,
            ColorOrder::Rgb,
            ColorOrder::Grb,
            ColorOrder::Gbr,
            ColorOrder::Brg,
            ColorOrder::Bgr,
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    StaticColor { r: u8, g: u8, b: u8 },
    PowerOn,
    PowerOff,
    Brightness(u8),
    Pattern(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidBrightness(u8),
    InvalidPattern(u8),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBrightness(value) => {
                write!(
                    formatter,
                    "brightness must be between 0 and 100, got {value}"
                )
            }
            Self::InvalidPattern(value) => {
                write!(
                    formatter,
                    "pattern index must be between 0 and 210, got {value}"
                )
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

pub struct Protocol {
    color_order: ColorOrder,
}

impl Protocol {
    pub fn new(color_order: ColorOrder) -> Self {
        Self { color_order }
    }

    pub fn set_color_order(&mut self, order: ColorOrder) {
        self.color_order = order;
    }

    pub fn encode(&self, cmd: &Command) -> Result<[u8; 9], ProtocolError> {
        match cmd {
            Command::StaticColor { r, g, b } => {
                let (c1, c2, c3) = self.color_order.reorder(*r, *g, *b);
                Ok([0x7B, 0xFF, 0x07, c1, c2, c3, 0x00, 0xFF, 0xBF])
            }
            Command::PowerOn => Ok([0x7B, 0xFF, 0x04, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF]),
            Command::PowerOff => Ok([0x7B, 0xFF, 0x04, 0x02, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF]),
            Command::Brightness(pct) => {
                if *pct > 100 {
                    return Err(ProtocolError::InvalidBrightness(*pct));
                }
                let scaled = ((*pct as u32) * 32 / 100) as u8;
                Ok([0x7B, 0xFF, 0x01, scaled, *pct, 0x00, 0xFF, 0xFF, 0xBF])
            }
            Command::Pattern(idx) => {
                if *idx > 210 {
                    return Err(ProtocolError::InvalidPattern(*idx));
                }
                Ok([0x7B, 0xFF, 0x03, *idx, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_color_rbg_order() {
        let proto = Protocol::new(ColorOrder::Rbg);
        assert_eq!(
            proto
                .encode(&Command::StaticColor { r: 0, g: 255, b: 0 })
                .unwrap(),
            [0x7B, 0xFF, 0x07, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0xBF]
        );
    }

    #[test]
    fn test_power_frames() {
        let proto = Protocol::new(ColorOrder::Rbg);
        assert_eq!(
            proto.encode(&Command::PowerOn).unwrap(),
            [0x7B, 0xFF, 0x04, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF]
        );
        assert_eq!(
            proto.encode(&Command::PowerOff).unwrap(),
            [0x7B, 0xFF, 0x04, 0x02, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF]
        );
    }

    #[test]
    fn test_static_color_is_not_mutated() {
        let proto = Protocol::new(ColorOrder::Rgb);
        assert_eq!(
            proto
                .encode(&Command::StaticColor {
                    r: 255,
                    g: 255,
                    b: 255
                })
                .unwrap(),
            [0x7B, 0xFF, 0x07, 255, 255, 255, 0x00, 0xFF, 0xBF]
        );
    }

    #[test]
    fn test_all_channel_orders() {
        let expected = [
            (ColorOrder::Rgb, [10, 20, 30]),
            (ColorOrder::Rbg, [10, 30, 20]),
            (ColorOrder::Grb, [20, 10, 30]),
            (ColorOrder::Gbr, [20, 30, 10]),
            (ColorOrder::Brg, [30, 10, 20]),
            (ColorOrder::Bgr, [30, 20, 10]),
        ];
        for (order, channels) in expected {
            let frame = Protocol::new(order)
                .encode(&Command::StaticColor {
                    r: 10,
                    g: 20,
                    b: 30,
                })
                .unwrap();
            assert_eq!(&frame[3..6], channels);
        }
    }

    #[test]
    fn test_brightness_frames_and_boundaries() {
        let proto = Protocol::new(ColorOrder::Rgb);
        assert_eq!(
            proto.encode(&Command::Brightness(0)).unwrap(),
            [0x7B, 0xFF, 0x01, 0, 0, 0, 0xFF, 0xFF, 0xBF]
        );
        assert_eq!(
            proto.encode(&Command::Brightness(50)).unwrap(),
            [0x7B, 0xFF, 0x01, 16, 50, 0x00, 0xFF, 0xFF, 0xBF]
        );
        assert_eq!(
            proto.encode(&Command::Brightness(100)).unwrap(),
            [0x7B, 0xFF, 0x01, 32, 100, 0x00, 0xFF, 0xFF, 0xBF]
        );
        assert_eq!(
            proto.encode(&Command::Brightness(101)),
            Err(ProtocolError::InvalidBrightness(101))
        );
    }

    #[test]
    fn test_pattern_frames_and_boundaries() {
        let proto = Protocol::new(ColorOrder::Rgb);
        assert_eq!(
            proto.encode(&Command::Pattern(0)).unwrap(),
            [0x7B, 0xFF, 0x03, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF]
        );
        assert_eq!(
            proto.encode(&Command::Pattern(210)).unwrap(),
            [0x7B, 0xFF, 0x03, 210, 0xFF, 0xFF, 0xFF, 0xFF, 0xBF]
        );
        assert_eq!(
            proto.encode(&Command::Pattern(211)),
            Err(ProtocolError::InvalidPattern(211))
        );
    }
}
