//! Compact classic-wave input model.

use std::ops::{Range, RangeInclusive};

const MIN_WAVE: i32 = 1;
const MAX_WAVE: i32 = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WaveSet {
    bits: u32,
}

impl WaveSet {
    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub fn single(wave: i32) -> Result<Self, String> {
        let mut set = Self::empty();
        set.insert(wave)?;
        Ok(set)
    }

    pub fn from_waves(input: impl IntoIterator<Item = i32>) -> Result<Self, String> {
        let mut set = Self::empty();
        for wave in input {
            set.insert(wave)?;
        }
        if set.is_empty() {
            return Err("wave selection cannot be empty".to_owned());
        }
        Ok(set)
    }

    fn insert(&mut self, wave: i32) -> Result<(), String> {
        if !(MIN_WAVE..=MAX_WAVE).contains(&wave) {
            return Err(format!("wave must be in 1..=20, got {wave}"));
        }
        self.bits |= 1_u32 << (wave - 1);
        Ok(())
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.bits
    }

    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    pub const fn iter(self) -> WaveSetIter {
        WaveSetIter {
            bits: self.bits,
            next: MIN_WAVE,
        }
    }
}

pub struct WaveSetIter {
    bits: u32,
    next: i32,
}

impl Iterator for WaveSetIter {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next <= MAX_WAVE {
            let wave = self.next;
            self.next += 1;
            if self.bits & (1_u32 << (wave - 1)) != 0 {
                return Some(wave);
            }
        }
        None
    }
}

pub trait IntoWaveSet {
    fn into_wave_set(self) -> Result<WaveSet, String>;
}

impl IntoWaveSet for WaveSet {
    fn into_wave_set(self) -> Result<WaveSet, String> {
        (!self.is_empty())
            .then_some(self)
            .ok_or_else(|| "wave selection cannot be empty".to_owned())
    }
}

impl IntoWaveSet for i32 {
    fn into_wave_set(self) -> Result<WaveSet, String> {
        WaveSet::single(self)
    }
}

impl<const N: usize> IntoWaveSet for [i32; N] {
    fn into_wave_set(self) -> Result<WaveSet, String> {
        WaveSet::from_waves(self)
    }
}

impl IntoWaveSet for &[i32] {
    fn into_wave_set(self) -> Result<WaveSet, String> {
        WaveSet::from_waves(self.iter().copied())
    }
}

impl IntoWaveSet for RangeInclusive<i32> {
    fn into_wave_set(self) -> Result<WaveSet, String> {
        WaveSet::from_waves(self)
    }
}

impl IntoWaveSet for Range<i32> {
    fn into_wave_set(self) -> Result<WaveSet, String> {
        WaveSet::from_waves(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_inputs_preserve_sorted_unique_waves() {
        let slice: &[i32] = &[2, 5];
        assert_eq!(1.into_wave_set().expect("single").iter().collect::<Vec<_>>(), [1]);
        assert_eq!(
            [1, 4, 7].into_wave_set().expect("array").iter().collect::<Vec<_>>(),
            [1, 4, 7]
        );
        assert_eq!(slice.into_wave_set().expect("slice").iter().collect::<Vec<_>>(), [2, 5]);
        assert_eq!(
            (3..=5).into_wave_set().expect("range").iter().collect::<Vec<_>>(),
            [3, 4, 5]
        );
    }
}
