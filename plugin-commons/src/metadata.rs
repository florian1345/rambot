use id3::{Content, Tag, TagLike, Timestamp};

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
        // See https://docs.puddletag.net/source/id3.html for keys

        if let Some(content) = to_str(frame.content()) {
            match frame.id() {
                "TIT1" => { meta_builder.set_super_title(content); },
                "TIT2" => {
                    set_title = true;
                    meta_builder.set_title(content);
                },
                "TIT3" => { meta_builder.set_sub_title(content); },
                "TOPE" => { meta_builder.set_artist(content); },
                "TCOM" => { meta_builder.set_composer(content); },
                "TPE1" => { meta_builder.set_lead_performer(content); },
                "TPE2" => { meta_builder.set_group_name(content); },
                "TPE3" => { meta_builder.set_conductor(content); },
                "TPE4" => { meta_builder.set_interpreter(content); },
                "TPUB" => { meta_builder.set_publisher(content); },
                "TALB" => { meta_builder.set_album(content); },
                "TRCK" => {
                    if let Ok(track) = content.parse() {
                        meta_builder.set_track(track);
                    }
                },
                "TDRL" => {
                    if let Ok(timestamp) = content.parse::<Timestamp>() {
                        meta_builder.set_year(timestamp.year);
                    }
                },
                _ => { }
            }
        }
    }

    // TODD can everything be done like this?

    if let Some(genre) = tag.genre_parsed() {
        meta_builder.set_genre(genre);
    }

    if !set_title {
        meta_builder = meta_builder.with_title(descriptor);
    }

    meta_builder.build()
}

#[cfg(test)]
mod tests {

    use id3::{Content, Frame, Tag, TagLike};

    use kernal::prelude::*;

    use crate::metadata_from_id3_tag;

    fn make_tag(frames: impl IntoIterator<Item = (&'static str, &'static str)>) -> Tag {
        let mut tag = Tag::new();

        for (id, content_str) in frames {
            let content = Content::Text(content_str.to_string());
            tag.add_frame(Frame::with_content(id, content));
        }

        tag
    }

    #[test]
    fn metadata_from_empty_tag_sets_correct_title_and_nothing_else() {
        let metadata = metadata_from_id3_tag(Tag::new(), "testDescriptor");

        assert_that!(metadata.title()).contains("testDescriptor");
        assert_that!(metadata.sub_title()).is_none();
        assert_that!(metadata.sub_title()).is_none();
        assert_that!(metadata.super_title()).is_none();
        assert_that!(metadata.artist()).is_none();
        assert_that!(metadata.composer()).is_none();
        assert_that!(metadata.lead_performer()).is_none();
        assert_that!(metadata.group_name()).is_none();
        assert_that!(metadata.conductor()).is_none();
        assert_that!(metadata.lyricist()).is_none();
        assert_that!(metadata.interpreter()).is_none();
        assert_that!(metadata.publisher()).is_none();
        assert_that!(metadata.album()).is_none();
        assert_that!(metadata.track()).is_none();
        assert_that!(metadata.year()).is_none();
        assert_that!(metadata.genre()).is_none();
    }

    #[test]
    fn metadata_from_tag_with_title_sets_correct_title() {
        let tag = make_tag([("TIT2", "testTitle")]);
        let metadata = metadata_from_id3_tag(tag, "testDescriptor");

        assert_that!(metadata.title()).contains("testTitle");
    }

    #[test]
    fn metadata_from_tag_with_genre_sets_correct_genre() {
        let tag = make_tag([("TCON", "(0)")]);
        let metadata = metadata_from_id3_tag(tag, "");

        assert_that!(metadata.genre()).contains("Blues");
    }

    #[test]
    fn metadata_from_tag_with_text_fields_sets_values_correctly() {
        let tag = make_tag([
            ("TIT1", "testSuperTitle"),
            ("TIT3", "testSubTitle"),
            ("TOPE", "testArtist"),
            ("TCOM", "testComposer"),
            ("TPE1", "testLeadPerformer"),
            ("TPE2", "testGroupName"),
            ("TPE3", "testConductor"),
            ("TPE4", "testInterpreter"),
            ("TPUB", "testPublisher"),
            ("TALB", "testAlbum")
        ]);
        let metadata = metadata_from_id3_tag(tag, "");

        assert_that!(metadata.super_title()).contains("testSuperTitle");
        assert_that!(metadata.sub_title()).contains("testSubTitle");
        assert_that!(metadata.artist()).contains("testArtist");
        assert_that!(metadata.composer()).contains("testComposer");
        assert_that!(metadata.lead_performer()).contains("testLeadPerformer");
        assert_that!(metadata.group_name()).contains("testGroupName");
        assert_that!(metadata.conductor()).contains("testConductor");
        assert_that!(metadata.interpreter()).contains("testInterpreter");
        assert_that!(metadata.publisher()).contains("testPublisher");
        assert_that!(metadata.album()).contains("testAlbum");
    }

    #[test]
    fn metadata_from_tag_with_valid_numeric_fields_sets_values_correctly() {
        let tag = make_tag([
            ("TRCK", "123"),
            ("TDRL", "1234"),
        ]);
        let metadata = metadata_from_id3_tag(tag, "");

        assert_that!(metadata.track()).contains(123);
        assert_that!(metadata.year()).contains(1234);
    }

    #[test]
    fn metadata_from_tag_with_invalid_numeric_fields_does_not_set_values() {
        let tag = make_tag([
            ("TRCK", "a"),
            ("TDRL", "b"),
        ]);
        let metadata = metadata_from_id3_tag(tag, "");

        assert_that!(metadata.track()).is_none();
        assert_that!(metadata.year()).is_none();
    }
}
