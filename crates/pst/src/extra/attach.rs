//! MailCraft: what an attachment holds beyond a file's bytes: an attached message
//! (`afEmbeddedMessage`, MS-OXCMSG 2.2.2.9), kept as MS-PST 2.4.6.2 keeps one, its own
//! recipients and attachments in its own subnode tree. And tagged properties of the
//! attachment's own (`PidTagAttachmentHidden`, `PidTagAttachmentFlags`…).

use super::*;

/// How deep attached messages go: a message attached to a message attached to the one given is
/// two deep.
pub const MAX_EMBEDDED_DEPTH: usize = 32;

/// What an attachment holds.
#[derive(Default)]
pub enum UnicodePstAttachmentContent<'a> {
    /// The file's bytes, [`UnicodePstAttachment::data`].
    #[default]
    Data,
    /// An attached message; `data` is empty and `filename` is its display name.
    Message(Box<UnicodePstEmbeddedMessage<'a>>),
}

/// A message attached to another one, with its own attachments.
pub struct UnicodePstEmbeddedMessage<'a> {
    pub message: UnicodePstMessage<'a>,
    pub attachments: Vec<UnicodePstAttachment<'a>>,
}

/// `PidTagAttachMethod`: `afByValue` and `afEmbeddedMessage`.
pub(crate) const ATTACH_BY_VALUE: i32 = 1;
pub(crate) const ATTACH_EMBEDDED_MESSAGE: i32 = 5;

/// The node IDs attached messages are given, as Outlook gives them: message IDs
/// (`NID_TYPE_NORMAL_MESSAGE`) no other node in the file has, after those of the messages a batch
/// adds; the file's next message ID is moved past them.
pub(crate) struct EmbeddedIds {
    next: u32,
    pub(crate) given: Vec<NodeId>,
}

impl EmbeddedIds {
    /// IDs from `first` on.
    pub(crate) fn new(first: u32) -> Self {
        Self {
            next: first,
            given: Vec::new(),
        }
    }

    /// The next one.
    pub(crate) fn take(&mut self) -> io::Result<NodeId> {
        let node = NodeId::new(NodeIdType::NormalMessage, self.next)?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(PstError::IntegerConversion)?;
        self.given.push(node);
        Ok(node)
    }
}

/// An attached message's node ID while its size is worked out: any takes the same room.
pub(crate) fn sizing_node() -> io::Result<NodeId> {
    Ok(NodeId::new(NodeIdType::NormalMessage, 1)?)
}

/// Properties of an attachment the writer works out itself: its size, its value and its method.
const ATTACHMENT_OWNED: [u16; 3] = [0x0E20, 0x3701, 0x3705];

/// The attached message, when that is what an attachment holds.
pub(crate) fn embedded<'b>(
    input: &'b UnicodePstAttachment<'_>,
) -> Option<&'b UnicodePstEmbeddedMessage<'b>> {
    match &input.content {
        UnicodePstAttachmentContent::Message(message) => Some(message),
        _ => None,
    }
}

/// The size of the file an attachment holds (0 for an attached message).
pub(crate) fn file_size(input: &UnicodePstAttachment<'_>) -> io::Result<usize> {
    match input.content {
        UnicodePstAttachmentContent::Data => Ok(input.data.len()),
        UnicodePstAttachmentContent::Message(_) => Ok(0),
    }
}

/// Attachments are an error unless they can be written: no bytes beside a message, attached
/// messages no deeper than
/// [`MAX_EMBEDDED_DEPTH`] and each a message [`check_message`] takes, added properties tagged,
/// not the writer's own, given once, each small enough for the attachment's one heap page.
/// `depth`: how deep the message they're attached to is (0 for the one given).
pub(crate) fn check_attachments(
    attachments: &[UnicodePstAttachment<'_>],
    depth: usize,
) -> io::Result<()> {
    let invalid = |message: &str| Err(io::Error::new(io::ErrorKind::InvalidInput, message));
    for attachment in attachments {
        match &attachment.content {
            UnicodePstAttachmentContent::Data => {}
            _ if !attachment.data.is_empty() => {
                return invalid("an attachment's bytes are given twice");
            }
            UnicodePstAttachmentContent::Message(message) => {
                if depth + 1 > MAX_EMBEDDED_DEPTH {
                    return invalid("attached messages are nested too deep");
                }
                check_message(&message.message)?;
                check_attachments(&message.attachments, depth + 1)?;
            }
        }
        let mut tags = HashSet::new();
        for property in attachment.properties {
            let UnicodePstPropertyKey::Tag(id) = property.key else {
                return invalid("an attachment's added properties are tagged ones");
            };
            if id == 0 || id >= 0x8000 {
                return invalid("a tagged property's ID must be from 0x0001 to 0x7FFF");
            }
            if ATTACHMENT_OWNED.contains(&id) {
                return invalid("the writer sets this property itself");
            }
            if !tags.insert(id) {
                return invalid("a property is given twice");
            }
            let value = property.value.property_value();
            if small_property_record(&value).is_none()
                && property_value_bytes(&value)?.len() > MAX_HN_ALLOCATION_SIZE
            {
                return invalid("an attachment's added property is too big");
            }
        }
    }
    Ok(())
}

/// Every message attached to `attachments`, at any depth, in order.
pub(crate) fn embedded_messages<'b>(
    attachments: &'b [UnicodePstAttachment<'b>],
    out: &mut Vec<&'b UnicodePstMessage<'b>>,
) {
    for attachment in attachments {
        if let UnicodePstAttachmentContent::Message(message) = &attachment.content {
            out.push(&message.message);
            embedded_messages(&message.attachments, out);
        }
    }
}

/// An attachment's added properties, as the attachment's property context takes them: in place
/// of the writer's own of the same ID.
pub(crate) fn with_attachment_properties(
    input: &UnicodePstAttachment<'_>,
    properties: &mut Vec<(u16, PropertyType, Option<Vec<u8>>, u32)>,
) -> io::Result<()> {
    for property in input.properties {
        let UnicodePstPropertyKey::Tag(id) = property.key else {
            continue;
        };
        let value = property.value.property_value();
        let prop_type = PropertyType::from(&value);
        let record = match small_property_record(&value) {
            Some(small) => (id, prop_type, None, small),
            None => (id, prop_type, Some(property_value_bytes(&value)?), 0),
        };
        properties.retain(|(existing, ..)| *existing != id);
        properties.push(record);
    }
    properties.sort_by_key(|(id, ..)| *id);
    Ok(())
}

#[cfg(test)]
mod tests;
