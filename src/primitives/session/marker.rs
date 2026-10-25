//! Timeline markers.

use crate::primitives::{MarkerId, TimePosition};

/// A named position on the timeline.
///
/// ```
/// use superb::primitives::session::Marker;
/// use superb::primitives::{MarkerId, TimePosition};
///
/// let marker = Marker::new("verse", TimePosition::from_samples(48_000));
/// assert_eq!(marker.name, "verse");
/// assert_eq!(marker.id(), MarkerId::INVALID);
/// assert_eq!(marker.position().samples(), 48_000);
/// assert_eq!(marker.color, None);
/// let placed = Marker::with_id(MarkerId::from_raw(4), "chorus", TimePosition::ZERO);
/// assert_eq!(placed.id(), MarkerId::from_raw(4));
/// assert_eq!(placed.position().samples(), 0);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub(crate) id: MarkerId,
    /// Display name.
    pub name: String,
    pub(crate) position: TimePosition,
    /// Display colour as `0xRRGGBB`.
    pub color: Option<u32>,
}

impl Marker {
    /// Creates a marker with no ID yet.
    pub fn new(name: impl Into<String>, position: TimePosition) -> Self {
        Self::with_id(MarkerId::INVALID, name, position)
    }

    /// Creates a marker with a pre-allocated ID.
    pub fn with_id(id: MarkerId, name: impl Into<String>, position: TimePosition) -> Self {
        Self {
            id,
            name: name.into(),
            position,
            color: None,
        }
    }

    /// The marker's ID (`MarkerId::INVALID` until added to a memory).
    pub fn id(&self) -> MarkerId {
        self.id
    }

    /// Where the marker sits.
    pub fn position(&self) -> TimePosition {
        self.position
    }
}
