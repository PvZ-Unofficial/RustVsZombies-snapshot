macro_rules! ffi_enum {
    ($(#[$meta:meta])* pub enum $name:ident { $($variant:ident = $raw:expr),+ $(,)? }) => {
        #[repr(i32)]
        $(#[$meta])*
        pub enum $name {
            $($variant = $raw),+
        }

        impl $name {
            pub const fn to_raw(self) -> i32 {
                self as i32
            }

            pub const fn from_raw(value: i32) -> Option<Self> {
                match value {
                    $($raw => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum SceneType {
        Day = 0,
        Night = 1,
        Pool = 2,
        Fog = 3,
        Roof = 4,
        MoonNight = 5,
        MushroomGarden = 6,
    }
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum PlantType {
        None = -1,
        PeaShooter = 0x0,
        Sunflower = 0x1,
        CherryBomb = 0x2,
        WallNut = 0x3,
        PotatoMine = 0x4,
        SnowPea = 0x5,
        Chomper = 0x6,
        Repeater = 0x7,
        PuffShroom = 0x8,
        SunShroom = 0x9,
        FumeShroom = 0xA,
        GraveBuster = 0xB,
        HypnoShroom = 0xC,
        ScaredyShroom = 0xD,
        IceShroom = 0xE,
        DoomShroom = 0xF,
        LilyPad = 0x10,
        Squash = 0x11,
        Threepeater = 0x12,
        TangleKelp = 0x13,
        Jalapeno = 0x14,
        Spikeweed = 0x15,
        Torchwood = 0x16,
        TallNut = 0x17,
        SeaShroom = 0x18,
        Plantern = 0x19,
        Cactus = 0x1A,
        Blover = 0x1B,
        SplitPea = 0x1C,
        Starfruit = 0x1D,
        Pumpkin = 0x1E,
        MagnetShroom = 0x1F,
        CabbagePult = 0x20,
        FlowerPot = 0x21,
        KernelPult = 0x22,
        CoffeeBean = 0x23,
        Garlic = 0x24,
        UmbrellaLeaf = 0x25,
        Marigold = 0x26,
        MelonPult = 0x27,
        GatlingPea = 0x28,
        TwinSunflower = 0x29,
        GloomShroom = 0x2A,
        Cattail = 0x2B,
        WinterMelon = 0x2C,
        GoldMagnet = 0x2D,
        Spikerock = 0x2E,
        CobCannon = 0x2F,
        Imitater = 0x30,
    }
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ZombieType {
        None = -1,
        Normal = 0x0,
        Flag = 0x1,
        Conehead = 0x2,
        PoleVaulting = 0x3,
        Buckethead = 0x4,
        Newspaper = 0x5,
        ScreenDoor = 0x6,
        Football = 0x7,
        Dancing = 0x8,
        BackupDancer = 0x9,
        DuckyTube = 0xA,
        Snorkel = 0xB,
        Zomboni = 0xC,
        DolphinRider = 0xE,
        JackInTheBox = 0xF,
        Balloon = 0x10,
        Digger = 0x11,
        Pogo = 0x12,
        Yeti = 0x13,
        Bungee = 0x14,
        Ladder = 0x15,
        Catapult = 0x16,
        Gargantuar = 0x17,
        Imp = 0x18,
        GigaGargantuar = 0x20,
    }
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum GridItemType {
        Grave = 0x1,
        Crater = 0x2,
        Ladder = 0x3,
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlantWeapon {
    Primary = 0,
    Secondary = 1,
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum PlantingReason {
        Ok = 0,
        NotHere = 1,
        OnlyOnGraves = 2,
        OnlyInPool = 3,
        OnlyOnGround = 4,
        NeedsPot = 5,
        NotOnArt = 6,
        NotPassedLine = 7,
        NeedsUpgrade = 8,
        NotOnGrave = 9,
        NotOnCrater = 10,
        NotOnWater = 11,
        NeedsGround = 12,
        NeedsSleeping = 13,
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelPultRule {
    Normal = 0,
    AlwaysButter = 1,
    AlwaysKernel = 2,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlantDamageRule {
    Normal = 0,
    Invincible = 1,
    Weak = 2,
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum MaidCheat {
        Stop = 0,
        CallPartner = 1,
        Dancing = 2,
        Move = 3,
    }
}

ffi_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum ZombieDanceCheat {
        None = 0,
        Fast = 1,
        Slow = 2,
    }
}
