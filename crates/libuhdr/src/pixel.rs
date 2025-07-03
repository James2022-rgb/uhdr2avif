
use std::ops;

#[derive(Clone)]
pub struct FloatImageContent {
    width: usize,
    height: usize,
    /// Row-major pixel data in linear RGB format.
    pixels: Vec<FloatPixel>,
}

impl FloatImageContent {
    pub fn with_extent(width: usize, height: usize) -> Self {
        let pixel_count = width * height;
        let pixels = vec![FloatPixel::zero(); pixel_count];
        Self { width, height, pixels }
    }

    pub fn get_at(&self, x: usize, y: usize) -> FloatPixel {
        let index = y * self.width + x;
        if index < self.pixels.len() {
            self.pixels[index]
        } else {
            panic!("Attempted to get pixel at ({}, {}) out of bounds for image of size {}x{}", x, y, self.width, self.height);
        }
    }

    pub fn set_at(&mut self, x: usize, y: usize, pixel: FloatPixel) {
        let index = y * self.width + x;
        if index < self.pixels.len() {
            self.pixels[index] = pixel;
        } else {
            panic!("Attempted to set pixel at ({}, {}) out of bounds for image of size {}x{}", x, y, self.width, self.height);
        }
    }
}

/// A pixel with 4 elements, where the last element is padding for 4-element, 16-byte alignment.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatPixel {
    /// The last element is padding for 4-element alignment.
    /// It is not used in any calculations.
    inner: [f32; 4],
}

impl From<[f32; 3]> for FloatPixel {
    fn from(inner: [f32; 3]) -> Self {
        Self { inner: [inner[0], inner[1], inner[2], 0.0] }
    }
}

impl ops::Index<usize> for FloatPixel {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[index]
    }
}

impl ops::IndexMut<usize> for FloatPixel {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.inner[index]
    }
}

impl ops::Add for FloatPixel {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self {
            inner: [
                self.inner[0] + other.inner[0],
                self.inner[1] + other.inner[1],
                self.inner[2] + other.inner[2],
                0.0,
            ],
        }
    }
}

impl ops::Sub for FloatPixel {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self {
            inner: [
                self.inner[0] - other.inner[0],
                self.inner[1] - other.inner[1],
                self.inner[2] - other.inner[2],
                0.0,
            ],
        }
    }
}

impl ops::Mul<f32> for FloatPixel {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self::Output {
        Self {
            inner: [
                self.inner[0] * scalar,
                self.inner[1] * scalar,
                self.inner[2] * scalar,
                0.0,
            ],
        }
    }
}

impl ops::Mul for FloatPixel {
    type Output = Self;

    fn mul(self, other: Self) -> Self::Output {
        Self {
            inner: [
                self.inner[0] * other.inner[0],
                self.inner[1] * other.inner[1],
                self.inner[2] * other.inner[2],
                0.0,
            ],
        }
    }
}

impl ops::Div<f32> for FloatPixel {
    type Output = Self;

    fn div(self, scalar: f32) -> Self::Output {
        Self {
            inner: [
                self.inner[0] / scalar,
                self.inner[1] / scalar,
                self.inner[2] / scalar,
                0.0,
            ],
        }
    }
}

impl ops::Div for FloatPixel {
    type Output = Self;

    fn div(self, other: Self) -> Self::Output {
        Self {
            inner: [
                self.inner[0] / other.inner[0],
                self.inner[1] / other.inner[1],
                self.inner[2] / other.inner[2],
                0.0,
            ],
        }
    }
}

impl FloatPixel {
    pub const fn zero() -> Self {
        Self { inner: [0.0, 0.0, 0.0, 0.0] }
    }

    pub const fn one() -> Self {
        Self { inner: [1.0, 1.0, 1.0, 0.0] }
    }

    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { inner: [r, g, b, 0.0] }
    }

    #[inline]
    pub fn rgb(&self) -> &[f32; 3] {
        unsafe { &*(self.inner.as_ptr() as *const [f32; 3]) }
    }

    #[inline]
    pub fn r(&self) -> f32 {
        self.inner[0]
    }

    #[inline]
    pub fn g(&self) -> f32 {
        self.inner[1]
    }

    #[inline]
    pub fn b(&self) -> f32 {
        self.inner[2]
    }

    #[inline]
    pub fn powf(lhs: &Self, rhs: &Self) -> Self {
        Self {
            inner: [
                f32::powf(lhs.inner[0], rhs.inner[0]),
                f32::powf(lhs.inner[1], rhs.inner[1]),
                f32::powf(lhs.inner[2], rhs.inner[2]),
                0.0,
            ],
        }
    }

    #[inline]
    pub fn rcp(&self) -> Self {
        Self {
            inner: [
                1.0 / self.inner[0],
                1.0 / self.inner[1],
                1.0 / self.inner[2],
                0.0,
            ],
        }
    }

    #[inline]
    pub fn exp2(&self) -> Self {
        Self {
            inner: [
                f32::exp2(self.inner[0]),
                f32::exp2(self.inner[1]),
                f32::exp2(self.inner[2]),
                0.0,
            ],
        }
    }
}

