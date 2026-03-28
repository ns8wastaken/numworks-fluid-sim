use crate::nadk::display::Color565;
use core::ops::{Add, Mul};

#[derive(Copy, Clone, Debug, Default)]
pub struct Rgb(pub u32);

impl Rgb {
    pub const ZERO: Self = Self(0);

    #[inline(always)]
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        let r10 = (r.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
        let g10 = (g.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
        let b10 = (b.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
        Self(r10 | (g10 << 10) | (b10 << 20))
    }

    #[inline(always)] pub fn r(&self) -> f32 { (self.0 & 0x3FF) as f32 / 1023.0 }
    #[inline(always)] pub fn g(&self) -> f32 { ((self.0 >> 10) & 0x3FF) as f32 / 1023.0 }
    #[inline(always)] pub fn b(&self) -> f32 { ((self.0 >> 20) & 0x3FF) as f32 / 1023.0 }

    pub fn to_color565(&self) -> Color565 {
        // Shift right to get roughly 8-bit values for from_rgb888
        let r = ((self.0 & 0x3FF) >> 2) as u16;
        let g = (((self.0 >> 10) & 0x3FF) >> 2) as u16;
        let b = (((self.0 >> 20) & 0x3FF) >> 2) as u16;
        Color565::from_rgb888(r, g, b)
    }

    /// Fast integer-based average for boundaries
    pub fn avg(&self, b: Self) -> Self {
        const AVG_MASK: u32 = !(1 << 9 | 1 << 19 | 1 << 29);
        Self(((self.0 >> 1) & AVG_MASK) + ((b.0 >> 1) & AVG_MASK))
    }
}

impl Add for Rgb {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        // Addition in f32 space to avoid bit-overflowing channels
        Self::new(self.r() + other.r(), self.g() + other.g(), self.b() + other.b())
    }
}

impl Mul<f32> for Rgb {
    type Output = Self;
    fn mul(self, scale: f32) -> Self {
        Self::new(self.r() * scale, self.g() * scale, self.b() * scale)
    }
}

// Bilinear interpolation: Rgb * f32 weight
impl Mul<Rgb> for f32 {
    type Output = Rgb;
    fn mul(self, rgb: Rgb) -> Rgb { rgb * self }
}
