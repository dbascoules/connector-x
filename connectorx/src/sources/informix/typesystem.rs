#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InformixTypeSystem {
    Text(bool),
}

impl_typesystem! {
    system = InformixTypeSystem,
    mappings = {
        { Text => String }
    }
}
