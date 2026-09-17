//! Entity borrows bounded by one shared backend borrow.
use crate::live_value::read_or_abort;
use crate::runtime::RuntimeResult;
use rsvz_backend_api::{
    BoardStateBackend, PlantPlacementBackend, PlantReadBackend, PlantRemoveBackend, SunQueryBackend, ZombieKillBackend,
    ZombieRawFactsBackend, ZombieReadBackend, ZombieRemoveBackend,
};
use rsvz_current::CurrentBackend;
use rsvz_model::{Grid, PlantId, PlantKind, ZombieId, ZombieKind};

/// A protected backend borrow. Creating a Frame does not query Board availability.
///
/// Game examples and borrow failures are checked with
/// `cargo test -p rsvz-game --features backend-tests,rsvz-current/pvz-emulator --doc`.
///
/// ```no_run
/// # #[cfg(feature = "backend-tests")]
/// # {
/// rsvz_game::timeline::at_frame(1, 401, |frame| {
///     for zombie in frame.zombies() {
///         if zombie.hp() < 100 { zombie.remove(); }
///     }
/// });
/// # }
/// ```
///
/// ```compile_fail
/// use std::cell::RefCell;
/// thread_local! { static SAVED: RefCell<Option<rsvz_game::Frame<'static>>> = const { RefCell::new(None) }; }
/// rsvz_game::timeline::at_frame(1, 401, |frame| SAVED.with(|saved| *saved.borrow_mut() = Some(frame)));
/// ```
///
/// ```compile_fail
/// rsvz_game::timeline::at_frame(1, 401, |frame| {
///     std::thread::scope(|scope| { scope.spawn(move || frame.zombies().count()); });
/// });
/// ```
pub struct Frame<'frame> {
    backend: &'frame CurrentBackend,
}

pub(crate) fn with_frame<R>(callback: impl for<'frame> FnOnce(Frame<'frame>) -> R) -> RuntimeResult<R> {
    rsvz_current::with_backend_shared(|backend| callback(Frame { backend }))
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
}

impl Frame<'_> {
    /// Finds the first matching live plant by native pool index at an anchor grid.
    pub fn plant_at(&self, grid: Grid, mut matches: impl FnMut(&PlantRef<'_>) -> bool) -> Option<PlantRef<'_>>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut found: Option<PlantRef<'_>> = None;
        read_or_abort(
            self.backend.for_each_plant_at_anchor_grid(grid, |handle| {
                let plant = PlantRef {
                    backend: self.backend,
                    handle,
                };
                if matches(&plant) && found.as_ref().is_none_or(|old| plant.id().index() < old.id().index()) {
                    found = Some(plant);
                }
                Ok(())
            }),
            "failed to find plant at grid",
        );
        found
    }

    pub fn sun(&self) -> u32
    where
        CurrentBackend: rsvz_backend_api::SunQueryBackend,
    {
        read_or_abort(self.backend.sun(), "failed to read sun")
    }

    pub fn spawn_allowed(&self, kind: ZombieKind) -> bool
    where
        CurrentBackend: rsvz_backend_api::BoardStateBackend,
    {
        read_or_abort(self.backend.spawn_allowed(kind as u32), "failed to read spawn types")
    }

    pub fn can_plant_at(&self, kind: PlantKind, grid: Grid) -> bool
    where
        CurrentBackend: rsvz_backend_api::PlantPlacementBackend,
    {
        let selection = read_or_abort(
            rsvz_model::CardSelection::Plant(kind).checked(),
            "invalid planting kind",
        );
        read_or_abort(self.backend.can_plant_at(selection, grid), "failed to check planting")
            == rsvz_model::Plantability::Allowed
    }

    pub fn plants(&self) -> impl Iterator<Item = PlantRef<'_>> + '_
    where
        CurrentBackend: PlantReadBackend,
    {
        let backend = self.backend;
        read_or_abort(backend.plants(), "failed to iterate plants").map(move |handle| PlantRef { backend, handle })
    }
    pub fn zombies(&self) -> impl Iterator<Item = ZombieRef<'_>> + '_
    where
        CurrentBackend: ZombieReadBackend,
    {
        let backend = self.backend;
        read_or_abort(backend.zombies(), "failed to iterate zombies").map(move |handle| ZombieRef { backend, handle })
    }
    pub fn plant(&self, id: PlantId) -> Option<PlantRef<'_>>
    where
        CurrentBackend: PlantReadBackend,
    {
        read_or_abort(self.backend.plant(id), "failed to resolve plant").map(|handle| PlantRef {
            backend: self.backend,
            handle,
        })
    }
    pub fn zombie(&self, id: ZombieId) -> Option<ZombieRef<'_>>
    where
        CurrentBackend: ZombieReadBackend,
    {
        read_or_abort(self.backend.zombie(id), "failed to resolve zombie").map(|handle| ZombieRef {
            backend: self.backend,
            handle,
        })
    }
}

/// A plant slot that remains address-valid until the enclosing Frame borrow ends.
///
/// ```compile_fail
/// use std::cell::RefCell;
/// thread_local! { static SAVED: RefCell<Option<rsvz_game::PlantRef<'static>>> = const { RefCell::new(None) }; }
/// rsvz_game::timeline::at_frame(1, 401, |frame| {
///     SAVED.with(|saved| *saved.borrow_mut() = frame.plants().next());
/// });
/// ```
pub struct PlantRef<'frame>
where
    CurrentBackend: PlantReadBackend,
{
    backend: &'frame CurrentBackend,
    handle: <CurrentBackend as PlantReadBackend>::PlantHandle<'frame>,
}
impl PlantRef<'_>
where
    CurrentBackend: PlantReadBackend,
{
    pub fn id(&self) -> PlantId {
        self.backend.plant_id(self.handle)
    }
    pub fn hp(&self) -> i32 {
        self.backend.plant_hp(self.handle)
    }
    pub fn kind(&self) -> PlantKind {
        read_or_abort(self.backend.plant_kind(self.handle), "failed to read plant kind")
    }
    pub fn raw_kind(&self) -> PlantKind {
        read_or_abort(
            self.backend.plant_raw_kind(self.handle),
            "failed to read raw plant kind",
        )
    }
    pub fn is_sleeping(&self) -> bool {
        self.backend.plant_is_sleeping(self.handle)
    }
    pub fn state(&self) -> i32 {
        self.backend.plant_state(self.handle)
    }
    pub fn grid(&self) -> Grid {
        crate::plant::grid_from_handle(self.backend, self.handle)
    }
    pub fn is_alive(&self) -> bool {
        self.backend.plant_is_alive(self.handle)
    }
    pub fn remove(&self)
    where
        CurrentBackend: PlantRemoveBackend,
    {
        read_or_abort(self.backend.remove_plant(self.handle), "failed to remove plant");
    }
}

/// A zombie slot that remains address-valid after death until the Frame borrow ends.
///
/// ```compile_fail
/// rsvz_game::timeline::at_frame(1, 401, |frame| {
///     let mut zombies = frame.zombies();
///     rsvz_game::tick::on_frame(move |_| { zombies.next(); });
/// });
/// ```
///
/// ```compile_fail
/// rsvz_game::timeline::at_frame(1, 401, |frame| {
///     let zombie = frame.zombies().next().unwrap();
///     std::thread::scope(|scope| { scope.spawn(move || zombie.hp()); });
/// });
/// ```
pub struct ZombieRef<'frame>
where
    CurrentBackend: ZombieReadBackend,
{
    backend: &'frame CurrentBackend,
    handle: <CurrentBackend as ZombieReadBackend>::ZombieHandle<'frame>,
}
impl ZombieRef<'_>
where
    CurrentBackend: ZombieReadBackend,
{
    pub fn id(&self) -> ZombieId {
        self.backend.zombie_id(self.handle)
    }
    pub fn hp(&self) -> i32 {
        self.backend.zombie_hp(self.handle)
    }
    pub fn kind(&self) -> ZombieKind {
        read_or_abort(self.backend.zombie_kind(self.handle), "failed to read zombie kind")
    }
    pub fn row(&self) -> i32 {
        self.backend.zombie_row(self.handle)
    }
    pub fn age(&self) -> i32 {
        self.backend.zombie_age(self.handle)
    }
    pub fn is_alive(&self) -> bool {
        self.backend.zombie_is_alive(self.handle)
    }
    pub fn remove(&self)
    where
        CurrentBackend: ZombieRemoveBackend,
    {
        read_or_abort(self.backend.remove_zombie(self.handle), "failed to remove zombie");
    }
    pub fn kill(&self)
    where
        CurrentBackend: ZombieKillBackend,
    {
        read_or_abort(self.backend.kill_zombie(self.handle), "failed to kill zombie");
    }
}

impl ZombieRef<'_>
where
    CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    pub fn x(&self) -> f32 {
        self.backend.zombie_pos_x(self.handle)
    }
    pub fn phase(&self) -> rsvz_model::ZombiePhase {
        read_or_abort(self.backend.zombie_phase(self.handle), "failed to read zombie phase")
    }
    pub fn animation_progress(&self) -> Option<f32> {
        read_or_abort(
            self.backend.zombie_reanim_anim_time(self.handle),
            "failed to read zombie animation",
        )
    }
    /// Samples motion from this borrowed slot; a dead object has no active motion state.
    pub fn motion_state(&self) -> Option<rsvz_model::ZombieMotionState> {
        read_or_abort(
            crate::logic::zombie_motion::zombie_motion_state_from_handle(self.backend, self.id(), self.handle),
            "failed to read zombie motion",
        )
    }
    /// Predicts the existing stable-motion model, not future phase changes or interactions.
    pub fn stable_x_trace(&self, horizon_frames: u32) -> Result<Vec<f32>, rsvz_model::ZombieMotionCallError> {
        let state = self
            .motion_state()
            .ok_or(rsvz_model::ZombieMotionCallError::ObjectUnavailable)?;
        crate::logic::zombie_motion::predict_stable_zombie_x_trace(&state, horizon_frames)
            .map_err(rsvz_model::ZombieMotionCallError::Motion)
    }
    /// Predicts one coordinate without allocating a trace.
    pub fn stable_x_at(&self, after_frames: u32) -> Result<f32, rsvz_model::ZombieMotionCallError> {
        let state = self
            .motion_state()
            .ok_or(rsvz_model::ZombieMotionCallError::ObjectUnavailable)?;
        crate::logic::zombie_motion::predict_stable_zombie_x_at(&state, after_frames)
            .map_err(rsvz_model::ZombieMotionCallError::Motion)
    }
}
