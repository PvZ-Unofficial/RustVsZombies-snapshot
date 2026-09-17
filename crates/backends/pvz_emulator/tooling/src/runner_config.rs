//! Cold-path configuration passed to the generated PE runner.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::wire;

const DEFAULT_SEED_SEQUENCE_SALT: u64 = 0xD1B5_4A32_D192_ED03;
static DEFAULT_SEED_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct RunRequest {
    pub threads: NonZeroUsize,
    pub seed_mode: wire::SeedMode,
    pub max_wall: Option<Duration>,
    pub max_sim_frames: Option<u64>,
    pub max_levels: Option<u64>,
    pub profile: bool,
    pub profile_detail: bool,
    pub performance_window: Option<Duration>,
    pub output_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GeneratedRunnerInvocation {
    pub config: wire::PeRunConfig,
    pub config_path: PathBuf,
}

pub(crate) fn configure_generated_runner_command(
    command: &mut Command, runtime_dir: &Path, request: &RunRequest,
) -> Result<GeneratedRunnerInvocation> {
    fs::create_dir_all(runtime_dir).with_context(|| format!("创建 PE runtime 目录失败: {}", runtime_dir.display()))?;
    let run_id = new_run_id();
    let config_path = runtime_dir.join(format!("{run_id}.config.json"));
    let config = wire::PeRunConfig {
        schema_version: wire::SCHEMA_VERSION,
        run_id: run_id.clone(),
        threads: request.threads.get(),
        seed_mode: request.seed_mode,
        limits: wire::RunLimits {
            max_wall_ms: request.max_wall.map(duration_ms_u64),
            max_sim_frames: request.max_sim_frames,
            max_levels: request.max_levels,
        },
        profile: request.profile,
        profile_detail: request.profile_detail,
        performance_window_ms: request.performance_window.map(duration_ms_u64),
        output_path: request.output_path.clone(),
        raw_result_path: runtime_dir.join(format!("{run_id}.result.json")),
    };
    config
        .validate(Some(&run_id))
        .map_err(anyhow::Error::msg)
        .context("校验 generated PE config 失败")?;
    write_json_new(&config_path, &config)?;
    command.env(wire::CONFIG_PATH_ENV, &config_path);
    Ok(GeneratedRunnerInvocation { config, config_path })
}

#[must_use]
pub(crate) fn default_thread_count() -> NonZeroUsize {
    std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

#[must_use]
pub(crate) fn generate_default_seed_base() -> u32 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let sequence = DEFAULT_SEED_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let entropy = now.as_secs()
        ^ (u64::from(now.subsec_nanos()) << 32)
        ^ u64::from(std::process::id())
        ^ sequence.wrapping_mul(DEFAULT_SEED_SEQUENCE_SALT);
    mix_seed_entropy(entropy)
}

fn mix_seed_entropy(mut value: u64) -> u32 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    u32::try_from(value & u64::from(u32::MAX)).expect("masked seed entropy fits in u32")
}

fn duration_ms_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn new_run_id() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{sequence:x}", std::process::id(), now.as_nanos())
}

fn write_json_new<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value).context("序列化 PE config JSON 失败")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("创建 PE config 失败: {}", path.display()))?;
    let result = file.write_all(&bytes).context("写入 PE config JSON 失败");
    drop(file);
    if result.is_err() {
        let _cleanup_result = fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_config_is_neither_overwritten_nor_removed() {
        let path = std::env::temp_dir().join(format!("rsvz-config-{}.json", new_run_id()));
        write_json_new(&path, &vec![1, 2]).unwrap();
        assert!(write_json_new(&path, &vec![3]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"[1,2]");
        fs::remove_file(path).unwrap();
    }
}
