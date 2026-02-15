
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

    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.height }

    pub fn get_at(&self, x: usize, y: usize) -> FloatPixel {
        let index = y * self.width + x;
        if index < self.pixels.len() {
            self.pixels[index]
        } else {
            panic!("Attempted to get pixel at ({}, {}) out of bounds for image of size {}x{}", x, y, self.width, self.height);
        }
    }

    #[inline]
    pub fn set_at(&mut self, x: usize, y: usize, pixel: FloatPixel) {
        let index = y * self.width + x;
        if index < self.pixels.len() {
            self.pixels[index] = pixel;
        } else {
            panic!("Attempted to set pixel at ({}, {}) out of bounds for image of size {}x{}", x, y, self.width, self.height);
        }
    }

    pub fn set_with_fn<F: Fn(usize, usize) -> FloatPixel>(&mut self, f: F) {
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = f(x, y);
                self.set_at(x, y, pixel);
            }
        }
    }

    /// Fetches a pixel at the given coordinates (x, y).
    pub fn fetch_pixel(
        &self,
        x: usize,
        y: usize,
    ) -> FloatPixel {
        let pixel_index = (y * self.width + x) * 3;

        self.pixels[pixel_index]
    }

    /// Samples a pixel coordinate using bilinear filtering and clamp addressing.
    /// U and V are in [0, 1].
    pub fn sample_bilinear(&self, u: f32, v: f32) -> FloatPixel {
        sample_bilinear_with(self.width, self.height, u, v, |x, y| {
            let x = x.min(self.width - 1);
            let y = y.min(self.height - 1);
            self.get_at(x, y)
        })
    }
}

/// Bilinear sampling with clamp addressing over a pixel grid of the given dimensions.
///
/// `u` and `v` are normalised coordinates in [0, 1].
/// `fetch` returns a pixel at integer coordinates (x, y); it may receive coordinates
/// up to `(width, height)`, so callers should clamp if needed.
pub fn sample_bilinear_with<F>(width: usize, height: usize, u: f32, v: f32, fetch: F) -> FloatPixel
where
    F: Fn(usize, usize) -> FloatPixel,
{
    let w = width as f32;
    let h = height as f32;

    let x = u * w;
    let y = v * h;

    let base_x = if x.fract() < 0.5 { x.floor() - 1.0 } else { x.floor() };
    let base_y = if y.fract() < 0.5 { y.floor() - 1.0 } else { y.floor() };

    let base_x = (base_x as isize).clamp(0, width as isize - 1) as usize;
    let base_y = (base_y as isize).clamp(0, height as isize - 1) as usize;

    let next_x = (base_x + 1).min(width - 1);
    let next_y = (base_y + 1).min(height - 1);

    let p00 = fetch(base_x, base_y);
    let p10 = fetch(next_x, base_y);
    let p01 = fetch(base_x, next_y);
    let p11 = fetch(next_x, next_y);

    let s = (x - base_x as f32).clamp(0.0, 1.0);
    let t = (y - base_y as f32).clamp(0.0, 1.0);

    fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

    FloatPixel::new(
        lerp(lerp(p00.r(), p10.r(), s), lerp(p01.r(), p11.r(), s), t),
        lerp(lerp(p00.g(), p10.g(), s), lerp(p01.g(), p11.g(), s), t),
        lerp(lerp(p00.b(), p10.b(), s), lerp(p01.b(), p11.b(), s), t),
    )
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

