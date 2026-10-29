"""The retry policy: defaults, backoff, Retry-After and the ceiling."""

from __future__ import annotations

import pytest

from tidepool_client import RetryAfterTooLongError, RetryPolicy


def test_defaults_are_bounded_and_match_the_other_sdks():
    policy = RetryPolicy()
    assert policy.max_retries == 2
    assert policy.base_delay_ms == 250
    assert policy.max_delay_ms == 4_000
    assert policy.max_retry_after_ms == 60_000


def test_exponential_backoff_is_capped():
    policy = RetryPolicy()
    assert policy.delay_seconds(0) == 0.25
    assert policy.delay_seconds(1) == 0.5
    assert policy.delay_seconds(9) == 4.0


def test_retry_after_replaces_backoff():
    assert RetryPolicy().delay_seconds(0, 120) == 0.12


def test_an_excessive_retry_after_is_an_error():
    with pytest.raises(RetryAfterTooLongError) as caught:
        RetryPolicy().delay_seconds(0, 60_001, operation_id="get_v1_hosts")
    assert caught.value.requested_ms == 60_001
    assert caught.value.limit_ms == 60_000


def test_jitter_stays_within_the_cap():
    policy = RetryPolicy(jitter=True)
    delay = policy.delay_seconds(2)
    assert 0.5 <= delay <= 4.0


def test_none_disables_retries_but_keeps_the_policy():
    policy = RetryPolicy.none()
    assert policy.max_retries == 0
    assert policy.max_delay_ms == 4_000


def test_negative_values_are_rejected():
    with pytest.raises(ValueError):
        RetryPolicy(max_retries=-1)
    with pytest.raises(ValueError):
        RetryPolicy(base_delay_ms=-1)
