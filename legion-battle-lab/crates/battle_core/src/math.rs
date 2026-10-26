use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Sub};

/// A ground-plane position: `x` maps to world X, `y` maps to world Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn normalized_or_zero(self) -> Self {
        let length = self.length();
        if length > 0.00001 {
            self / length
        } else {
            Self::ZERO
        }
    }

    pub fn distance_squared(self, other: Self) -> f32 {
        (self - other).length_squared()
    }

    pub fn lerp(self, other: Self, amount: f32) -> Self {
        self + (other - self) * amount
    }

    pub fn clamp_length(self, maximum: f32) -> Self {
        let length_squared = self.length_squared();
        if length_squared > maximum * maximum {
            self * (maximum / length_squared.sqrt())
        } else {
            self
        }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Point {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}
impl Div<f32> for Point {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}
