//! Synchronous safety of planting an imitator ice-shroom in a night scene.

use crate::logic::card_timing::IMITATOR_MORPH_DELAY;
use crate::logic::contact::{circle_hits_rect, zombie_attack_bounds_from_handle};
use crate::logic::zombie_motion::vanilla_track_frames;
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{
    GridGeometryBackend, PlantContactBackend, PlantReadBackend, SceneBackend, ZombieRawFactsBackend, ZombieReadBackend,
    ZombieStateBackend,
};
use rsvz_current::CurrentBackend;
use rsvz_model::{
    AbsoluteContactRange, ContactCircle, ContactRect, Grid, PlantKind, ZombieKind, ZombiePhase, ZombieTrackProfile,
};

const MORPH: i32 = IMITATOR_MORPH_DELAY;
const EFFECT: i32 = MORPH + 100;

/// Whether an imitator ice-shroom planted now is safe from the current zombies.
///
/// For flat night scenes (Night, Fog, MushroomGarden); early activation by a hammer after morphing
/// is successful. Any live frozen zombie is an error, even in another row.
/// Movement uses vanilla speed bounds, not hidden rerolled speeds, future melon
/// hits, or an unopened Jack's countdown. A false result means safety could not
/// be established, not that the plant will certainly die.
///
/// The candidate replaces the ordinary plant in this square; its pumpkin and
/// container remain. Existing plants elsewhere are not credited as future shields.
/// The caller owns legality, cards, sun, shovelling, retries and subsequent actions.
/// No task is registered. 暂不覆盖篮球、未来刷新和未来受到伤害后触发的投掷/失去跳具/气球落地。
pub fn is_safe_imitator_ice(grid: Grid) -> RuntimeResult<bool>
where
    CurrentBackend:
        SceneBackend + GridGeometryBackend + PlantContactBackend + ZombieRawFactsBackend + ZombieStateBackend,
{
    crate::access::with_backend(|b| {
        let scene = b.scene().map_err(error)?;
        if !grid.is_in_bounds(scene.row_count(), 9) {
            return Err(RuntimeError::new("imitator ice grid is outside the current lawn"));
        }
        if !scene.is_night() || scene.has_roof() {
            return Err(RuntimeError::new("safe imitator ice requires a flat night scene"));
        }
        let x = b.grid_to_pixel_x(grid.col, grid.row).map_err(error)?;
        let y = b.grid_to_pixel_y(grid.col, grid.row).map_err(error)?;
        let body = ContactRect::new(x + 10, y, x + 70, y + 80);
        let mut blast = body;
        let mut chew = defense(body);
        let mut crush = chew;
        let mut health = 300;
        b.for_each_plant_at_grid(grid, |p| {
            match b.plant_raw_kind(p)? {
                PlantKind::Pumpkin => {
                    let rect = b.plant_hit_box(p)?;
                    blast = union_rect(blast, rect);
                    chew = union(chew, defense(rect));
                    crush = union(crush, chew);
                    health += b.plant_hp(p).max(0);
                }
                PlantKind::FlowerPot | PlantKind::LilyPad => {
                    let rect = b.plant_hit_box(p)?;
                    blast = union_rect(blast, rect);
                    crush = union(crush, defense(rect));
                }
                _ => {}
            }
            Ok(())
        })
        .map_err(error)?;

        let mut safe = true;
        let mut bites = 0;
        for z in b.zombies().map_err(error)? {
            if !b.zombie_is_alive(z) || b.zombie_is_disappeared(z) {
                continue;
            }
            // Finish this global precondition check even after finding a threat.
            if b.zombie_frozen_countdown(z) > 0 {
                return Err(RuntimeError::new(
                    "safe imitator ice requires all live zombies to be thawed",
                ));
            }
            if !safe || b.zombie_is_mind_controlled(z) {
                continue;
            }
            let kind = b.zombie_kind(z).map_err(error)?;
            let phase = b.zombie_phase(z).map_err(error)?;
            let row = b.zombie_row(z);
            let zx = b.zombie_pos_x(z);
            let slow = b.zombie_chilled_countdown(z);
            let vehicle = matches!(kind, ZombieKind::Zomboni | ZombieKind::Catapult);
            if (!vehicle && !b.zombie_has_head(z)) || b.zombie_is_blown_away(z) {
                continue;
            }
            if kind == ZombieKind::JackInTheBox {
                if phase == ZombiePhase::JackInTheBoxPopping {
                    if b.zombie_phase_counter(z) <= EFFECT {
                        safe = !circle_hits_rect(
                            ContactCircle::new(
                                b.zombie_int_x(z) + b.zombie_width(z) / 2,
                                b.zombie_int_y(z) + b.zombie_height(z) / 2,
                                90,
                            ),
                            blast,
                        );
                    }
                    continue;
                }
                // Do not inspect the hidden pre-pop countdown. An unopened Jack
                // can stop anywhere in its reachable interval and then pop in 110cs.
                if jack_can_blast(
                    zx,
                    b.zombie_int_y(z) + b.zombie_height(z) / 2,
                    slow,
                    walking_bound(kind),
                    blast,
                ) {
                    safe = false;
                    continue;
                }
            }
            if kind == ZombieKind::Dancing && (row - grid.row).abs() <= 1 {
                // A missing follower may rise next to the candidate. Count its
                // bite budget as well as the leader; no persistent follower cache.
                for (slot, dr, dx) in [(0, -1, 0.0), (1, 1, 0.0), (2, 0, -100.0), (3, 0, 100.0)] {
                    if row + dr != grid.row {
                        continue;
                    }
                    let id = rsvz_model::ZombieId::from_raw(b.zombie_follower_id(z, slot).map_err(error)?);
                    if b.zombie(id).map_err(error)?.is_some() {
                        continue;
                    }
                    let attack = AbsoluteContactRange::new(zx as i32 + dx as i32 + 20, zx as i32 + dx as i32 + 70);
                    if let Some(at) = contact_at(attack, chew, 0.5, slow, false, MORPH) {
                        // A newly summoned follower cannot bite during its 150cs rise.
                        // It is born unchilled, even if the leader is still chilled.
                        bites += bite_damage(at.max(150), 0);
                    }
                }
                safe &= bites < health;
            }
            if row != grid.row || !safe {
                continue;
            }
            if kind == ZombieKind::Bungee {
                if b.zombie_int_x(z) == x {
                    let grab = match phase {
                        ZombiePhase::BungeeAtBottom => b.zombie_phase_counter(z),
                        ZombiePhase::BungeeDiving | ZombiePhase::BungeeDivingScreaming => {
                            (b.zombie_altitude(z).max(0.0) / 8.0).ceil() as i32 + 300
                        }
                        _ => EFFECT + 1,
                    };
                    safe = grab > EFFECT;
                }
                continue;
            }
            if kind == ZombieKind::Balloon && phase == ZombiePhase::BalloonFlying {
                continue;
            }
            if kind == ZombieKind::Pogo && b.zombie_has_object(z) {
                continue; // Jumping over an ordinary mushroom does not hurt it.
            }
            if kind == ZombieKind::Digger && phase == ZombiePhase::DiggerTunneling {
                continue; // Natural surfacing plus stun is longer than the morph window.
            }
            if phase == ZombiePhase::GargantuarThrowing && b.zombie_has_object(z) && body.left < zx as i32 {
                // The throw is already visible, but its random landing position
                // is not a permitted input. Do not certify the threatened rear area.
                safe = false;
                continue;
            }
            let attack = zombie_attack_bounds_from_handle(b, z)
                .map_err(error)?
                .map(|a| a.range)
                .unwrap_or(AbsoluteContactRange::new(zx as i32 + 20, zx as i32 + 70));
            if vehicle {
                if !b.zombie_has_flat_tires(z) {
                    // A stopped catapult may resume. Awake ice itself is immune
                    // to direct drive-over; only the pre-morph window is lethal.
                    let speed = if kind == ZombieKind::Zomboni { 0.25 } else { 0.37 };
                    safe = contact_at(
                        attack,
                        crush,
                        speed,
                        if kind == ZombieKind::Zomboni { 0 } else { slow },
                        false,
                        MORPH,
                    )
                    .is_none();
                }
                continue;
            }
            // Flight/jump endpoints need a different trajectory from walking.
            // Reject only candidates in the swept reach, rather than silently
            // interpreting the current jump as slow ground movement.
            if matches!(
                phase,
                ZombiePhase::ImpGettingThrown
                    | ZombiePhase::ImpLanding
                    | ZombiePhase::PolevaulterInVault
                    | ZombiePhase::DolphinInJump
            ) {
                safe = contact_at(attack, chew, 3.0, 0, false, MORPH).is_none();
                continue;
            }
            let rightward = (kind == ZombieKind::Digger && phase != ZombiePhase::DiggerWalkingWithoutAxe)
                || phase == ZombiePhase::YetiRunning;
            let Some(at) = contact_at(attack, chew, walking_bound(kind), slow, rightward, MORPH) else {
                continue;
            };
            if matches!(kind, ZombieKind::Gargantuar | ZombieKind::GigaGargantuar) {
                safe = phase != ZombiePhase::GargantuarSmashing && hammer_after_morph(at, slow);
            } else {
                bites += bite_damage(at, slow);
                safe = bites < health;
            }
        }
        Ok(safe)
    })
}

fn error(e: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(e.to_string())
}
fn defense(r: ContactRect) -> AbsoluteContactRange {
    AbsoluteContactRange::new(r.left + 20, r.right - 20)
}
fn union(a: AbsoluteContactRange, b: AbsoluteContactRange) -> AbsoluteContactRange {
    AbsoluteContactRange::new(a.left.min(b.left), a.right.max(b.right))
}
fn union_rect(a: ContactRect, b: ContactRect) -> ContactRect {
    ContactRect::new(
        a.left.min(b.left),
        a.top.min(b.top),
        a.right.max(b.right),
        a.bottom.max(b.bottom),
    )
}

// The maximum per-frame ground-track displacement bounds every legal rerolled
// speed and phase. This avoids reading a hidden speed or trusting a future reset.
fn walking_bound(kind: ZombieKind) -> f32 {
    use ZombieKind as Z;
    use ZombieTrackProfile as P;
    let (profile, speed) = match kind {
        Z::Gargantuar | Z::GigaGargantuar => (P::GargantuarWalk, 0.37),
        Z::Football => (P::FootballWalk, 0.68),
        Z::JackInTheBox => (P::JackBoxWalk, 0.68),
        Z::Ladder => (P::LadderWalk, 0.81),
        Z::Newspaper => (P::NewspaperWalk, 0.91),
        Z::PoleVaulting => (P::PoleVaultBeforeJump, 0.68),
        Z::Digger => (P::DiggerWalk, 0.37),
        Z::Imp => (P::ImpWalk, 0.37),
        Z::Dancing | Z::BackupDancer => return 0.5,
        Z::Flag | Z::Bobsled => (P::NormalWalkA, 0.6),
        Z::Snorkel | Z::DolphinRider => (P::NormalWalkA, 0.91),
        Z::Yeti => (P::YetiWalk, 0.8),
        _ => (P::NormalWalkA, 0.37),
    };
    let bound = track_bound(profile, speed);
    if matches!(kind, Z::Normal | Z::Conehead | Z::Buckethead | Z::ScreenDoor) {
        bound
            .max(track_bound(P::NormalWalkB, speed))
            .max(track_bound(P::NormalDance, speed))
    } else {
        bound
    }
}
fn track_bound(profile: ZombieTrackProfile, speed: f32) -> f32 {
    let frames = vanilla_track_frames(profile);
    let max_step = frames.windows(2).map(|v| v[1] - v[0]).fold(0.0f32, f32::max);
    (max_step * 0.47 * speed * frames.len() as f32 / (frames[frames.len() - 1] - frames[0]))
        .max(speed) // A reset may temporarily use uniform movement.
        .next_up()
}

fn contact_at(
    attack: AbsoluteContactRange, target: AbsoluteContactRange, speed: f32, slow: i32, rightward: bool, horizon: i32,
) -> Option<i32> {
    let reach = (speed * horizon as f32).ceil() as i32;
    if if rightward {
        attack.left > target.right || attack.right + reach < target.left
    } else {
        attack.right < target.left || attack.left - reach > target.right
    } {
        return None;
    }
    let mut distance = 0.0f32;
    for t in 0..horizon {
        let dx = if rightward { distance } else { -distance };
        // Outward rounding keeps this a reachability bound, not a guessed phase.
        if attack.left + dx.floor() as i32 <= target.right && attack.right + dx.ceil() as i32 >= target.left {
            return Some(t);
        }
        distance += speed * if slow > t + 1 { 0.5 } else { 1.0 };
    }
    None
}

fn bite_damage(contact: i32, slow: i32) -> i32 {
    // Unknown bite residue: allow the first bite immediately on contact, then
    // use native 4/8cs periods. Count only the vulnerable placeholder phase.
    let chilled_frames = (slow - 1).clamp(contact, MORPH) - contact;
    let normal_frames = MORPH - contact - chilled_frames;
    4 * ((chilled_frames + 7) / 8 + (normal_frames + 3) / 4)
}

fn hammer_after_morph(contact: i32, slow: i32) -> bool {
    // 134 normal / 266 chilled cs. 133 normal-speed units plus the event tick;
    // advancing integer half-units also covers slow expiring during the swing.
    let mut units = 0;
    for t in contact..MORPH {
        units += if slow > t + 1 { 1 } else { 2 };
        if units >= 266 {
            return false;
        }
    }
    true
}

fn jack_can_blast(x: f32, y: i32, slow: i32, speed: f32, body: ContactRect) -> bool {
    if y < body.top - 90 || y > body.bottom + 90 {
        return false;
    }
    let mut left = x;
    for t in 0..=EFFECT - 110 {
        // A later or slower Jack may be anywhere in this swept interval.
        let cx = (body.left.max(left.floor() as i32 + 60)).min(x.ceil() as i32 + 60);
        if circle_hits_rect(ContactCircle::new(cx, y, 90), body) {
            return true;
        }
        left -= speed * if slow > t + 1 { 0.5 } else { 1.0 };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bite_budget_and_slow_expiry() {
        assert_eq!(bite_damage(0, 1000), 160);
        assert_eq!(bite_damage(0, 0), 320);
        assert!(bite_damage(24, 0) < 300);
        assert!(bite_damage(0, 100) > bite_damage(0, 1000));
        assert_eq!(bite_damage(MORPH, 0), 0);
    }
    #[test]
    fn new_hammer_has_a_fixed_budget() {
        assert!(!hammer_after_morph(0, 1000));
        assert!(hammer_after_morph(60, 1000));
        assert!(!hammer_after_morph(60, 100));
        assert!(hammer_after_morph(190, 0));
    }
}
