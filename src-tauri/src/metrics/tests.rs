#![cfg(test)]

use super::{MetricsSampler, SampleEnvelope};
use chrono::Utc;
use std::time::Duration;

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

    let snapshot = sampler.latest_snapshot().await;

    let now = Utc::now();
    let age = now.signed_duration_since(snapshot.captured_at);
    assert!(
        age.num_seconds() < 5,
        "snapshot too old: {}s",
        age.num_seconds()
    );

    let cpu = &snapshot.cpu;
    assert!(
        cpu.value.is_some() || cpu.error.is_some(),
        "cpu sample missing value and error"
    );

    let memory = &snapshot.memory;
    assert!(
        memory.value.is_some() || memory.error.is_some(),
        "memory sample missing value and error"
    );

    let disks = &snapshot.disks;
    assert!(
        disks.value.is_some() || disks.error.is_some(),
        "disks sample missing value and error"
    );

    let uptime = &snapshot.uptime;
    assert!(
        uptime.value.is_some() || uptime.error.is_some(),
        "uptime sample missing value and error"
    );

    drop(sampler);
}
