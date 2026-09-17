use rsvz_backend_api::*;
use rsvz_current::CurrentBackend;

fn capabilities<B: CommonZombieDanceBackend + CobImpactDelayBackend + CoinProfileBackend
    + InputBackend + AudioBackend + GardenBackend + TrophyUnlockBackend + HiddenModeUnlockBackend>() {}
fn main() { capabilities::<CurrentBackend>(); }
