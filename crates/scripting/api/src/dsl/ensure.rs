//! Wave-zero zombie row normalization.

use rsvz_current::CurrentBackend;
use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::RuntimeError;
use crate::setup::parse_zombie_abbreviations;
use rsvz_backend_api::backend::{
    BoardReadinessBackend, GameUiBackend, GridTerrainBackend, SceneBackend, ZombiePositionWriteBackend,
};
use rsvz_model::model::ZombieKind;

thread_local! {
    static GROUPS: RefCell<Vec<EnsureGroup>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct EnsureGroup {
    wave: i32,
    kind: ZombieKind,
    rows: Rc<RefCell<Vec<i32>>>,
}

pub(crate) struct EnsureGroupGuard {
    previous: Vec<EnsureGroup>,
}

pub(crate) fn begin_script() -> EnsureGroupGuard {
    let previous = GROUPS.with(|groups| groups.replace(Vec::new()));
    EnsureGroupGuard { previous }
}

impl Drop for EnsureGroupGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        GROUPS.with(|groups| {
            groups.replace(previous);
        });
    }
}

/// Converts one zombie kind or one abbreviation into a concrete kind.
pub trait IntoZombieKindArg {
    fn into_zombie_kind_arg(self) -> Result<ZombieKind, String>;
}

impl IntoZombieKindArg for ZombieKind {
    fn into_zombie_kind_arg(self) -> Result<ZombieKind, String> {
        Ok(self)
    }
}

impl IntoZombieKindArg for char {
    fn into_zombie_kind_arg(self) -> Result<ZombieKind, String> {
        self.to_string().as_str().into_zombie_kind_arg()
    }
}

impl IntoZombieKindArg for &str {
    fn into_zombie_kind_arg(self) -> Result<ZombieKind, String> {
        let kinds = parse_zombie_abbreviations(self).map_err(|error| error.to_string())?;
        match kinds.as_slice() {
            [kind] => Ok(*kind),
            _ => Err("ensure_exist requires exactly one zombie abbreviation".to_owned()),
        }
    }
}

/// Converts packed or iterable script-facing one-based rows.
pub trait IntoZombieRows {
    fn into_zombie_rows(self) -> Result<Vec<i32>, String>;
}

impl IntoZombieRows for i32 {
    fn into_zombie_rows(self) -> Result<Vec<i32>, String> {
        if self <= 0 {
            return Err(invalid_rows());
        }
        self.to_string()
            .chars()
            .map(|ch| {
                ch.to_digit(10)
                    .and_then(|digit| Self::try_from(digit).ok())
                    .filter(|row| *row > 0)
                    .ok_or_else(invalid_rows)
            })
            .collect()
    }
}

impl<const N: usize> IntoZombieRows for [i32; N] {
    fn into_zombie_rows(self) -> Result<Vec<i32>, String> {
        rows_from_iter(self)
    }
}

impl IntoZombieRows for &[i32] {
    fn into_zombie_rows(self) -> Result<Vec<i32>, String> {
        rows_from_iter(self.iter().copied())
    }
}

impl IntoZombieRows for Vec<i32> {
    fn into_zombie_rows(self) -> Result<Vec<i32>, String> {
        rows_from_iter(self)
    }
}

fn rows_from_iter(rows: impl IntoIterator<Item = i32>) -> Result<Vec<i32>, String> {
    let rows = rows.into_iter().collect::<Vec<_>>();
    if rows.is_empty() || rows.iter().any(|row| *row <= 0) {
        Err(invalid_rows())
    } else {
        Ok(rows)
    }
}

fn invalid_rows() -> String {
    "ensure_exist rows must be positive one-based row digits".to_owned()
}

/// Registers zombie-row normalization for the current wave group.
pub fn ensure_exist<K, Rows>(kind: K, rows: Rows)
where
    CurrentBackend: SceneBackend
        + GridTerrainBackend
        + ZombiePositionWriteBackend
        + GameUiBackend
        + BoardReadinessBackend
        + 'static,
    K: IntoZombieKindArg,
    Rows: IntoZombieRows,
{
    let kind = kind.into_zombie_kind_arg();
    let rows = rows.into_zombie_rows();
    match (kind, rows) {
        (Ok(kind), Ok(rows)) => ensure_for_current_waves(kind, &rows),
        (Err(error), _) | (_, Err(error)) => crate::registration::record_error(error),
    }
}

fn ensure_for_current_waves(kind: ZombieKind, rows: &[i32])
where
    CurrentBackend: SceneBackend
        + GridTerrainBackend
        + ZombiePositionWriteBackend
        + GameUiBackend
        + BoardReadinessBackend
        + 'static,
{
    let Some(bits) = crate::registration::require_current_wave_bits() else {
        return;
    };
    for wave in super::input::WaveSet::from_bits(bits).iter() {
        ensure_group(wave, kind, rows);
    }
}

fn ensure_group(wave: i32, kind: ZombieKind, rows: &[i32])
where
    CurrentBackend: SceneBackend
        + GridTerrainBackend
        + ZombiePositionWriteBackend
        + GameUiBackend
        + BoardReadinessBackend
        + 'static,
{
    let existing = GROUPS.with(|groups| {
        groups
            .borrow()
            .iter()
            .find(|group| group.wave == wave && group.kind == kind)
            .cloned()
    });
    if let Some(group) = existing {
        merge_rows(&mut group.rows.borrow_mut(), rows);
        return;
    }

    let grouped_rows = Rc::new(RefCell::new(Vec::new()));
    merge_rows(&mut grouped_rows.borrow_mut(), rows);
    let callback_rows = Rc::clone(&grouped_rows);
    let result = rsvz_game::timeline::try_at(wave, 0, move || {
        let rows = callback_rows.borrow();
        rsvz_game::logic::zombies::ensure_zombie_rows_one_based(kind, rows.iter().copied()).map_err(runtime_error)
    });
    match result {
        Ok(_handle) => GROUPS.with(|groups| {
            groups.borrow_mut().push(EnsureGroup {
                wave,
                kind,
                rows: grouped_rows,
            });
        }),
        Err(error) => crate::registration::record_error(error),
    }
}

fn merge_rows(target: &mut Vec<i32>, rows: &[i32]) {
    for row in rows.iter().copied() {
        if !target.contains(&row) {
            target.push(row);
        }
    }
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kind_and_packed_rows() {
        assert_eq!("红".into_zombie_kind_arg().expect("kind"), ZombieKind::GigaGargantuar);
        assert_eq!(16.into_zombie_rows().expect("rows"), [1, 6]);
        assert!("".into_zombie_kind_arg().is_err());
        assert!(10.into_zombie_rows().is_err());
    }

    #[test]
    fn merging_preserves_first_occurrence_order() {
        let mut rows = vec![1, 6];
        merge_rows(&mut rows, &[6, 2, 1, 5]);
        assert_eq!(rows, [1, 6, 2, 5]);
    }
}
