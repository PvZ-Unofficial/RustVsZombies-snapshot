//! Timing helpers.

use crate::model::Wave;

/// 是否为旗帜波。
#[must_use]
pub const fn is_flag_wave(wave: Wave) -> bool {
    wave.0 > 0 && wave.0 % 10 == 0
}

/// 是否为大波前倒计时阶段。
#[must_use]
pub const fn is_huge_wave_countdown(countdown: i32) -> bool {
    countdown > 0 && countdown <= 750
}
