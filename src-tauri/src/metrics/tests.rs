#![cfg(test)]

use super::{MetricsSampler, SampleEnvelope};
use chrono::Utc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

#[test]
fn envelope_fresh_preserves_metadata() {
    let start = Utc::now();
    let envelope = SampleEnvelope::fresh(
        42u8,
        start,
        Duration::from_millis(1500),
        Duration::from_millis(12),
        "unit-test",
    );
    assert_eq!(envelope.value, Some(42));
    assert_eq!(envelope.source, "unit-test");
    assert_eq!(envelope.valid_for_ms, 1500);
    assert_eq!(envelope.latency_ms, 12);
    assert!(envelope.error.is_none());
}

#[tokio::test]
async fn sampler_emits_recent_snapshot() {
    let sampler = MetricsSampler::spawn();
    sampler.wait_until_ready().await;

    let snapshot = timeout(Duration::from_secs(5), async {
        loop {
            let snap = sampler.latest_snapshot().await;
            if snap.memory.value.is_some() && snap.cpu.value.is_some() {
                break snap;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("sampler did not produce data in time");

    let now = Utc::now();
    let age = now.signed_duration_since(snapshot.captured_at);
    assert!(
        age.num_seconds() < 5,
        "snapshot too old: {}s",
        age.num_seconds()
    );

    drop(sampler);
}
