use chunkedge::block::{BlockKind, BlockState, PropName, PropValue};
use chunkedge::nbt::{Compound, Value};

use crate::error::{Result, SchematicError};

fn strip_namespace(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_ns, path)| path)
}

/// Resolve a block id plus a list of `(property, value)` pairs into a
/// [`BlockState`].
///
/// `name` may include a namespace (`minecraft:stone`) or not (`stone`).
/// Unknown blocks / properties / values are reported as errors rather than
/// silently dropped.
pub fn resolve_state(name: &str, props: &[(&str, &str)]) -> Result<BlockState> {
    let path = strip_namespace(name);

    let kind =
        BlockKind::from_str(path).ok_or_else(|| SchematicError::UnknownBlock(name.to_owned()))?;

    let mut state = kind.to_state();

    for (key, value) in props {
        let prop_name = PropName::from_str(key)
            .ok_or_else(|| SchematicError::UnknownProperty((*key).to_owned()))?;
        let prop_value = PropValue::from_str(value)
            .ok_or_else(|| SchematicError::UnknownPropertyValue((*value).to_owned()))?;
        state = state.set(prop_name, prop_value);
    }

    Ok(state)
}

/// Parse a full block-state string such as
/// `minecraft:oak_stairs[facing=east,half=bottom,waterlogged=false]`
/// into a [`BlockState`].
pub fn parse_state_string(s: &str) -> Result<BlockState> {
    let s = s.trim();

    let Some(open) = s.find('[') else {
        return resolve_state(s, &[]);
    };

    let name = &s[..open];
    let rest = &s[open + 1..];
    let inner = rest
        .strip_suffix(']')
        .ok_or_else(|| SchematicError::Malformed(format!("unterminated state string `{s}`")))?;

    let mut props: Vec<(&str, &str)> = Vec::new();
    if !inner.trim().is_empty() {
        for pair in inner.split(',') {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| SchematicError::Malformed(format!("bad property `{pair}`")))?;
            props.push((k.trim(), v.trim()));
        }
    }

    resolve_state(name, &props)
}

/// Resolve a block from an explicit `Name` plus an optional `Properties`
/// compound. Property values are expected to be NBT strings.
pub fn resolve_from_compound(name: &str, properties: Option<&Compound>) -> Result<BlockState> {
    let mut props: Vec<(&str, &str)> = Vec::new();
    if let Some(properties) = properties {
        for (key, value) in properties.iter() {
            let Value::String(value) = value else {
                return Err(SchematicError::WrongType("Properties value"));
            };
            props.push((key.as_str(), value.as_str()));
        }
    }
    resolve_state(name, &props)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_name() {
        let s = parse_state_string("minecraft:stone").unwrap();
        assert_eq!(s, BlockKind::from_str("stone").unwrap().to_state());
    }

    #[test]
    fn name_without_namespace() {
        assert!(parse_state_string("dirt").is_ok());
    }

    #[test]
    fn with_properties() {
        let s = parse_state_string("minecraft:oak_stairs[facing=east,half=bottom]").unwrap();
        let plain = BlockKind::from_str("oak_stairs").unwrap().to_state();
        assert_ne!(s, plain, "properties should change the state");
    }

    #[test]
    fn unknown_block_errors() {
        assert!(matches!(
            parse_state_string("minecraft:definitely_not_a_block"),
            Err(SchematicError::UnknownBlock(_))
        ));
    }

    #[test]
    fn unterminated_errors() {
        assert!(parse_state_string("minecraft:stone[facing=east").is_err());
    }
}
