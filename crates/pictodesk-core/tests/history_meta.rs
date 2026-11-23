//! History metadata: the provenance log stored in `.pdk`.

use pictodesk_core::document::{format_history_time, Document, HistoryEvent};
use pictodesk_core::format;

#[test]
fn history_time_formats_utc() {
    assert_eq!(format_history_time(0), "1970-01-01 00:00");
    assert_eq!(format_history_time(1_763_904_000), "2025-11-23 13:20");
    assert_eq!(format_history_time(1_763_912_345), "2025-11-23 15:39");
}

#[test]
fn history_log_round_trips_through_pdk() {
    let mut doc = Document::new(8, 8);
    doc.history_log.push(HistoryEvent {
        when: 1_763_904_000,
        action: "Created".into(),
    });
    doc.log_event("Levels");
    doc.log_event("Saved");
    let bytes = format::serialize(&doc);
    let back = format::deserialize(&bytes).expect("load");
    assert_eq!(back.history_log.len(), 3);
    assert_eq!(back.history_log[0].action, "Created");
    assert_eq!(back.history_log[0].when, 1_763_904_000);
    assert_eq!(back.history_log[1].action, "Levels");
    assert_eq!(back.history_log[2].action, "Saved");
    assert!(back.history_log[1].when > 0);
}

#[test]
fn log_is_capped() {
    let mut doc = Document::new(4, 4);
    for i in 0..520 {
        doc.history_log.push(HistoryEvent {
            when: i,
            action: format!("e{i}"),
        });
    }
    // log_event enforces the cap on append.
    doc.log_event("latest");
    assert_eq!(doc.history_log.len(), 500);
    assert_eq!(doc.history_log.last().unwrap().action, "latest");
}
