use crate::model::{AssumedWavelength, RelativeTime, Wave};

/// Converts script-facing wave/time declarations into relative times.
pub trait IntoRelativeTime {
    fn into_relative_time(self) -> RelativeTime;
}

impl IntoRelativeTime for RelativeTime {
    fn into_relative_time(self) -> RelativeTime {
        self
    }
}

impl IntoRelativeTime for (Wave, i32) {
    fn into_relative_time(self) -> RelativeTime {
        RelativeTime::new(self.0, self.1)
    }
}

impl IntoRelativeTime for (i32, i32) {
    fn into_relative_time(self) -> RelativeTime {
        RelativeTime::new(Wave(self.0), self.1)
    }
}

/// Converts script-facing wavelength declarations into assumed wavelengths.
pub trait IntoAssumedWavelength {
    fn into_assumed_wavelength(self) -> AssumedWavelength;
}

impl IntoAssumedWavelength for AssumedWavelength {
    fn into_assumed_wavelength(self) -> AssumedWavelength {
        self
    }
}

impl IntoAssumedWavelength for (Wave, i32) {
    fn into_assumed_wavelength(self) -> AssumedWavelength {
        AssumedWavelength {
            wave: self.0,
            length: self.1,
        }
    }
}

impl IntoAssumedWavelength for (i32, i32) {
    fn into_assumed_wavelength(self) -> AssumedWavelength {
        AssumedWavelength {
            wave: Wave(self.0),
            length: self.1,
        }
    }
}
