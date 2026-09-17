use super::*;

impl PlantReadBackend for PeBackend {
    type PlantHandle<'a> = crate::handles::PePlantHandle<'a>;
    type PlantIter<'a> = crate::handles::PePlantIter<'a>;

    fn plants(&self) -> Result<Self::PlantIter<'_>> {
        self.with_current_world(|world| {
            let pool = crate::access::plant_pool(world);
            let limit = pool.max_used_count();
            crate::handles::PePlantIter::new(self, limit)
        })
    }

    fn plant(&self, id: PlantId) -> Result<Option<Self::PlantHandle<'_>>> {
        self.with_current_world(|world| {
            self.plant_handle_from_pool(crate::access::plant_pool(world), id)
                .filter(|handle| self.pe_plant_is_live(*handle))
        })
    }

    fn plant_id<'a>(&'a self, handle: Self::PlantHandle<'a>) -> PlantId {
        self.plant_id_from_handle(handle)
    }

    fn plant_kind<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<PlantKind> {
        let raw = pe_raw_plant_type(read_pe_raw_field!(handle.as_ptr(), type_))?;
        let effective = if raw == pe_rs::PlantType::Imitater {
            pe_raw_plant_type(read_pe_raw_field!(handle.as_ptr(), imitater_target))?
        } else {
            raw
        };
        crate::convert::kind::plant_from_pe(effective)
    }

    fn plant_raw_kind<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<PlantKind> {
        pe_plant_kind_from_handle(handle)
    }

    fn plant_x<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), x)
    }

    fn plant_state<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        normalize_plant_state(read_pe_raw_field!(handle.as_ptr(), status))
    }

    fn plant_is_squished<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool {
        read_pe_raw_field!(handle.as_ptr(), is_smashed)
    }

    fn plant_on_bungee_state<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), edible)
    }

    fn plant_is_sleeping<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool {
        read_pe_raw_field!(handle.as_ptr(), is_sleeping)
    }

    fn plant_wake_up_counter<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), countdown.awake)
    }

    fn plant_state_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), countdown.status)
    }

    fn plant_effect_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), countdown.effect)
    }

    fn plant_disappear_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), countdown.dead)
    }

    fn plant_eating_flash_counter<'a>(&'a self, _handle: Self::PlantHandle<'a>) -> i32 {
        // PE does not model cosmetic eating flashes; preserve its explicit zero semantics.
        0
    }

    fn plant_reanim_circulation<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<Option<f32>> {
        Ok(Some(read_pe_raw_field!(handle.as_ptr(), reanim.progress)))
    }

    fn plant_hp<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), hp)
    }

    fn plant_row<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), row) as i32
    }
    fn plant_col<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), col) as i32
    }

    fn plant_is_alive<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool {
        self.pe_plant_is_live(handle)
    }

    /// The callback borrows cannot escape the installed backend scope.
    ///
    /// ```compile_fail
    /// use rsvz_backend_api::PlantReadBackend;
    /// let escaped = rsvz_pvz_emulator_backend::with_backend(|backend| {
    ///     let mut saved = None;
    ///     backend.for_each_plant_at_grid(rsvz_model::Grid { row: 0, col: 0 }, |plant| {
    ///         saved = Some(plant);
    ///         Ok(())
    ///     }).unwrap();
    ///     saved
    /// });
    /// ```
    fn for_each_plant_at_grid<'a, F>(&'a self, grid: Grid, mut visit: F) -> Result<()>
    where
        F: FnMut(Self::PlantHandle<'a>) -> Result<()>,
    {
        let mut visited = [std::ptr::null_mut(); 4];
        let mut visited_len = 0;
        self.visit_plant_map_grid(grid, &mut visited, &mut visited_len, &mut visit)
    }
    fn for_each_plant_at_anchor_grid<'a, F>(&'a self, grid: Grid, mut visit: F) -> Result<()>
    where
        F: FnMut(Self::PlantHandle<'a>) -> Result<()>,
    {
        let mut visited = [std::ptr::null_mut(); 4];
        let mut visited_len = 0;
        self.visit_plant_map_grid(grid, &mut visited, &mut visited_len, &mut |plant| {
            if self.plant_row(plant) == grid.row && self.plant_col(plant) == grid.col {
                visit(plant)?;
            }
            Ok(())
        })
    }
}

impl PlantVisualStateBackend for PeBackend {
    fn set_plant_eating_flash_counter<'a>(&'a self, _plant: Self::PlantHandle<'a>, _value: i32) -> Result<()> {
        Ok(())
    }

    fn update_plant_reanim_color<'a>(&'a self, _plant: Self::PlantHandle<'a>) -> Result<()> {
        Ok(())
    }
}

impl ImitatorMorphBackend for PeBackend {
    fn imitator_morph_successor(&self, placeholder: PlantId) -> Result<Option<PlantId>> {
        self.with_current_world(|world| world.imitater_morph_successor(placeholder.raw()).map(PlantId::from_raw))
    }
}

impl GridItemReadBackend for PeBackend {
    type GridItemHandle<'a> = crate::handles::PeGridItemHandle<'a>;
    type GridItemIter<'a> = crate::handles::PeGridItemIter<'a>;

    fn grid_items(&self) -> Result<Self::GridItemIter<'_>> {
        self.with_current_world(|world| {
            let pool = crate::access::grid_item_pool(world);
            let limit = pool.max_used_count();
            crate::handles::PeGridItemIter::new(self, limit)
        })
    }

    fn grid_item_id<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> GridItemId {
        self.grid_item_id_from_handle(handle)
    }

    fn grid_item_kind<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<GridItemKind> {
        pe_grid_item_kind_from_handle(handle)
    }

    fn grid_item_row<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), row) as i32
    }
    fn grid_item_col<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), col) as i32
    }

    fn grid_item_is_alive<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> bool {
        self.pe_grid_item_is_live(handle)
    }
}

impl PlantContactBackend for PeBackend {
    fn plant_hit_box<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect> {
        let raw = self.with_current_world(|world| {
            // SAFETY: the raw pointer is bound to this current backend borrow.
            world.plant_hit_box(unsafe { pe_rs::PlantRef::from_non_null(plant.ptr) })
        })?;
        pe_contact_rect(raw)
    }

    fn plant_attack_rect<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> Result<ContactRect> {
        let raw = self.with_current_world(|world| {
            // SAFETY: the raw pointer is bound to this current backend borrow.
            world.plant_attack_rect(
                unsafe { pe_rs::PlantRef::from_non_null(plant.ptr) },
                pe_plant_weapon(weapon),
            )
        })?;
        pe_contact_rect(raw)
    }

    fn plant_damage_range_flags<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> DamageRangeFlags {
        // SAFETY: the live handle and typed weapon satisfy the scalar atom's preconditions.
        let bits = unsafe { pe_rs::raw::pe_rs_plant_damage_range_flags(plant.as_ptr(), weapon as i32) };
        DamageRangeFlags::from_bits_retain(bits as u32)
    }
}
