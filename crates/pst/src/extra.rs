//! MailCraft: what a message carries beyond the writer's own list of properties: its class and
//! any tagged property, in its property context and in the cells of its folder's contents table.

use super::*;

/// A value of a property added to a message ([`UnicodePstMessage::properties`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnicodePstValue<'a> {
    /// `PtypInteger16`.
    Integer16(i16),
    /// `PtypInteger32`.
    Integer32(i32),
    /// `PtypBoolean`.
    Boolean(bool),
    /// `PtypTime`: 100-nanosecond intervals since 1601-01-01 UTC.
    Time(i64),
    /// `PtypString`.
    Unicode(&'a str),
    /// `PtypBinary`.
    Binary(&'a [u8]),
    /// `PtypMultipleString`.
    MultipleUnicode(&'a [&'a str]),
    /// `PtypMultipleBinary`.
    MultipleBinary(&'a [&'a [u8]]),
}

impl UnicodePstValue<'_> {
    pub(crate) fn property_value(&self) -> PropertyValue {
        let unicode = |value: &str| UnicodeValue::new(value.encode_utf16().collect());
        match *self {
            Self::Integer16(value) => PropertyValue::Integer16(value),
            Self::Integer32(value) => PropertyValue::Integer32(value),
            Self::Boolean(value) => PropertyValue::Boolean(value),
            Self::Time(value) => PropertyValue::Time(value),
            Self::Unicode(value) => PropertyValue::Unicode(unicode(value)),
            Self::Binary(value) => PropertyValue::Binary(BinaryValue::new(value.to_vec())),
            // Each string ends in a NUL, as Outlook writes them; readers drop it.
            Self::MultipleUnicode(values) => PropertyValue::MultipleUnicode(
                values
                    .iter()
                    .map(|value| UnicodeValue::new(value.encode_utf16().chain(Some(0)).collect()))
                    .collect(),
            ),
            Self::MultipleBinary(values) => PropertyValue::MultipleBinary(
                values
                    .iter()
                    .map(|value| BinaryValue::new(value.to_vec()))
                    .collect(),
            ),
        }
    }
}

/// Which property a value is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnicodePstPropertyKey {
    /// A tagged property, by its ID (below 0x8000).
    Tag(u16),
}

/// A property added to a message: it replaces the writer's own value of the same property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnicodePstProperty<'a> {
    pub key: UnicodePstPropertyKey,
    pub value: UnicodePstValue<'a>,
}

impl<'a> UnicodePstProperty<'a> {
    /// The tagged property `id`.
    pub fn tagged(id: u16, value: UnicodePstValue<'a>) -> Self {
        Self {
            key: UnicodePstPropertyKey::Tag(id),
            value,
        }
    }
}

/// `PidTagMessageFlags`' `mfRead` and `mfHasAttach` (MS-OXCMSG 2.2.1.6).
pub(crate) const MESSAGE_FLAG_READ: i32 = 0x01;
const MESSAGE_FLAG_HAS_ATTACH: i32 = 0x10;

/// Properties the writer works out from the rest and keeps right: a message's class is
/// [`UnicodePstMessage::message_class`], its size, whether it has attachments and its row ID and
/// version are the writer's.
const WRITER_OWNED: [u16; 5] = [
    0x001A,
    0x0E08,
    0x0E1B,
    LTP_ROW_ID_PROP_ID,
    LTP_ROW_VERSION_PROP_ID,
];

/// A message's class and added properties are an error unless they can be written: a class,
/// tagged IDs below 0x8000 (named ones have their own range), none of the writer's own, each
/// once.
pub(crate) fn check_message(input: &UnicodePstMessage<'_>) -> io::Result<()> {
    let invalid = |message: &str| Err(io::Error::new(io::ErrorKind::InvalidInput, message));
    if input.message_class.is_empty() {
        return invalid("message class must not be empty");
    }
    let mut seen = HashSet::new();
    for property in input.properties {
        let UnicodePstPropertyKey::Tag(id) = property.key;
        if id == 0 || id >= 0x8000 {
            return invalid("a tagged property's ID must be from 0x0001 to 0x7FFF");
        }
        if WRITER_OWNED.contains(&id) {
            return invalid("the writer sets this property itself");
        }
        if !seen.insert(id) {
            return invalid("a property is given twice");
        }
    }
    Ok(())
}

/// The value given for the tagged property `id`.
fn tagged_value<'a>(input: &UnicodePstMessage<'a>, id: u16) -> Option<UnicodePstValue<'a>> {
    input
        .properties
        .iter()
        .find(|property| property.key == UnicodePstPropertyKey::Tag(id))
        .map(|property| property.value)
}

/// `PidTagMessageFlags`: as given (read when not), with `mfHasAttach` as the message is.
pub(crate) fn message_flags(input: &UnicodePstMessage<'_>, has_attachments: bool) -> i32 {
    let given = match tagged_value(input, 0x0E07) {
        Some(UnicodePstValue::Integer32(flags)) => flags,
        _ => MESSAGE_FLAG_READ,
    };
    let attach = if has_attachments {
        MESSAGE_FLAG_HAS_ATTACH
    } else {
        0
    };
    (given & !MESSAGE_FLAG_HAS_ATTACH) | attach
}

/// Whether a message goes in unread.
pub(crate) fn unread(input: &UnicodePstMessage<'_>) -> bool {
    message_flags(input, false) & MESSAGE_FLAG_READ == 0
}

/// The writer's own properties of a message (`properties`) with the added ones in place of the
/// same IDs'.
pub(crate) fn with_added(
    input: &UnicodePstMessage<'_>,
    mut properties: Vec<(u16, PropertyValue)>,
) -> Vec<(u16, PropertyValue)> {
    for property in input.properties {
        let UnicodePstPropertyKey::Tag(id) = property.key;
        // PidTagMessageFlags keeps mfHasAttach, which the writer sets.
        if id == 0x0E07 {
            continue;
        }
        properties.retain(|(existing, _)| *existing != id);
        properties.push((id, property.value.property_value()));
    }
    properties
}

/// The contents table's cell for `column` from an added property: `None` when none is given for
/// it, `Some(None)` when the column's type isn't the value's.
pub(crate) fn added_table_cell(
    input: &UnicodePstMessage<'_>,
    column: &TableColumnDescriptor,
) -> io::Result<Option<Option<TableCell>>> {
    if column.prop_id() == 0x0E07 {
        return Ok(None);
    }
    let Some(value) = tagged_value(input, column.prop_id()) else {
        return Ok(None);
    };
    let prop_type = column.prop_type();
    if let (UnicodePstValue::Unicode(text), PropertyType::String8) = (value, prop_type) {
        return Ok(Some(string_table_cell(prop_type, text)));
    }
    let value = value.property_value();
    if PropertyType::from(&value) != prop_type {
        return Ok(Some(None));
    }
    Ok(Some(Some(if table_pointer_type(prop_type) {
        TableCell::Bytes(property_value_bytes(&value)?)
    } else {
        TableCell::Value(value)
    })))
}

#[cfg(test)]
mod tests;
