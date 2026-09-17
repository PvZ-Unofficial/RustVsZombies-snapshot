const MAGIC: &[u8; 8] = b"RSVZSF01";
const FORMAT_VERSION: u16 = 1;
const HEADER_LEN: usize = 60;
const AXIS_COUNT: usize = 4;
const RELEASE_KIND_COUNT: usize = 3;
const JACK_GEOMETRY_COUNT: usize = 5;
const ONLINE_TABLE_BYTES: &[u8] = include_bytes!("../../../data/smart_fodder/v1.bin");

/// Parses the versioned table embedded in the release binary.
pub fn smart_fodder_tables() -> Result<DamageTables<'static>, SmartFodderTableError> {
    DamageTables::parse(ONLINE_TABLE_BYTES)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseTailKind {
    Ladder,
    Football,
    JackNoPop,
}

impl ReleaseTailKind {
    const fn index(self) -> usize {
        match self {
            Self::Ladder => 0,
            Self::Football => 1,
            Self::JackNoPop => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JackGeometry {
    SameRow,
    UpperRow100,
    LowerRow100,
    UpperRow85,
    LowerRow85,
}

impl JackGeometry {
    const fn index(self) -> usize {
        match self {
            Self::SameRow => 0,
            Self::UpperRow100 => 1,
            Self::LowerRow100 => 2,
            Self::UpperRow85 => 3,
            Self::LowerRow85 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Axis {
    min: f32,
    step: f32,
    count: usize,
}

impl Axis {
    fn query(self, x: f32) -> Result<AxisQuery, SmartFodderTableError> {
        if !x.is_finite() {
            return Err(SmartFodderTableError::CoordinateOutOfRange);
        }
        let scaled = (x - self.min) / self.step;
        let last = (self.count - 1) as f32;
        if scaled < -1.0e-4 || scaled > last + 1.0e-4 {
            return Err(SmartFodderTableError::CoordinateOutOfRange);
        }
        let nearest = scaled.round();
        if (scaled - nearest).abs() <= 1.0e-4 {
            let index = usize::try_from(nearest as i64)
                .ok()
                .filter(|index| *index < self.count)
                .ok_or(SmartFodderTableError::CoordinateOutOfRange)?;
            return Ok(AxisQuery::Exact(index));
        }
        let lower = usize::try_from(scaled.floor() as i64)
            .ok()
            .filter(|lower| lower.saturating_add(1) < self.count)
            .ok_or(SmartFodderTableError::CoordinateOutOfRange)?;
        Ok(AxisQuery::Interpolate {
            left: lower,
            right: lower + 1,
            right_weight: scaled - lower as f32,
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum AxisQuery {
    Exact(usize),
    Interpolate {
        left: usize,
        right: usize,
        right_weight: f32,
    },
}

/// Zero-copy view over the versioned online smart-fodder table artifact.
#[derive(Clone, Copy, Debug)]
pub struct DamageTables<'a> {
    bytes: &'a [u8],
    max_h: usize,
    axes: [Axis; AXIS_COUNT],
    release_offsets: [usize; RELEASE_KIND_COUNT],
    pole_offset: usize,
    jack_offset: usize,
}

impl<'a> DamageTables<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, SmartFodderTableError> {
        if bytes.get(..MAGIC.len()) != Some(MAGIC) {
            return Err(SmartFodderTableError::InvalidMagic);
        }
        if read_u16(bytes, 8)? != FORMAT_VERSION {
            return Err(SmartFodderTableError::UnsupportedVersion);
        }
        let max_h = usize::from(read_u16(bytes, 10)?);
        let mut axes = [Axis {
            min: 0.0,
            step: 0.0,
            count: 0,
        }; AXIS_COUNT];
        for (index, axis) in axes.iter_mut().enumerate() {
            let offset = 12 + index * 12;
            *axis = Axis {
                min: read_f32(bytes, offset)?,
                step: read_f32(bytes, offset + 4)?,
                count: usize::from(read_u16(bytes, offset + 8)?),
            };
            if !axis.min.is_finite() || !axis.step.is_finite() || axis.step <= 0.0 || axis.count < 2 {
                return Err(SmartFodderTableError::InvalidAxis);
            }
        }

        let h_count = max_h.checked_add(1).ok_or(SmartFodderTableError::SizeOverflow)?;
        let mut cursor = HEADER_LEN;
        let mut release_offsets = [0; RELEASE_KIND_COUNT];
        for index in 0..RELEASE_KIND_COUNT {
            release_offsets[index] = cursor;
            cursor = advance_f32(cursor, axes[index].count, h_count)?;
        }
        let pole_offset = cursor;
        cursor = advance_f32(cursor, 1, h_count)?;
        let jack_offset = cursor;
        cursor = advance_f32(jack_offset, JACK_GEOMETRY_COUNT * axes[3].count, h_count)?;
        if cursor != bytes.len() {
            return Err(SmartFodderTableError::InvalidLength);
        }
        Ok(Self {
            bytes,
            max_h,
            axes,
            release_offsets,
            pole_offset,
            jack_offset,
        })
    }

    #[must_use]
    pub const fn max_h(self) -> usize {
        self.max_h
    }

    pub fn release_damage(self, kind: ReleaseTailKind, x: f32, h: usize) -> Result<f32, SmartFodderTableError> {
        self.check_h(h)?;
        let kind_index = kind.index();
        let axis = self.axes[kind_index];
        let query = axis.query(x)?;
        self.query_axis(self.release_offsets[kind_index], axis.count, h, query)
    }

    pub(crate) fn release_x_query(
        self, kind: ReleaseTailKind, x: f32,
    ) -> Result<(usize, Option<(usize, f64)>), SmartFodderTableError> {
        match self.axes[kind.index()].query(x)? {
            AxisQuery::Exact(index) => Ok((index, None)),
            AxisQuery::Interpolate {
                left,
                right,
                right_weight,
            } => Ok((left, Some((right, f64::from(right_weight))))),
        }
    }

    pub(crate) fn release_damage_grid(
        self, kind: ReleaseTailKind, x_index: usize, h: usize,
    ) -> Result<f32, SmartFodderTableError> {
        self.check_h(h)?;
        let kind_index = kind.index();
        let axis = self.axes[kind_index];
        if x_index >= axis.count {
            return Err(SmartFodderTableError::CoordinateOutOfRange);
        }
        self.query_axis(
            self.release_offsets[kind_index],
            axis.count,
            h,
            AxisQuery::Exact(x_index),
        )
    }

    pub fn pole_damage(self, h: usize) -> Result<f32, SmartFodderTableError> {
        self.check_h(h)?;
        self.value(self.pole_offset, h)
    }

    pub fn jack_danger(
        self, geometry: JackGeometry, x: f32, after_release: usize,
    ) -> Result<f32, SmartFodderTableError> {
        self.check_h(after_release)?;
        let axis = self.axes[3];
        let query = axis.query(x)?;
        let geometry_base = geometry
            .index()
            .checked_mul(axis.count)
            .and_then(|index| index.checked_mul(self.max_h + 1))
            .and_then(|offset| offset.checked_mul(size_of::<f32>()))
            .and_then(|offset| self.jack_offset.checked_add(offset))
            .ok_or(SmartFodderTableError::SizeOverflow)?;
        self.query_axis(geometry_base, axis.count, after_release, query)
    }

    pub(crate) fn jack_x_query(self, x: f32) -> Result<(usize, Option<(usize, f64)>), SmartFodderTableError> {
        match self.axes[3].query(x)? {
            AxisQuery::Exact(index) => Ok((index, None)),
            AxisQuery::Interpolate {
                left,
                right,
                right_weight,
            } => Ok((left, Some((right, f64::from(right_weight))))),
        }
    }

    pub(crate) const fn jack_x_count(self) -> usize {
        self.axes[3].count
    }

    pub(crate) fn jack_danger_grid(
        self, geometry: JackGeometry, x_index: usize, after_release: usize,
    ) -> Result<f32, SmartFodderTableError> {
        self.check_h(after_release)?;
        let axis = self.axes[3];
        if x_index >= axis.count {
            return Err(SmartFodderTableError::CoordinateOutOfRange);
        }
        let geometry_base = geometry
            .index()
            .checked_mul(axis.count)
            .and_then(|index| index.checked_mul(self.max_h + 1))
            .and_then(|offset| offset.checked_mul(size_of::<f32>()))
            .and_then(|offset| self.jack_offset.checked_add(offset))
            .ok_or(SmartFodderTableError::SizeOverflow)?;
        self.query_axis(geometry_base, axis.count, after_release, AxisQuery::Exact(x_index))
    }

    fn query_axis(
        self, base: usize, _x_count: usize, h: usize, query: AxisQuery,
    ) -> Result<f32, SmartFodderTableError> {
        let h_count = self.max_h + 1;
        let at = |x_index: usize| {
            x_index
                .checked_mul(h_count)
                .and_then(|offset| offset.checked_add(h))
                .ok_or(SmartFodderTableError::SizeOverflow)
                .and_then(|index| self.value(base, index))
        };
        match query {
            AxisQuery::Exact(index) => at(index),
            AxisQuery::Interpolate {
                left,
                right,
                right_weight,
            } => {
                let left = at(left)?;
                Ok((at(right)? - left).mul_add(right_weight, left))
            }
        }
    }

    fn check_h(self, h: usize) -> Result<(), SmartFodderTableError> {
        if h <= self.max_h {
            Ok(())
        } else {
            Err(SmartFodderTableError::HorizonOutOfRange)
        }
    }

    fn value(self, base: usize, index: usize) -> Result<f32, SmartFodderTableError> {
        let offset = index
            .checked_mul(size_of::<f32>())
            .and_then(|offset| base.checked_add(offset))
            .ok_or(SmartFodderTableError::SizeOverflow)?;
        read_f32(self.bytes, offset)
    }
}

fn advance_f32(cursor: usize, outer: usize, inner: usize) -> Result<usize, SmartFodderTableError> {
    outer
        .checked_mul(inner)
        .and_then(|count| count.checked_mul(size_of::<f32>()))
        .and_then(|bytes| cursor.checked_add(bytes))
        .ok_or(SmartFodderTableError::SizeOverflow)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, SmartFodderTableError> {
    let raw: [u8; 2] = bytes
        .get(offset..offset + 2)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(SmartFodderTableError::Truncated)?;
    Ok(u16::from_le_bytes(raw))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, SmartFodderTableError> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(SmartFodderTableError::Truncated)?;
    let value = f32::from_le_bytes(raw);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(SmartFodderTableError::NonFiniteValue)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmartFodderTableError {
    #[error("smart-fodder table header is truncated")]
    Truncated,
    #[error("smart-fodder table magic is invalid")]
    InvalidMagic,
    #[error("smart-fodder table format version is unsupported")]
    UnsupportedVersion,
    #[error("smart-fodder table axis is invalid")]
    InvalidAxis,
    #[error("smart-fodder table length does not match its axes")]
    InvalidLength,
    #[error("smart-fodder table contains a non-finite value")]
    NonFiniteValue,
    #[error("smart-fodder table size overflows the host address space")]
    SizeOverflow,
    #[error("smart-fodder release coordinate is outside the measured support")]
    CoordinateOutOfRange,
    #[error("smart-fodder query horizon is outside the measured support")]
    HorizonOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(max_h: u16) -> Vec<u8> {
        let axes = [(100.0, 3.0, 2_u16); AXIS_COUNT];
        let h_count = usize::from(max_h) + 1;
        let value_count = RELEASE_KIND_COUNT * 2 * h_count + h_count + JACK_GEOMETRY_COUNT * 2 * h_count;
        let mut bytes = Vec::with_capacity(HEADER_LEN + value_count * 4);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&max_h.to_le_bytes());
        for (min, step, count) in axes {
            bytes.extend_from_slice(&f32::to_le_bytes(min));
            bytes.extend_from_slice(&f32::to_le_bytes(step));
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(&0_u16.to_le_bytes());
        }
        for value in 0..value_count {
            bytes.extend_from_slice(&(value as f32).to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parses_versioned_zero_copy_table_and_queries_all_sections() {
        let bytes = fixture(2);
        let tables = DamageTables::parse(&bytes).expect("table");

        assert_eq!(tables.max_h(), 2);
        assert_eq!(tables.release_damage(ReleaseTailKind::Ladder, 100.0, 1), Ok(1.0));
        assert_eq!(tables.release_damage(ReleaseTailKind::Ladder, 101.0, 1), Ok(2.0));
        assert_eq!(tables.release_damage(ReleaseTailKind::Ladder, 102.0, 1), Ok(3.0));
        assert_eq!(tables.pole_damage(2), Ok(20.0));
        assert_eq!(tables.jack_danger(JackGeometry::SameRow, 100.0, 0), Ok(21.0));
        assert_eq!(tables.jack_danger(JackGeometry::SameRow, 101.0, 0), Ok(22.0));
        assert_eq!(tables.jack_danger(JackGeometry::SameRow, 102.0, 0), Ok(23.0));
    }

    #[test]
    fn rejects_out_of_support_queries_instead_of_clamping() {
        let bytes = fixture(2);
        let tables = DamageTables::parse(&bytes).expect("table");

        assert_eq!(
            tables.release_damage(ReleaseTailKind::Football, 99.0, 0),
            Err(SmartFodderTableError::CoordinateOutOfRange)
        );
        assert_eq!(tables.pole_damage(3), Err(SmartFodderTableError::HorizonOutOfRange));
    }

    #[test]
    fn parser_rejects_trailing_or_truncated_payload() {
        let mut trailing = fixture(1);
        trailing.push(0);
        assert!(matches!(
            DamageTables::parse(&trailing),
            Err(SmartFodderTableError::InvalidLength)
        ));
        assert!(matches!(
            DamageTables::parse(&fixture(1)[..20]),
            Err(SmartFodderTableError::Truncated)
        ));
    }
}
