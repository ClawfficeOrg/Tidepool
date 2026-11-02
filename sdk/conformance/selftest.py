#!/usr/bin/env python3
"""Self-checks for the conformance harness itself.

These run without a pool and without any SDK binding: they pin the parts that
decide whether a binding passed, which is the part that would silently start
lying if it drifted. The scenario list is loaded against the placeholder
fixture so a broken scenario is caught here rather than in CI.

Run::

    python3 sdk/conformance/selftest.py

Exit code 0 means every check held.
"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))

import suite  # noqa: E402  (import after the path fix for script execution)

FIXTURES = Path(__file__).resolve().parent / "fixtures" / "variables.json"


class FakeAdapter:
    """An adapter that answers from a per-binding table, with no process."""

    replies: Dict[str, List[Any]] = {}
    seen: List[Dict[str, Any]] = []

    def __init__(self, binding: suite.Binding, base_url: str, **_: Any) -> None:
        self.binding = binding

    def start(self) -> None:
        """Nothing to start."""

    def close(self) -> None:
        """Nothing to stop."""

    def request(self, command: str, params: Mapping[str, Any]) -> Any:
        """Return the next scripted reply for this binding."""
        FakeAdapter.seen.append({"binding": self.binding.id, "params": dict(params)})
        scripted = FakeAdapter.replies.get(self.binding.id) or []
        if not scripted:
            raise AssertionError(f"no reply scripted for {self.binding.id}")
        return scripted.pop(0)


def binding(binding_id: str) -> suite.Binding:
    """One binding with a command that is never executed."""
    return suite.Binding(id=binding_id, language=binding_id, command=("noop",))


def step(
    step_id: str,
    *,
    command: str = "call",
    params: Optional[Dict[str, Any]] = None,
    expect: Any = None,
    agree: tuple = (),
    capture: Optional[Dict[str, str]] = None,
) -> suite.Step:
    """Build one step."""
    return suite.Step(
        id=step_id,
        command=command,
        params=params or {},
        expect=expect,
        agree=agree,
        capture=capture or {},
    )


def scenario(*steps: suite.Step) -> suite.Scenario:
    """Build one scenario around the given steps."""
    return suite.Scenario(
        id="selftest", title="selftest", goal="", steps=list(steps), source="selftest"
    )


class HarnessTest(unittest.TestCase):
    """Checks the verdict logic, not any one binding."""

    def setUp(self) -> None:
        self._adapter = suite.Adapter
        suite.Adapter = FakeAdapter  # type: ignore[assignment]
        FakeAdapter.replies = {}
        FakeAdapter.seen = []

    def tearDown(self) -> None:
        suite.Adapter = self._adapter  # type: ignore[assignment]

    def run_one(
        self,
        scenario_doc: suite.Scenario,
        replies: Dict[str, List[Any]],
        variables: Optional[Dict[str, Any]] = None,
    ) -> suite.Report:
        """Run one scenario against fake bindings."""
        FakeAdapter.replies = {key: list(value) for key, value in replies.items()}
        return suite.run_suite(
            [binding(name) for name in replies],
            [scenario_doc],
            variables or {},
            "http://127.0.0.1:1",
        )

    def test_a_diverging_error_code_fails_the_binding(self) -> None:
        """A binding that answers a different problem code fails."""
        doc = scenario(
            step("dispatch", expect={"outcome": "error", "code": "conflict"})
        )
        report = self.run_one(
            doc,
            {
                "python": [{"outcome": "error", "code": "conflict"}],
                "rust": [{"outcome": "error", "code": "gone"}],
            },
        )
        self.assertFalse(report.ok)
        failed = {outcome.binding for outcome in report.failures}
        self.assertEqual(failed, {"rust"})

    def test_a_diverging_paging_cursor_fails_every_binding_that_disagrees(self) -> None:
        """Bindings that return different cursors are all marked failing."""
        doc = scenario(
            step(
                "first-page",
                expect={"outcome": "ok"},
                agree=["body.next"],
            )
        )
        report = self.run_one(
            doc,
            {
                "python": [{"outcome": "ok", "body": {"next": 2}}],
                "typescript": [{"outcome": "ok", "body": {"next": 3}}],
            },
        )
        self.assertFalse(report.ok)
        messages = [
            failure for outcome in report.failures for failure in outcome.failures
        ]
        self.assertTrue(
            any("bindings disagree" in message for message in messages), messages
        )

    def test_a_capture_is_private_to_its_binding(self) -> None:
        """Two bindings capture their own ids and never read their sibling's."""
        doc = scenario(
            step("register", capture={"host_id": "body.id"}),
            step("mint", params={"host_id": "{{host_id}}"}, expect=None),
        )
        report = self.run_one(
            doc,
            {
                "python": [{"body": {"id": "python-host"}}, {"ok": True}],
                "rust": [{"body": {"id": "rust-host"}}, {"ok": True}],
            },
        )
        self.assertTrue(report.ok, report.summary())
        minted = [
            entry["params"].get("host_id")
            for entry in FakeAdapter.seen
            if "host_id" in entry["params"]
        ]
        self.assertEqual(minted, ["python-host", "rust-host"])

    def test_a_missing_binding_is_never_a_clean_pass(self) -> None:
        """Selecting a subset is reported as incomplete, not as a pass."""
        doc = scenario(step("contract", expect=None))
        report = self.run_one(
            doc,
            {
                "python": [{}],
                "rust": [{}],
            },
        )
        self.assertTrue(report.ok)
        FakeAdapter.replies = {"python": [{}], "rust": [{}]}
        partial = suite.run_suite(
            [binding("python"), binding("rust")],
            [scenario(step("contract", expect=None))],
            {},
            "http://127.0.0.1:1",
            selected=["python"],
        )
        self.assertFalse(partial.coverage_complete)
        self.assertFalse(partial.ok)
        self.assertIn("NOT A CLEAN PASS", suite.format_report(partial))

    def test_a_captured_value_can_be_interpolated_into_a_later_step(self) -> None:
        """Interpolation reads the binding's own scope."""
        doc = scenario(
            step("register", capture={"id": "body.id"}),
            step("read", params={"id": "{{id}}"}, expect={"outcome": "ok"}),
        )
        report = self.run_one(
            doc,
            {
                "python": [{"body": {"id": "abc"}}, {"outcome": "ok"}],
            },
        )
        self.assertTrue(report.ok, report.summary())


class ScenarioTest(unittest.TestCase):
    """Checks the shipped scenario documents."""

    def test_every_scenario_loads_against_the_fixture(self) -> None:
        """The committed scenarios parse and every template resolves."""
        variables = json.loads(FIXTURES.read_text(encoding="utf-8"))
        scenarios = suite.load_scenarios(suite.DEFAULT_SCENARIO_DIR, variables)
        self.assertGreaterEqual(len(scenarios), 8)
        for loaded in scenarios:
            self.assertTrue(loaded.steps, loaded.id)

    def test_the_manifest_requires_all_three_bindings(self) -> None:
        """A missing binding is a missing binding, not a silent skip."""
        bindings = suite.load_manifest(suite.DEFAULT_MANIFEST)
        self.assertEqual(
            sorted(binding.id for binding in bindings),
            ["python", "rust", "typescript"],
        )

    def test_an_unknown_operator_is_a_suite_error(self) -> None:
        """A typo in an expectation cannot pass silently."""
        with self.assertRaises(suite.SuiteError):
            suite.match_expectation({"$nonsense": 1}, 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
