//! Cross-frame object IDs.

macro_rules! data_array_id {
    ($name:ident) => {
        #[doc = "A cross-frame object identifier that must be re-resolved before use."]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $name {
            /// 构造后端已验证的 raw object ID。
            #[doc(hidden)]
            #[inline(always)]
            pub const fn from_raw(raw: u32) -> Self {
                Self(raw)
            }

            /// 返回后端内部 object ID。
            #[doc(hidden)]
            #[inline(always)]
            pub const fn raw(self) -> u32 {
                self.0
            }

            /// 后端对象槽位索引。
            #[doc(hidden)]
            #[inline(always)]
            pub const fn index(self) -> u16 {
                (self.0 & 0xffff) as u16
            }

            /// 后端对象 generation/key。
            #[doc(hidden)]
            #[inline(always)]
            pub const fn generation(self) -> u16 {
                (self.0 >> 16) as u16
            }
        }
    };
}

data_array_id!(PlantId);
data_array_id!(ZombieId);
data_array_id!(ItemId);
data_array_id!(GridItemId);
data_array_id!(ProjectileId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_id_splits_generation_and_index() {
        let id = ZombieId::from_raw(0x03e9_0007);
        assert_eq!(id.index(), 7);
        assert_eq!(id.generation(), 1001);
        assert_eq!(id.raw(), 0x03e9_0007);
    }
}
