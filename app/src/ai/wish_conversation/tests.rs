//! Integration-style tests that exercise the StubAdapter →
//! sink → InFlight pipeline as a unit. Tests that the chunks
//! emitted by the adapter, when fed sequentially into
//! `InFlight::apply`, produce the expected committed blocks.
//!
//! Pure-logic — no `AppContext`, no `ModelContext`. Demonstrates
//! the contract that the future Hermon and LocalLlm adapters
//! must satisfy.

use super::adapter::{ConversationAdapter, StubAdapter};
use super::model::InFlight;
use super::types::{Conversation, ConversationId, MessageBlock, StreamChunk};
use std::sync::{Arc, Mutex};

#[test]
fn stub_adapter_drives_in_flight_to_a_committed_text_block() {
    // Set up: empty conversation, a fresh InFlight buffer.
    let conv = Conversation::new(ConversationId::new("c"), "wish-coder".into());
    let in_flight = Arc::new(Mutex::new(InFlight::default()));

    // Ad-hoc sink that pipes chunks straight into the in-flight
    // buffer. Tracks whether a commit was triggered.
    let committed = Arc::new(Mutex::new(false));
    let in_flight_for_sink = in_flight.clone();
    let committed_for_sink = committed.clone();
    let sink = Box::new(move |chunk: StreamChunk| {
        let mut buf = in_flight_for_sink.lock().unwrap();
        if buf.apply(&chunk) {
            *committed_for_sink.lock().unwrap() = true;
        }
    });

    // Run the adapter.
    let adapter = StubAdapter::new();
    adapter.send(&conv, "hello", sink);

    // Assertions.
    assert!(*committed.lock().unwrap(), "stub adapter must emit Done");
    let mut buf = in_flight.lock().unwrap();
    let blocks = buf.take_blocks();
    assert_eq!(blocks.len(), 1);
    assert!(matches!(
        &blocks[0],
        MessageBlock::Text { text } if text == "You said: hello"
    ));
}

#[test]
fn stub_adapter_with_custom_prefix_round_trips() {
    let conv = Conversation::new(ConversationId::new("c"), "wish-coder".into());
    let in_flight = Arc::new(Mutex::new(InFlight::default()));
    let in_flight_for_sink = in_flight.clone();
    let sink = Box::new(move |chunk: StreamChunk| {
        in_flight_for_sink.lock().unwrap().apply(&chunk);
    });

    let adapter = StubAdapter::with_prefix(">> ");
    adapter.send(&conv, "ping", sink);

    let mut buf = in_flight.lock().unwrap();
    let blocks = buf.take_blocks();
    assert_eq!(blocks.len(), 1);
    assert!(matches!(
        &blocks[0],
        MessageBlock::Text { text } if text == ">> ping"
    ));
}

#[test]
fn adapter_label_is_human_readable() {
    // Sanity check the label contract — the chat header will
    // display this verbatim, so it must be non-empty.
    let stub = StubAdapter::new();
    let label = stub.label();
    assert!(!label.is_empty());
}
