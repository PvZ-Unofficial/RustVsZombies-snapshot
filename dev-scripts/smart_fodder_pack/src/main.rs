use std::env;
use std::fs;
use std::path::PathBuf;

const MAGIC: &[u8; 8] = b"RSVZSF01";
const FORMAT_VERSION: u16 = 1;
const H_MAX: u16 = 1_142;
const X_MIN: f32 = 620.0;
const X_STEP: f32 = 3.0;
const X_COUNT: u16 = 35;
const H_COUNT: usize = H_MAX as usize + 1;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("usage: smart-fodder-pack INPUT.json OUTPUT.bin")?);
    let output = PathBuf::from(args.next().ok_or("usage: smart-fodder-pack INPUT.json OUTPUT.bin")?);
    let allow_smoke = args.next().is_some_and(|argument| argument == "--allow-smoke");
    if args.next().is_some() {
        return Err("usage: smart-fodder-pack INPUT.json OUTPUT.bin [--allow-smoke]".into());
    }

    let json = fs::read_to_string(input)?;
    require_root_value(&json, "format_version", u64::from(FORMAT_VERSION))?;
    require_root_value(&json, "trials_per_case", if allow_smoke { 1 } else { 50_000 })?;
    require_root_value(&json, "h_max", u64::from(H_MAX))?;
    require_root_value(&json, "x_count", u64::from(X_COUNT))?;
    require_root_float(&json, "x_min", f64::from(X_MIN))?;
    require_root_float(&json, "x_step", f64::from(X_STEP))?;
    if !allow_smoke {
        require_recorded_commit(&json, "rsvz_source_commit")?;
        require_recorded_commit(&json, "pvz_emulator_source_commit")?;
    }

    let release_section = section(&json, "release_tail", "jack_position_tail")?;
    let jack_section = section(&json, "jack_position_tail", "pole_tail")?;
    let pole_section = section_to_end(&json, "pole_tail")?;
    let release = derived_means(release_section, "sum_damage", allow_smoke)?;
    let jack = derived_means(jack_section, "danger_count", allow_smoke)?;
    let pole = derived_means(pole_section, "sum_damage", allow_smoke)?;
    let release_count = 3 * usize::from(X_COUNT) * H_COUNT;
    let jack_count = 5 * usize::from(X_COUNT) * H_COUNT;
    if release.len() != release_count || jack.len() != jack_count || pole.len() != H_COUNT {
        return Err(format!(
            "unexpected table sizes: release={}, jack={}, pole={}",
            release.len(),
            jack.len(),
            pole.len()
        )
        .into());
    }
    validate_range("release", &release, 0.0, 300.0)?;
    validate_range("jack", &jack, 0.0, 1.0)?;
    validate_range("pole", &pole, 0.0, 300.0)?;
    validate_axes(release_section, jack_section, pole_section)?;

    let value_count = release.len() + pole.len() + jack.len();
    let mut bytes = Vec::with_capacity(60 + value_count * 4);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&H_MAX.to_le_bytes());
    for _ in 0..4 {
        bytes.extend_from_slice(&X_MIN.to_le_bytes());
        bytes.extend_from_slice(&X_STEP.to_le_bytes());
        bytes.extend_from_slice(&X_COUNT.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
    }
    append_f32(&mut bytes, &release);
    append_f32(&mut bytes, &pole);
    append_f32(&mut bytes, &jack);
    fs::write(output, bytes)?;
    Ok(())
}

fn require_root_float(json: &str, key: &str, expected: f64) -> Result<(), Box<dyn std::error::Error>> {
    let value = field_f64_values(json, key)?
        .into_iter()
        .next()
        .ok_or_else(|| format!("missing {key}"))?;
    if value != expected {
        return Err(format!("{key} is {value}, expected {expected}").into());
    }
    Ok(())
}

fn require_recorded_commit(json: &str, key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let marker = format!("\"{key}\":\"");
    let value = json
        .split_once(&marker)
        .and_then(|(_before, tail)| tail.split_once('"'))
        .map(|(value, _tail)| value)
        .ok_or_else(|| format!("missing {key}"))?;
    if value == "unrecorded" || value.len() < 7 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(format!("{key} must be a recorded hexadecimal commit").into());
    }
    Ok(())
}

fn require_root_value(json: &str, key: &str, expected: u64) -> Result<(), Box<dyn std::error::Error>> {
    let marker = format!("\"{key}\":");
    let tail = json.split_once(&marker).ok_or_else(|| format!("missing {key}"))?.1;
    let value = tail
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .ok_or_else(|| format!("invalid {key}"))?
        .parse::<u64>()?;
    if value != expected {
        return Err(format!("{key} is {value}, expected {expected}").into());
    }
    Ok(())
}

fn section<'a>(json: &'a str, key: &str, next: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let start = json
        .find(&format!("\"{key}\":["))
        .ok_or_else(|| format!("missing {key}"))?;
    let end = json[start..]
        .find(&format!("],\"{next}\":"))
        .map(|offset| start + offset + 1)
        .ok_or_else(|| format!("unterminated {key}"))?;
    Ok(&json[start..end])
}

fn section_to_end<'a>(json: &'a str, key: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let start = json
        .find(&format!("\"{key}\":["))
        .ok_or_else(|| format!("missing {key}"))?;
    Ok(&json[start..])
}

fn derived_means(section: &str, sum_key: &str, allow_smoke: bool) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let counts = field_u64_values(section, "n")?;
    let sums = field_f64_values(section, sum_key)?;
    let declared = field_f64_values(section, "mean")?;
    if counts.len() != sums.len() || sums.len() != declared.len() || counts.is_empty() {
        return Err(format!("mismatched n/{sum_key}/mean field counts").into());
    }
    let mut means = Vec::with_capacity(counts.len());
    for ((count, sum), declared) in counts.into_iter().zip(sums).zip(declared) {
        if count == 0 || (!allow_smoke && count != 50_000) {
            return Err("every formal table entry must contain n=50000".into());
        }
        let derived = sum / count as f64;
        if !derived.is_finite() || (derived - declared).abs() > 1.0e-9 * derived.abs().max(1.0) {
            return Err(format!("declared mean {declared} does not match {sum_key}/n={derived}").into());
        }
        means.push(derived as f32);
    }
    Ok(means)
}

fn field_u64_values(json: &str, key: &str) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    let marker = format!("\"{key}\":");
    let mut values = Vec::new();
    let mut rest = json;
    while let Some((_before, tail)) = rest.split_once(&marker) {
        let end = tail
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(tail.len());
        values.push(tail[..end].parse()?);
        rest = &tail[end..];
    }
    Ok(values)
}

fn field_f64_values(json: &str, key: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let marker = format!("\"{key}\":");
    let mut values = Vec::new();
    let mut rest = json;
    while let Some((_before, tail)) = rest.split_once(&marker) {
        let end = tail
            .find(|character: char| !matches!(character, '0'..='9' | '-' | '+' | '.' | 'e' | 'E'))
            .unwrap_or(tail.len());
        values.push(tail[..end].parse()?);
        rest = &tail[end..];
    }
    Ok(values)
}

fn validate_axes(release: &str, jack: &str, pole: &str) -> Result<(), Box<dyn std::error::Error>> {
    let release_kind = field_u64_values(release, "table_kind")?;
    let release_x = field_u64_values(release, "x_index")?;
    let release_h = field_u64_values(release, "h")?;
    let release_px = field_f64_values(release, "x_px")?;
    let release_axes = AxisFields {
        kinds: &release_kind,
        xs: &release_x,
        hs: &release_h,
        pixels: &release_px,
    };
    for kind in 0..3_u64 {
        for x in 0..u64::from(X_COUNT) {
            for h in 0..H_COUNT as u64 {
                let index = ((kind * u64::from(X_COUNT) + x) * H_COUNT as u64 + h) as usize;
                validate_axis_entry(index, (kind, x, h), release_axes)?;
            }
        }
    }
    let geometry = field_u64_values(jack, "geometry")?;
    let jack_x = field_u64_values(jack, "x_index")?;
    let jack_s = field_u64_values(jack, "s")?;
    let jack_px = field_f64_values(jack, "x_px")?;
    let jack_axes = AxisFields {
        kinds: &geometry,
        xs: &jack_x,
        hs: &jack_s,
        pixels: &jack_px,
    };
    for kind in 0..5_u64 {
        for x in 0..u64::from(X_COUNT) {
            for h in 0..H_COUNT as u64 {
                let index = ((kind * u64::from(X_COUNT) + x) * H_COUNT as u64 + h) as usize;
                validate_axis_entry(index, (kind, x, h), jack_axes)?;
            }
        }
    }
    let pole_h = field_u64_values(pole, "h")?;
    if pole_h != (0..H_COUNT as u64).collect::<Vec<_>>() {
        return Err("pole h axis is missing, duplicated, or out of order".into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct AxisFields<'a> {
    kinds: &'a [u64],
    xs: &'a [u64],
    hs: &'a [u64],
    pixels: &'a [f64],
}

fn validate_axis_entry(
    index: usize, expected: (u64, u64, u64), axes: AxisFields<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (expected_kind, expected_x, expected_h) = expected;
    let expected_pixel = f64::from(X_MIN) + expected_x as f64 * f64::from(X_STEP);
    if axes.kinds.get(index) != Some(&expected_kind)
        || axes.xs.get(index) != Some(&expected_x)
        || axes.hs.get(index) != Some(&expected_h)
        || axes.pixels.get(index) != Some(&expected_pixel)
    {
        return Err(format!("table axis entry {index} is missing, duplicated, or out of order").into());
    }
    Ok(())
}

fn append_f32(output: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        output.extend_from_slice(&value.to_le_bytes());
    }
}

fn validate_range(name: &str, values: &[f32], minimum: f32, maximum: f32) -> Result<(), Box<dyn std::error::Error>> {
    if values
        .iter()
        .any(|value| !value.is_finite() || !(minimum..=maximum).contains(value))
    {
        return Err(format!("{name} table contains a non-finite or out-of-range value").into());
    }
    Ok(())
}
