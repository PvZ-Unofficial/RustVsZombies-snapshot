//! Open, typed session output reduced and encoded on the cold path.

use std::any::{Any, TypeId};
use std::io::{self, Write};

use crate::error::{RuntimeError, RuntimeResult};

trait ArtifactValue: Any + Send {
    fn value_type_id(&self) -> TypeId;
    fn value_type_name(&self) -> &'static str;
    fn merge_box(&mut self, other: Box<dyn ArtifactValue>) -> RuntimeResult<()>;
    fn write_json(&self, writer: &mut dyn Write) -> io::Result<()>;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send>;
}

struct TypedArtifact<T> {
    value: T,
    merge: fn(&mut T, T) -> RuntimeResult<()>,
    write_json: fn(&T, &mut dyn Write) -> io::Result<()>,
}

impl<T: Send + 'static> ArtifactValue for TypedArtifact<T> {
    fn value_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn value_type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn merge_box(&mut self, other: Box<dyn ArtifactValue>) -> RuntimeResult<()> {
        let other = other
            .into_any()
            .downcast::<Self>()
            .map_err(|_artifact| RuntimeError::new("session artifact source type changed"))?;
        (self.merge)(&mut self.value, other.value)
    }

    fn write_json(&self, writer: &mut dyn Write) -> io::Result<()> {
        (self.write_json)(&self.value, writer)
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any + Send> {
        self
    }
}

/// One type-erased value with its same-type reducer and JSON encoder.
///
/// Construction allocates once and is intended only for session completion.
/// Frame dispatch never touches or clones the contained value.
pub struct SessionArtifact {
    value: Box<dyn ArtifactValue>,
}

impl SessionArtifact {
    pub fn new<T>(
        value: T, merge: fn(&mut T, T) -> RuntimeResult<()>, write_json: fn(&T, &mut dyn Write) -> io::Result<()>,
    ) -> Self
    where
        T: Send + 'static,
    {
        Self {
            value: Box::new(TypedArtifact {
                value,
                merge,
                write_json,
            }),
        }
    }

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        self.value.value_type_name()
    }

    pub fn merge_from(&mut self, other: Self) -> RuntimeResult<()> {
        if self.value.value_type_id() != other.value.value_type_id() {
            return Err(RuntimeError::new(format!(
                "cannot merge session artifact {} into {}",
                other.type_name(),
                self.type_name()
            )));
        }
        self.value.merge_box(other.value)
    }

    pub fn write_json(&self, writer: &mut dyn Write) -> io::Result<()> {
        self.value.write_json(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct UserCounts {
        values: Vec<u32>,
    }

    fn merge_counts(target: &mut UserCounts, mut source: UserCounts) -> RuntimeResult<()> {
        target.values.append(&mut source.values);
        Ok(())
    }

    fn write_counts(value: &UserCounts, writer: &mut dyn Write) -> io::Result<()> {
        write!(writer, "{{\"kind\":\"user\",\"values\":{:?}}}", value.values)
    }

    fn counts(values: &[u32]) -> SessionArtifact {
        SessionArtifact::new(
            UserCounts {
                values: values.to_vec(),
            },
            merge_counts,
            write_counts,
        )
    }

    #[test]
    fn custom_artifacts_reduce_and_encode_a_complete_json_root() {
        let mut artifact = counts(&[1, 2]);
        artifact.merge_from(counts(&[3])).expect("same user type should merge");
        let mut json = Vec::new();
        artifact.write_json(&mut json).expect("user artifact should encode");
        assert_eq!(
            String::from_utf8(json).expect("JSON should be UTF-8"),
            r#"{"kind":"user","values":[1, 2, 3]}"#
        );
    }

    #[test]
    fn different_artifact_types_do_not_merge() {
        fn merge_u32(target: &mut u32, source: u32) -> RuntimeResult<()> {
            *target += source;
            Ok(())
        }
        fn write_u32(value: &u32, writer: &mut dyn Write) -> io::Result<()> {
            write!(writer, "{value}")
        }

        let error = counts(&[1])
            .merge_from(SessionArtifact::new(2u32, merge_u32, write_u32))
            .expect_err("different artifact types must fail");
        assert!(error.to_string().contains("cannot merge session artifact"));
    }
}
