use serde_json::{Value, json};
use ytm_core::models::{LyricLine, Lyrics};
use ytm_core::parsers::lyrics::{
    parse_lrc, parse_lyrics, parse_lyrics_browse_id, parse_timed_lyrics,
};

#[test]
fn finds_the_lyrics_tab_by_browse_id() {
    let next: Value = serde_json::from_str(include_str!("fixtures/lyrics_next.json")).unwrap();
    assert_eq!(parse_lyrics_browse_id(&next), Some("MPLYt-test"));
    assert_eq!(parse_lyrics_browse_id(&json!({})), None);
}

#[test]
fn joins_plain_text_runs_and_attribution() {
    let browse: Value = serde_json::from_str(include_str!("fixtures/lyrics_browse.json")).unwrap();
    assert_eq!(
        parse_lyrics(&browse),
        Lyrics {
            text: "First line\nSecond line".into(),
            source: Some("Source: Test provider".into()),
        }
    );
    assert_eq!(parse_lyrics(&json!({})).text, "");
}

#[test]
fn reads_nested_timing_in_start_order_and_skips_bad_rows() {
    let data: Value = serde_json::from_str(include_str!("fixtures/lyrics_timed.json")).unwrap();
    assert_eq!(
        parse_timed_lyrics(&data).unwrap(),
        [
            LyricLine {
                start_ms: 1250,
                end_ms: Some(2750),
                text: "First line".into()
            },
            LyricLine {
                start_ms: 3250,
                end_ms: None,
                text: "Second line".into()
            },
        ]
    );
    assert_eq!(
        parse_timed_lyrics(&json!({"timedLyricsModel": {"lyricsData": []}})),
        None
    );
}

#[test]
fn lrc_applies_offsets_expands_repeated_tags_and_strips_word_timing() {
    let lines = parse_lrc("[ar:Test artist]\n[offset:250]\n[00:04.25][00:02.500]<00:02.50>First <00:02.80>line\n[00:06.00]").unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.start_ms, line.end_ms, line.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (0, Some(2250), ""),
            (2250, Some(4000), "First line"),
            (4000, Some(5750), "First line"),
            (5750, Some(10750), "")
        ]
    );
    assert_eq!(parse_lrc("ordinary untimed text"), None);
}
