//! Optional script decision facts; no automatic control and no native gameplay changes.
use super::*;
use rsvz::core::model::PlantId;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static WATCH: Cell<Option<(u64, PlantId, Pos)>> = const { Cell::new(None) };
    static AIR: Cell<Option<(u64, &'static str, u16)>> = const { Cell::new(None) };
}
pub(super) fn enabled() -> bool {
    ENABLED.get()
}
pub(super) fn install() -> RuntimeResult<()> {
    ENABLED.set(std::env::var("MGE_TRACE_DECISIONS").as_deref() == Ok("1"));
    if enabled() {
        rsvz::measure::trace_plant_losses([Blover])?;
        rsvz::event::on_home_entry(rsvz::event::EventOptions::new(), |event| {
            let wave_time = rsvz::core::timeline::with_timeline_ref(|timeline| {
                (1..=20)
                    .filter_map(|wave| {
                        let refresh = timeline.wave_clocks().refresh_clock(Wave(wave))?;
                        (refresh <= event.main_counter).then_some((wave, event.main_counter - refresh))
                    })
                    .last()
            });
            rsvz::log(
                rsvz::LogLevel::Debug,
                format_args!("mge_home_detail event={event:?} wave_time={wave_time:?} sun={}", sun()),
            );
        })
        .map_err(err)?;
        rsvz::event::on_plant_effect(rsvz::event::EventOptions::new(), |event| {
            use rsvz::core::model::{PlantEffectOutcome, PlantEffectSource};
            if !matches!(event.outcome, PlantEffectOutcome::Killed | PlantEffectOutcome::Squished)
                || !matches!(
                    event.raw_kind,
                    GloomShroom | WinterMelon | TwinSunflower | UmbrellaLeaf | Blover
                )
            {
                return;
            }
            if let PlantEffectSource::Bite(id) = event.source {
                let kind = rsvz::with_backend(|b| {
                    b.zombie(id)
                        .expect("trace biting zombie")
                        .map(|z| b.zombie_kind(z).expect("trace biting kind"))
                });
                rsvz::log(
                    rsvz::LogLevel::Debug,
                    format_args!(
                        "mge_bite_loss plant={} actor={} actor_kind={kind:?}",
                        event.plant_id.raw(),
                        id.raw()
                    ),
                );
            }
        })
        .map_err(err)?;
    }
    Ok(())
}
pub(super) fn watch(id: PlantId, pos: Pos) {
    WATCH.set(Some((rsvz::session::world_epoch(), id, pos)));
}
pub(super) fn tick() {
    if !enabled() {
        return;
    }
    let Some((epoch, id, pos)) = WATCH.get() else {
        return;
    };
    if epoch != rsvz::session::world_epoch() {
        WATCH.set(None);
        return;
    }
    let status = rsvz::with_backend(|b| {
        b.plant(id)
            .expect("trace Blover")
            .map(|p| (b.plant_state(p), b.plant_is_squished(p)))
    });
    let outcome = match status {
        Some((_, true)) => "squished_before_observed_blow",
        Some((2, false)) => "blow_observed",
        Some(_) => return,
        None => "gone_before_observed_blow",
    };
    rsvz::log(
        rsvz::LogLevel::Debug,
        format_args!(
            "mge_blover id={} grid={pos:?} outcome={outcome} sun={}",
            id.raw(),
            sun()
        ),
    );
    WATCH.set(None);
}
pub(super) fn removal(pos: Pos, operation: &'static str) {
    if enabled() {
        rsvz::log(
            rsvz::LogLevel::Debug,
            format_args!(
                "mge_remove operation={operation} grid={pos:?} main={:?}",
                main_kind(pos)
            ),
        );
    }
}
pub(super) fn air() {
    if !enabled() {
        return;
    }
    let near = balloon_near(470, -50);
    let cd = rsvz::core::logic::cards::card_cd(sel(Blover));
    let balance = sun();
    let mut safe_mask = 0;
    let reason = if !near {
        "no_near_balloon"
    } else if cd.is_none() {
        "not_selected"
    } else if cd.is_some_and(|x| x > 0) {
        "cooldown"
    } else if balance < 100 {
        "sun_below_100"
    } else {
        for (i, pos) in [
            (1, 5),
            (5, 5),
            (1, 6),
            (5, 6),
            (2, 7),
            (4, 7),
            (1, 4),
            (5, 4),
            (3, 8),
            (3, 9),
        ]
        .into_iter()
        .enumerate()
        {
            if rsvz::is_safe_blover(grid(pos)).expect("trace safety") {
                safe_mask |= 1 << i;
            }
        }
        if safe_mask == 0 {
            "no_safe_grid"
        } else {
            "ready_check_plantability"
        }
    };
    let state = (rsvz::session::world_epoch(), reason, safe_mask);
    if AIR.replace(Some(state)) != Some(state) {
        rsvz::log(
            rsvz::LogLevel::Debug,
            format_args!("mge_air reason={reason} cd={cd:?} sun={balance} safe_mask={safe_mask:#x}"),
        );
    }
}
