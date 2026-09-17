//! Script decisions are temporary diagnostics; they do not change action policy.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, Default)]
struct CardCounts {
    planted: u64,
    unusable: u64,
    rejected: u64,
    spent: u64,
    last_rejection: Option<PlantRejectReason>,
}
struct Counts {
    cards: [CardCounts; 49],
    // Reasons before any missing-grid search; these are skipped checks, not known failed repairs.
    gloom_checks: [u64; 3],
    // An actual missing target was found and skipped.
    gloom_threat: [u64; 8],
    gloom_placement: [u64; 8],
}
impl Default for Counts {
    fn default() -> Self {
        Self {
            cards: [CardCounts::default(); 49],
            gloom_checks: [0; 3],
            gloom_threat: [0; 8],
            gloom_placement: [0; 8],
        }
    }
}
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNTS: RefCell<Counts> = RefCell::new(Counts::default());
}
pub fn configure() {
    ENABLED.set(std::env::var("HOUTUI_SL_TRACE_ACTIONS").as_deref() != Ok("0"));
}
pub fn enabled() -> bool {
    ENABLED.get()
}
fn index(card: CardSelection) -> usize {
    match card {
        CardSelection::Plant(kind) => kind as usize,
        CardSelection::Imitator(_) => 48,
    }
}
pub fn outcome(card: CardSelection, pos: Pos, result: PlantSeedOutcome, before: Option<u32>) {
    if !enabled() {
        return;
    }
    let after = before.map(|_| sun());
    COUNTS.with_borrow_mut(|counts| {
        let c = &mut counts.cards[index(card)];
        match result {
            PlantSeedOutcome::Planted(_) => {
                c.planted += 1;
                c.spent += u64::from(before.unwrap_or(0).saturating_sub(after.unwrap_or(0)));
            }
            PlantSeedOutcome::Unusable => c.unusable += 1,
            PlantSeedOutcome::Rejected(reason) => {
                c.rejected += 1;
                c.last_rejection = Some(reason);
            }
        }
    });
    if matches!(result, PlantSeedOutcome::Planted(_))
        && matches!(
            card,
            CardSelection::Plant(IceShroom | DoomShroom | CherryBomb | Jalapeno | GloomShroom)
                | CardSelection::Imitator(IceShroom)
        )
    {
        rsvz::log(
            rsvz::LogLevel::Debug,
            format_args!(
                "action_planted epoch={} rounds={} card={card:?} grid={pos:?} outcome={result:?} sun_before={before:?} sun_after={after:?}",
                rsvz::session::world_epoch(),
                rsvz::session::completed_rounds()
            ),
        );
    }
}
pub fn request(card: CardSelection, positions: &[Pos], delay: Option<i32>) {
    if enabled() {
        rsvz::log(
            rsvz::LogLevel::Debug,
            format_args!(
                "action_request card={card:?} grids={positions:?} retry_delay={delay:?} sun={} cd={:?}",
                sun(),
                card_cd(card)
            ),
        );
    }
}
pub fn gloom_check(reason: usize) {
    if enabled() {
        COUNTS.with_borrow_mut(|c| c.gloom_checks[reason] += 1);
    }
}
pub fn gloom_grid(pos: Pos, placement: bool) {
    if enabled() {
        let index = GLOOMS.iter().position(|&p| p == pos).expect("gloom target");
        COUNTS.with_borrow_mut(|c| {
            if placement {
                c.gloom_placement[index] += 1;
            } else {
                c.gloom_threat[index] += 1;
            }
        });
    }
}
pub fn flush(boundary: &'static str) {
    if !enabled() {
        return;
    }
    let counts = COUNTS.with_borrow_mut(std::mem::take);
    let (clock, balance) = rsvz::with_backend(|b| (b.clock().ok(), b.sun().ok()));
    for (kind, c) in counts.cards.iter().enumerate() {
        if c.planted + c.unusable + c.rejected != 0 {
            rsvz::log(
                rsvz::LogLevel::Debug,
                format_args!(
                    "action_summary boundary={boundary} epoch={} rounds={} clock={clock:?} sun={balance:?} card_code={kind} planted={} unusable_checks={} rejected_checks={} spent={} last_rejection={:?}",
                    rsvz::session::world_epoch(),
                    rsvz::session::completed_rounds(),
                    c.planted,
                    c.unusable,
                    c.rejected,
                    c.spent,
                    c.last_rejection
                ),
            );
        }
    }
    if counts.gloom_checks.iter().any(|&n| n != 0)
        || counts
            .gloom_threat
            .iter()
            .chain(&counts.gloom_placement)
            .any(|&n| n != 0)
    {
        rsvz::log(
            rsvz::LogLevel::Debug,
            format_args!(
                "gloom_summary boundary={boundary} epoch={} rounds={} clock={clock:?} sun={balance:?} skipped_checks_low_sun={} skipped_checks_fume_unavailable={} skipped_checks_gloom_cd={} missing_grid_threat={:?} missing_grid_placement={:?}",
                rsvz::session::world_epoch(),
                rsvz::session::completed_rounds(),
                counts.gloom_checks[0],
                counts.gloom_checks[1],
                counts.gloom_checks[2],
                counts.gloom_threat,
                counts.gloom_placement
            ),
        );
    }
}
