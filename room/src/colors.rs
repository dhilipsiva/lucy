//! QUINE colour tokens (dhilipsiva.dev, `tokens/colors.css`), as the hand-off names them. The
//! renderer works in linear light, so each sRGB hex is converted once.

pub const VOID_1000: u32 = 0x07070A;
pub const VOID_900: u32 = 0x0B0B10;
pub const VOID_800: u32 = 0x111119;
pub const VOID_700: u32 = 0x181822;
pub const VOID_600: u32 = 0x20202C;
pub const VOID_500: u32 = 0x2A2A38;
pub const VOID_400: u32 = 0x3A3A4A;
pub const PAPER_100: u32 = 0xF4F3EF;
pub const PAPER_200: u32 = 0xC9C8C2;
pub const FG_MUTED: u32 = 0x908F9A;
pub const FG_FAINT: u32 = 0x8B8A95;
pub const FG_GHOST: u32 = 0x3C3C49;
pub const EMBER_300: u32 = 0xFF9A6F;
pub const EMBER_500: u32 = 0xF2542D;
pub const EMBER_700: u32 = 0xB23413;
pub const PHOSPHOR_300: u32 = 0x8BF0CE;
pub const PHOSPHOR_500: u32 = 0x38E3A6;
pub const PHOSPHOR_700: u32 = 0x16A877;
pub const QUANTA_300: u32 = 0xB6A8FF;
pub const QUANTA_500: u32 = 0x8B7CF6;
pub const AMBER_500: u32 = 0xF5B544;
pub const SKY_500: u32 = 0x4DB8FF;
pub const CRIMSON_500: u32 = 0xFF4D5E;

/// `#rrggbb` for the canvas, the way CSS writes it.
pub fn css(hex: u32) -> String {
    format!("#{:06X}", hex)
}
/// `rgba(r,g,b,a)` for the canvas.
pub fn css_a(hex: u32, a: f32) -> String {
    format!(
        "rgba({},{},{},{})",
        (hex >> 16) & 255,
        (hex >> 8) & 255,
        hex & 255,
        a
    )
}
/// sRGB hex to linear RGB, for the renderer.
pub fn linear(hex: u32) -> [f32; 3] {
    let c = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [c((hex >> 16) & 255), c((hex >> 8) & 255), c(hex & 255)]
}

/// The verdict colours: TRUE phosphor, FALSE sky, ERROR crimson, words violet, recall neutral, eyes phosphor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    True,
    False,
    Error,
    Words,
    Recall,
    Eyes,
}
impl Verdict {
    pub fn line(self) -> u32 {
        match self {
            Verdict::True | Verdict::Eyes => PHOSPHOR_500,
            Verdict::False => SKY_500,
            Verdict::Error => CRIMSON_500,
            Verdict::Words => QUANTA_500,
            Verdict::Recall => VOID_400,
        }
    }
    pub fn fg(self) -> u32 {
        match self {
            Verdict::True | Verdict::Eyes => PHOSPHOR_300,
            Verdict::False => SKY_500,
            Verdict::Error => CRIMSON_500,
            Verdict::Words => QUANTA_300,
            Verdict::Recall => FG_MUTED,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Verdict::True => "TRUE",
            Verdict::False => "FALSE · not derivable",
            Verdict::Error => "ERROR",
            Verdict::Words => "IN WORDS · not a verdict",
            Verdict::Recall => "RECALL",
            Verdict::Eyes => "EYES",
        }
    }
    /// TRUE and EYES carry the glowing dot.
    pub fn dot(self) -> bool {
        matches!(self, Verdict::True | Verdict::Eyes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_round_trips_to_css() {
        assert_eq!(css(EMBER_500), "#F2542D");
        assert_eq!(css_a(VOID_900, 0.84), "rgba(11,11,16,0.84)");
    }
    #[test]
    fn linear_is_monotone_and_bounded() {
        let w = linear(0xFFFFFF);
        assert!((w[0] - 1.0).abs() < 1e-6);
        let v = linear(VOID_900);
        assert!(v[0] > 0.0 && v[0] < 0.01);
    }
}
