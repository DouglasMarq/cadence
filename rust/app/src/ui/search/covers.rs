use super::cards::CardKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CoverKey {
    pub kind: CardKind,
    pub id: String,
}

impl CoverKey {
    pub fn new(kind: CardKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}
