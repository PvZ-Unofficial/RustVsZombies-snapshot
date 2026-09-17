use super::*;

pub(super) fn fft_spectrum(values: &[f64], reverse: bool, plan: &FftPlan) -> FftSpectrum {
    let fft_len = plan.len();
    let mut real = vec![0.0_f32; fft_len];
    let mut imag = vec![0.0_f32; fft_len];
    if reverse {
        for (target, value) in real.iter_mut().zip(values.iter().rev()) {
            *target = *value as f32;
        }
    } else {
        for (target, value) in real.iter_mut().zip(values) {
            *target = *value as f32;
        }
    }
    fft(plan, &mut real, &mut imag, false);
    FftSpectrum { real, imag }
}
pub(super) fn correlate_spectra(
    left: &FftSpectrum, right: &FftSpectrum, result_len: usize, work_real: &mut [f32], work_imag: &mut [f32],
    plan: &FftPlan,
) -> Vec<f64> {
    debug_assert_eq!(left.real.len(), right.real.len());
    debug_assert_eq!(left.real.len(), work_real.len());
    debug_assert_eq!(work_real.len(), work_imag.len());
    for index in 0..work_real.len() {
        work_real[index] = left.real[index] * right.real[index] - left.imag[index] * right.imag[index];
        work_imag[index] = left.real[index] * right.imag[index] + left.imag[index] * right.real[index];
    }
    fft(plan, work_real, work_imag, true);
    (0..result_len)
        .map(|death| f64::from(work_real[result_len - 1 - death].max(0.0)))
        .collect()
}
pub(super) fn fft(plan: &FftPlan, real: &mut [f32], imag: &mut [f32], inverse: bool) {
    let count = real.len();
    debug_assert!(count.is_power_of_two());
    debug_assert_eq!(count, imag.len());
    for (index, reversed) in plan.bit_reversed.iter().copied().enumerate().skip(1) {
        let reversed = usize::from(reversed);
        if index < reversed {
            real.swap(index, reversed);
            imag.swap(index, reversed);
        }
    }
    if count >= 2 {
        for start in (0..count).step_by(2) {
            let even_real = real[start];
            let even_imag = imag[start];
            let odd_real = real[start + 1];
            let odd_imag = imag[start + 1];
            real[start] = even_real + odd_real;
            imag[start] = even_imag + odd_imag;
            real[start + 1] = even_real - odd_real;
            imag[start + 1] = even_imag - odd_imag;
        }
    }
    let mut width = 4;
    let mut twiddle_start = 1;
    while width <= count {
        for start in (0..count).step_by(width) {
            for offset in 0..width / 2 {
                let (twiddle_real, mut twiddle_imag) = plan.twiddles[twiddle_start + offset];
                if inverse {
                    twiddle_imag = -twiddle_imag;
                }
                let even = start + offset;
                let odd = even + width / 2;
                let odd_real = real[odd] * twiddle_real - imag[odd] * twiddle_imag;
                let odd_imag = real[odd] * twiddle_imag + imag[odd] * twiddle_real;
                real[odd] = real[even] - odd_real;
                imag[odd] = imag[even] - odd_imag;
                real[even] += odd_real;
                imag[even] += odd_imag;
            }
        }
        twiddle_start += width / 2;
        width *= 2;
    }
    if inverse {
        for (real, imag) in real.iter_mut().zip(imag) {
            *real /= count as f32;
            *imag /= count as f32;
        }
    }
}
