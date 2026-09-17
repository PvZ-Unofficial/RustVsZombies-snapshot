#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::prelude::*;
    use rsvz::runtime::RuntimeError;
    use rsvz::tick::{TickControl, TickOptions};
    reload(MainUiOrFightUi);
    set_zombies("普");
    select_cards("P");
    rsvz::setup::skip_seed_chooser();
    let mut first: Option<i32> = Option::None;
    let mut frames = 0u32;
    rsvz::tick::spawn(TickOptions::playing_frame(), move |meta| {
        let clock = meta.clock.ok_or_else(|| RuntimeError::new("playing clock missing"))?;
        let start = *first.get_or_insert(clock);
        frames += 1;
        if frames < 31 {
            return Ok(TickControl::Continue);
        }
        if clock <= start {
            return Err(RuntimeError::new("native clock did not advance"));
        }
        let original = rsvz::with_backend(|backend| {
            let original = backend.sun()?;
            backend.set_sun(1234)?;
            let actual = backend.sun();
            // Restore even when reading the modified value failed.
            backend.set_sun(original)?;
            let actual = actual?;
            backend.sun().map(|restored| (original, actual, restored))
        })
        .map_err(|e| RuntimeError::new(e.to_string()))?;
        if original.1 != 1234 || original.2 != original.0 {
            return Err(RuntimeError::new("sun write/read/restore mismatch"));
        }
        if cfg!(feature = "never-stop") {
            return Ok(TickControl::Continue);
        }
        rsvz::publish_artifact(rsvz::SessionArtifact::new(
            (frames, start, clock, original.0),
            |_, _| Err(RuntimeError::new("live smoke runs in one process")),
            |&(frames, first_clock, last_clock, sun), out| {
                write!(
                    out,
                    "{{\"kind\":\"live_smoke\",\"frames\":{frames},\"first_clock\":{first_clock},\"last_clock\":{last_clock},\"sun_before\":{sun},\"sun_restored\":{sun}}}"
                )
            },
        ))?;
        rsvz::stop_script();
        Ok(TickControl::Stop)
    });
    Ok(())
}
