use crate::extensions::Extensions;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: Option<String>,
    pub extensions: Extensions,
}
