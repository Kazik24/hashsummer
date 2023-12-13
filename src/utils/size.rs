use std::fmt::{Debug, Display, Formatter};
use std::ops::{Add, AddAssign, Sub, SubAssign};

#[derive(Default, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ByteSize(pub u64);

impl From<u64> for ByteSize {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
impl From<u32> for ByteSize {
    fn from(value: u32) -> Self {
        Self(value as _)
    }
}
impl From<usize> for ByteSize {
    fn from(value: usize) -> Self {
        Self(value as _)
    }
}

impl Add for ByteSize {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for ByteSize {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}
impl AddAssign<u64> for ByteSize {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs
    }
}
impl SubAssign for ByteSize {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0
    }
}
impl SubAssign<u64> for ByteSize {
    fn sub_assign(&mut self, rhs: u64) {
        self.0 -= rhs
    }
}

impl Sub for ByteSize {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl ByteSize {
    pub fn wrapping_add(self, rhs: u64) -> Self {
        Self(self.0.wrapping_add(rhs))
    }
    pub fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }
    pub fn checked_add(self, rhs: u64) -> Option<Self> {
        Some(Self(self.0.checked_add(rhs)?))
    }
    pub fn wrapping_sub(self, rhs: u64) -> Self {
        Self(self.0.wrapping_sub(rhs))
    }
    pub fn saturating_sub(self, rhs: u64) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
    pub fn checked_sub(self, rhs: u64) -> Option<Self> {
        Some(Self(self.0.checked_sub(rhs)?))
    }
}

impl Debug for ByteSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (frac, prefix) = match self.0 {
            v if v < 1024 => (v as f64, ""),
            v if v < 1024u64.pow(2) => (v as f64 / 1024u64 as f64, "K"),
            v if v < 1024u64.pow(3) => (v as f64 / 1024u64.pow(2) as f64, "M"),
            v if v < 1024u64.pow(4) => (v as f64 / 1024u64.pow(3) as f64, "G"),
            v if v < 1024u64.pow(5) => (v as f64 / 1024u64.pow(4) as f64, "T"),
            v if v < 1024u64.pow(6) => (v as f64 / 1024u64.pow(5) as f64, "P"),
            v => (v as f64 / 1024u64.pow(6) as f64, "E"),
        };
        if f.precision().is_none() {
            write!(f, "{frac:.3?}")?;
        } else {
            Display::fmt(&frac, f)?;
        }
        write!(f, " {prefix}B")
    }
}

impl Display for ByteSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}
