use id3::{Content, Tag, Timestamp};

use rambot_api::{AudioMetadata, AudioMetadataBuilder};

fn to_str(content: &Content) -> Option<String> {
    match content {
        Content::Text(t) => Some(t.clone()),
        Content::ExtendedText(t) => Some(t.value.clone()),
        Content::Link(l) => Some(l.clone()),
        Content::ExtendedLink(l) => Some(l.link.clone()),
        Content::Comment(c) => Some(c.text.clone()),
        Content::Lyrics(l) => Some(l.text.clone()),
        Content::Unknown(u) => String::from_utf8(u.data.clone()).ok(),
        _ => None
    }
}

/// Converts an ID3 [Tag] into [AudioMetadata].
///
/// # Arguments
///
/// * `tag`: The ID3 [Tag] to convert.
/// * `descriptor`: The descriptor of the audio source, which is used as a
/// fallback title in case the given `tag` contains none.
///
/// # Returns
///
/// A newly constructed [AudioMetadata] instance filled with information from
/// the given tag.
pub fn metadata_from_id3_tag(tag: Tag, descriptor: &str) -> AudioMetadata {
    let mut meta_builder = AudioMetadataBuilder::new();
    let mut set_title = false;

    for frame in tag.frames() {
        // See https://id3.org/id3v2.4.0-frames for keys

        if let Some(content) = to_str(frame.content()) {
            match frame.id() {
                "TIT2" => {
                    set_title = true;
                    meta_builder = meta_builder.with_title(content);
                },
                "TOPE" => meta_builder = meta_builder.with_artist(content),
                "TALB" => meta_builder = meta_builder.with_album(content),
                "TDRL" => {
                    if let Ok(timestamp) = content.parse::<Timestamp>() {
                        meta_builder = meta_builder.with_year(timestamp.year);
                    }
                },
                _ => { }
            }
        }
    }

    if !set_title {
        meta_builder = meta_builder.with_title(descriptor);
    }

    meta_builder.build()
}
