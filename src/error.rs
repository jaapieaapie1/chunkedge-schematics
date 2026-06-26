use thiserror::Error;

/// Errors produced while reading, parsing, or pasting a schematic.
#[derive(Debug, Error)]
pub enum SchematicError {
    /// Failed to read the file from disk.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The NBT payload could not be decoded.
    #[error("nbt decode error: {0}")]
    Nbt(String),

    /// The file extension / contents did not match any supported format.
    #[error("unrecognized schematic format")]
    UnknownFormat,

    /// A required NBT tag was missing.
    #[error("missing tag `{0}`")]
    MissingTag(&'static str),

    /// A tag was present but had the wrong NBT type.
    #[error("tag `{0}` has wrong type")]
    WrongType(&'static str),

    /// The data was structurally malformed.
    #[error("malformed schematic: {0}")]
    Malformed(String),

    /// A palette entry named a block that ChunkEdge does not know.
    #[error("unknown block `{0}`")]
    UnknownBlock(String),

    /// A palette entry referenced an unknown block-state property name.
    #[error("unknown block property `{0}`")]
    UnknownProperty(String),

    /// A palette entry referenced an unknown block-state property value.
    #[error("unknown block property value `{0}`")]
    UnknownPropertyValue(String),

    /// This format is recognized but support for it is not compiled in
    /// (its cargo feature is disabled) or not yet implemented.
    #[error("schematic format `{0}` is not supported in this build")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, SchematicError>;
