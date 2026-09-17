//! Script input spellings and overloads for core opening setup.

use rsvz_game::lineup::{LineupApplyBackend, LineupReloadPolicy};
use rsvz_game::logic::cards::IntoCardSelection;
pub use rsvz_game::logic::zombies::random_zombie_types;
pub use rsvz_game::setup::select_cards_with;
mod input;
pub use input::{
    CardSelectionParseError, IntoZombieTypeSelection, ZombieSelectionError, parse_card_abbreviations,
    parse_zombie_abbreviations,
};
use rsvz_model::ZombieSpawnMode;
use rsvz_model::{CardSelection, PlantKind};

pub use rsvz_game::setup::{
    reload, set_game_speed, skip_between, skip_seed_chooser, skip_seed_chooser_with_options, skip_until,
    with_script_setup,
};

crate::callable::callable_api! {
    /// Applies a lineup.
    ///
    /// During script registration the request is saved and applied by the host
    /// at the safe opening-board stage. From a runtime callback it is applied
    /// to the current board immediately.
    pub lineup: LineupCommand;

    where {
        rsvz_current::CurrentBackend: LineupApplyBackend,
    }


    impl<S>
    where {
        S: AsRef<str>,
    }
    call(code: S) -> () {
        rsvz_game::setup::lineup(code.as_ref(), LineupReloadPolicy::InitialOnly);
    }

    impl<S>
    where {
        S: AsRef<str>,
    }
    call(code: S, policy: LineupReloadPolicy) -> () {
        rsvz_game::setup::lineup(code.as_ref(), policy);
    }
}

fn set_zombies_impl(input: impl IntoZombieTypeSelection, mode: ZombieSpawnMode) {
    let selection = match input.into_zombie_type_selection() {
        Ok(selection) => selection,
        Err(error) => {
            crate::registration::record_error(error);
            return;
        }
    };
    rsvz_game::setup::set_zombies(selection, mode);
}

crate::callable::callable_api! {
    /// Configures exact or constrained-random zombie types for the opening.
    ///
    /// Exact inputs accept Chinese abbreviations or `ZombieKind` collections.
    /// Use [`random_zombie_types`] for a partially fixed type set: its first
    /// argument is required and its second argument is banned.
    ///
    /// The one-argument form uses [`ZombieSpawnMode::Average`]. Pass
    /// [`ZombieSpawnMode::Exact`] to preserve the input order and count in
    /// every wave, or [`ZombieSpawnMode::Natural`] to delegate weighted list
    /// generation to the game's native picker.
    pub set_zombies: SetZombies;

    where {
    }

    impl<I>
    where {
        I: IntoZombieTypeSelection,
    }
    call(input: I) -> () {
        set_zombies_impl(input, ZombieSpawnMode::Average);
    }

    impl<I>
    where {
        I: IntoZombieTypeSelection,
    }
    call(input: I, mode: ZombieSpawnMode) -> () {
        set_zombies_impl(input, mode);
    }
}

pub fn set_wave_zombies(wave: i32, input: impl AsRef<str>) {
    // Preserve validation order: a bad wave is reported before parsing aliases.
    if !(1..=20).contains(&wave) {
        crate::registration::record_error(format!("wave must be in 1..=20, got {wave}"));
        return;
    }
    match parse_zombie_abbreviations(input.as_ref()) {
        Ok(zombies) => rsvz_game::setup::set_wave_zombies(wave, zombies),
        Err(error) => crate::registration::record_error(error),
    }
}

/// Registration-time inputs accepted by [`select_cards`].
#[doc(hidden)]
pub trait IntoSetupCards {
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String>;
}

impl IntoSetupCards for CardSelection {
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        Ok(vec![self])
    }
}

impl IntoSetupCards for PlantKind {
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        Ok(vec![CardSelection::Plant(self)])
    }
}

impl<T, const N: usize> IntoSetupCards for [T; N]
where
    T: IntoCardSelection,
{
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        Ok(self.into_iter().map(IntoCardSelection::into_card_selection).collect())
    }
}

impl<T> IntoSetupCards for Vec<T>
where
    T: IntoCardSelection,
{
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        Ok(self.into_iter().map(IntoCardSelection::into_card_selection).collect())
    }
}

impl<T> IntoSetupCards for &[T]
where
    T: IntoCardSelection + Copy,
{
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        Ok(self
            .iter()
            .copied()
            .map(IntoCardSelection::into_card_selection)
            .collect())
    }
}

impl IntoSetupCards for &str {
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        parse_card_abbreviations(self).map_err(|error| error.to_string())
    }
}

impl IntoSetupCards for String {
    fn into_setup_cards(self) -> Result<Vec<CardSelection>, String> {
        self.as_str().into_setup_cards()
    }
}

/// Callable setup entry point supporting one card input or `base + extras`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SelectCards;

impl SelectCards {
    fn apply(inputs: impl IntoIterator<Item = Result<Vec<CardSelection>, String>>) {
        let mut cards = Vec::new();
        let mut has_error = false;
        for input in inputs {
            match input {
                Ok(mut input) => cards.append(&mut input),
                Err(error) => {
                    crate::registration::record_error(error);
                    has_error = true;
                }
            }
        }
        if has_error {
            return;
        }
        rsvz_game::setup::select_cards(cards);
    }
}

crate::callable::impl_callable! {
    impl<Cards> SelectCards
    where {
        Cards: IntoSetupCards,
    }
    call(cards: Cards) -> () {
        Self::apply([cards.into_setup_cards()]);
    }
}

crate::callable::impl_callable! {
    impl<Base, Extras> SelectCards
    where {
        Base: IntoSetupCards,
        Extras: IntoSetupCards,
    }
    call(base: Base, extras: Extras) -> () {
        Self::apply([base.into_setup_cards(), extras.into_setup_cards()]);
    }
}

#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const select_cards: SelectCards = SelectCards;

#[cfg(all(test, feature = "pvz-emulator"))]
mod tests {
    use super::*;
    use rsvz_game::lineup::PendingLineup;
    use rsvz_game::logic::fast_forward::{FastForwardOptions, FastForwardWindow};
    use rsvz_game::logic::zombies::{ZombieSpawnRequest, ZombieTypeSelection};
    use rsvz_game::setup::reset_script_setup as reset_current;
    use rsvz_model::{DEFAULT_SPAWN_WAVES, ReloadMode, SpawnList};
    #[cfg(test)]
    #[must_use]
    fn desired_cards() -> Option<Vec<CardSelection>> {
        with_script_setup(|setup| setup.desired_cards.clone())
    }

    #[cfg(test)]
    #[must_use]
    fn reload_mode() -> ReloadMode {
        with_script_setup(|setup| setup.reload_mode)
    }

    #[cfg(test)]
    #[must_use]
    fn desired_spawn_list() -> Option<SpawnList> {
        with_script_setup(|setup| setup.spawn_list.clone())
    }

    #[cfg(test)]
    #[must_use]
    fn desired_lineup() -> Option<PendingLineup> {
        with_script_setup(|setup| setup.lineup.clone())
    }

    #[cfg(test)]
    #[must_use]
    fn zombie_spawn_request() -> Option<ZombieSpawnRequest> {
        with_script_setup(|setup| setup.zombie_spawn_request.clone())
    }

    #[must_use]
    fn fast_forward_windows() -> Vec<FastForwardWindow> {
        with_script_setup(|setup| setup.fast_forward_windows.clone())
    }

    use rsvz_model::{CardSelection, PlantKind, RelativeTime, Wave, ZombieKind};

    fn set_zombies(input: impl IntoZombieTypeSelection) {
        set_zombies_impl(input, ZombieSpawnMode::Average);
    }

    fn desired_exact_zombies() -> Option<Vec<ZombieKind>> {
        match zombie_spawn_request()?.selection() {
            ZombieTypeSelection::Exact(types) => Some(types.clone()),
            ZombieTypeSelection::Random { .. } => None,
        }
    }

    fn reset_setup_state() {
        reset_current();
    }

    #[test]
    fn setup_facade_writes_validated_current_state() {
        reset_setup_state();

        crate::registration::run_script(|| {
            reload(ReloadMode::MainUiOrFightUi);
            let _deferred = lineup(" 3,8 1 1 1 0 0 ");
            set_zombies("红白 矿");
            skip_until((12, 1318));
            skip_between((13, 0), (15, 0));
            Ok(())
        })
        .expect("valid setup should register");

        assert_eq!(reload_mode(), ReloadMode::MainUiOrFightUi);
        let pending = desired_lineup().expect("lineup should be set");
        assert_eq!(pending.source(), "3,8 1 1 1 0 0");
        assert_eq!(pending.policy(), LineupReloadPolicy::InitialOnly);
        assert_eq!(
            desired_exact_zombies(),
            Some(vec![
                ZombieKind::GigaGargantuar,
                ZombieKind::Gargantuar,
                ZombieKind::Digger
            ])
        );
        assert_eq!(
            fast_forward_windows(),
            vec![
                FastForwardWindow::until(RelativeTime::new(Wave(12), 1318), FastForwardOptions::aggressive(),),
                FastForwardWindow::between(
                    RelativeTime::new(Wave(13), 0),
                    RelativeTime::new(Wave(15), 0),
                    FastForwardOptions::aggressive(),
                ),
            ]
        );
    }

    #[test]
    fn failed_setup_calls_are_atomic_and_later_calls_still_run() {
        reset_setup_state();
        crate::registration::run_script(|| {
            let _deferred = lineup("3,8 1 1 1 0 0");
            set_zombies("红白");
            Ok(())
        })
        .expect("baseline setup should register");

        let error = crate::registration::run_script(|| {
            let _invalid = lineup("");
            assert_eq!(
                desired_lineup()
                    .expect("invalid lineup must not replace the valid value")
                    .source(),
                "3,8 1 1 1 0 0"
            );
            set_zombies("普怪红");
            assert_eq!(
                desired_exact_zombies(),
                Some(vec![ZombieKind::GigaGargantuar, ZombieKind::Gargantuar])
            );
            set_zombies("普");
            set_zombies(" ，；");
            Ok(())
        })
        .expect_err("invalid setup should be reported");

        assert_eq!(
            error.message().as_ref(),
            "lineup input is empty; unknown zombie abbreviation '怪'; zombie list cannot be empty"
        );
        assert_eq!(
            desired_lineup().expect("valid lineup should remain").source(),
            "3,8 1 1 1 0 0"
        );
        assert_eq!(desired_exact_zombies(), Some(vec![ZombieKind::Normal]));
    }

    #[test]
    fn select_cards_supports_one_and_two_argument_forms_in_order() {
        reset_setup_state();
        crate::registration::run_script(|| {
            select_cards(
                "IINAJ",
                [
                    PlantKind::FlowerPot,
                    PlantKind::SplitPea,
                    PlantKind::FumeShroom,
                    PlantKind::Starfruit,
                    PlantKind::GraveBuster,
                ],
            );
            Ok(())
        })
        .expect("valid cards");

        assert_eq!(
            desired_cards(),
            Some(vec![
                CardSelection::Plant(PlantKind::IceShroom),
                CardSelection::Imitator(PlantKind::IceShroom),
                CardSelection::Plant(PlantKind::DoomShroom),
                CardSelection::Plant(PlantKind::CherryBomb),
                CardSelection::Plant(PlantKind::Jalapeno),
                CardSelection::Plant(PlantKind::FlowerPot),
                CardSelection::Plant(PlantKind::SplitPea),
                CardSelection::Plant(PlantKind::FumeShroom),
                CardSelection::Plant(PlantKind::Starfruit),
                CardSelection::Plant(PlantKind::GraveBuster),
            ])
        );

        crate::registration::run_script(|| {
            select_cards([PlantKind::CoffeeBean]);
            Ok(())
        })
        .expect("one argument cards");
        assert_eq!(desired_cards(), Some(vec![CardSelection::Plant(PlantKind::CoffeeBean)]));
    }

    #[test]
    fn set_wave_zombies_keeps_other_waves_and_builds_a_full_huge_wave() {
        reset_setup_state();
        crate::registration::run_script(|| {
            set_zombies("红白");
            set_wave_zombies(20, "普杆障");
            Ok(())
        })
        .expect("valid wave override");

        let spawn = desired_spawn_list().expect("spawn list");
        assert_eq!(desired_exact_zombies(), None);
        assert_eq!(spawn.wave_count(), DEFAULT_SPAWN_WAVES);
        assert_eq!(spawn.wave(0).map(<[_]>::len), Some(50));
        assert_eq!(
            spawn.wave(19).and_then(|wave| wave.get(..7)),
            Some(
                [
                    ZombieKind::Flag,
                    ZombieKind::Normal,
                    ZombieKind::PoleVaulting,
                    ZombieKind::Conehead,
                    ZombieKind::Normal,
                    ZombieKind::PoleVaulting,
                    ZombieKind::Conehead,
                ]
                .as_slice()
            )
        );
    }

    #[test]
    fn set_zombies_discards_an_older_wave_override() {
        reset_setup_state();
        crate::registration::run_script(|| {
            set_zombies("红白");
            set_wave_zombies(20, "普杆障");
            set_zombies("杆车");
            Ok(())
        })
        .expect("valid setup");

        assert_eq!(desired_spawn_list(), None);
        assert_eq!(
            desired_exact_zombies(),
            Some(vec![ZombieKind::PoleVaulting, ZombieKind::Zomboni])
        );
    }

    #[test]
    fn set_zombies_records_constrained_random_natural_mode() {
        reset_setup_state();
        crate::registration::run_script(|| {
            set_zombies_impl(
                random_zombie_types([ZombieKind::Gargantuar], [ZombieKind::Football]),
                ZombieSpawnMode::Natural,
            );
            Ok(())
        })
        .expect("valid constrained-random setup");

        let request = zombie_spawn_request().expect("spawn request");
        assert_eq!(request.mode(), ZombieSpawnMode::Natural);
        assert_eq!(
            request.selection(),
            &ZombieTypeSelection::Random {
                required: vec![ZombieKind::Gargantuar],
                banned: vec![ZombieKind::Football],
            }
        );
    }

    #[test]
    fn set_zombies_callable_covers_default_and_explicit_modes() {
        reset_setup_state();
        let command = SetZombies::new();
        crate::registration::run_script(|| {
            command([ZombieKind::Normal, ZombieKind::Gargantuar]);
            command(
                random_zombie_types([ZombieKind::Gargantuar], [ZombieKind::Football]),
                ZombieSpawnMode::Natural,
            );
            command(
                [ZombieKind::JackInTheBox, ZombieKind::JackInTheBox, ZombieKind::Ladder],
                ZombieSpawnMode::Exact,
            );
            Ok(())
        })
        .expect("both callable overloads should register");

        let request = zombie_spawn_request().expect("exact request should be retained");
        assert_eq!(request.mode(), ZombieSpawnMode::Exact);
        assert_eq!(
            request.selection(),
            &ZombieTypeSelection::Exact(vec![
                ZombieKind::JackInTheBox,
                ZombieKind::JackInTheBox,
                ZombieKind::Ladder,
            ])
        );
    }

    #[test]
    fn invalid_select_cards_calls_do_not_replace_previous_state() {
        reset_setup_state();
        crate::registration::run_script(|| {
            select_cards([PlantKind::CoffeeBean]);
            Ok(())
        })
        .expect("baseline");

        let error = crate::registration::run_script(|| {
            select_cards("?");
            select_cards("III");
            select_cards(
                "IINAJ",
                [
                    PlantKind::FlowerPot,
                    PlantKind::SplitPea,
                    PlantKind::FumeShroom,
                    PlantKind::Starfruit,
                    PlantKind::GraveBuster,
                    PlantKind::CoffeeBean,
                ],
            );
            Ok(())
        })
        .expect_err("all invalid calls should be collected");

        let message = error.message();
        assert!(message.contains("unknown card abbreviation '?'"));
        assert!(message.contains("multiple imitator cards selected"));
        assert!(message.contains("too many cards selected: 11 > 10"));
        assert_eq!(desired_cards(), Some(vec![CardSelection::Plant(PlantKind::CoffeeBean)]));
    }

    #[test]
    fn two_argument_select_cards_collects_both_conversion_errors() {
        reset_setup_state();

        let error = crate::registration::run_script(|| {
            select_cards("?", "?");
            Ok(())
        })
        .expect_err("both card inputs are invalid");

        assert_eq!(error.message().matches("unknown card abbreviation '?'").count(), 2);
        assert_eq!(desired_cards(), None);
    }
}
