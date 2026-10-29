#!/usr/bin/env python3
"""Run the cross-SDK conformance suite.

The suite sends one scenario list to every SDK binding and fails when a binding
answers differently from the contract or from its siblings. See
``sdk/conformance/README.md`` for the protocol and the scenario format.

Examples::

    python3 sdk/conformance/run_conformance.py --list
    python3 sdk/conformance/run_conformance.py \
        --base-url http://127.0.0.1:8080 --vars /tmp/conformance-state.json
    python3 sdk/conformance/run_conformance.py --base-url ... --vars ... \
        --bindings rust --scenario auth.contract   # debugging only

Exit codes: 0 clean pass with full coverage; 1 a binding failed; 2 the suite
could not run (bad manifest, scenario, variables or adapter); 3 incomplete
coverage (``--bindings`` was used), which is never a clean pass.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import suite  # noqa: E402  (import after the path fix for script execution)


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="run_conformance.py",
        description="Run one scenario list against every SDK binding.",
    )
    parser.add_argument(
        "--base-url",
        help="absolute pool URL every adapter talks to (required to run)",
    )
    parser.add_argument(
        "--vars",
        type=Path,
        help="JSON file of scenario variables produced by the fixture/provisioner",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=suite.DEFAULT_MANIFEST,
        help="adapter manifest (default: sdk/conformance/adapters/manifest.json)",
    )
    parser.add_argument(
        "--scenarios",
        type=Path,
        default=suite.DEFAULT_SCENARIO_DIR,
        help="directory of scenario JSON files",
    )
    parser.add_argument(
        "--bindings",
        help="comma-separated subset for debugging; marks the run incomplete",
    )
    parser.add_argument(
        "--scenario",
        action="append",
        default=[],
        dest="scenario_ids",
        help="run only this scenario id (repeatable, debugging only)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=180.0,
        help="seconds to wait for each adapter response (default: 180)",
    )
    parser.add_argument(
        "--json",
        type=Path,
        help="write the machine-readable report to this path",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list scenarios and bindings, then exit",
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="only print the verdict and failures",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Run the suite and translate the verdict into an exit code."""
    args = _parse_args(argv)
    try:
        bindings = suite.load_manifest(args.manifest)
        if args.list:
            print("bindings:")
            for binding in bindings:
                print(f"  {binding.id} ({binding.language})")
            variables = {}
            if args.vars:
                variables = _load_variables(args.vars)
            print("scenarios:")
            for scenario in suite.load_scenarios(args.scenarios, variables):
                print(f"  {scenario.id}: {scenario.title}")
            return 0
        if not args.base_url:
            raise suite.SuiteError("--base-url is required (or use --list)")
        if not args.vars:
            raise suite.SuiteError(
                "--vars is required: the scenario variables come from the fixture"
            )
        variables = _load_variables(args.vars)
        scenarios = suite.load_scenarios(args.scenarios, variables)
        if args.scenario_ids:
            wanted = set(args.scenario_ids)
            unknown = sorted(wanted - {scenario.id for scenario in scenarios})
            if unknown:
                raise suite.SuiteError(
                    "unknown scenario id(s): " + ", ".join(unknown)
                )
            scenarios = [
                scenario for scenario in scenarios if scenario.id in wanted
            ]
        _require_executables(bindings)
        selected = None
        if args.bindings:
            selected = [part.strip() for part in args.bindings.split(",") if part.strip()]
        report = suite.run_suite(
            bindings,
            scenarios,
            variables,
            args.base_url,
            selected=selected,
            timeout=args.timeout,
        )
    except suite.SuiteError as error:
        print(f"conformance: {error}", file=sys.stderr)
        return 2
    if args.json:
        args.json.write_text(
            json.dumps(report.summary(), indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    if not args.quiet or not report.ok:
        print(suite.format_report(report))
    if not report.coverage_complete:
        return 3
    return 0 if report.ok else 1


def _load_variables(path: Path) -> dict:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise suite.SuiteError(f"cannot read variables file {path}: {error}") from error
    except ValueError as error:
        raise suite.SuiteError(f"variables file {path} is not JSON: {error}") from error
    if not isinstance(document, dict):
        raise suite.SuiteError(f"variables file {path} must hold one JSON object")
    return document


def _require_executables(bindings: list[suite.Binding]) -> None:
    """Fail early, with the build hint, when a binding's adapter is missing."""
    import os

    for binding in bindings:
        command = binding.command[0]
        if os.sep not in command:
            continue
        path = Path(command)
        if not path.is_absolute():
            path = suite.REPOSITORY / path
        if not path.exists():
            hint = ""
            if binding.id == "rust":
                hint = (
                    "; build it with: cargo build --manifest-path "
                    "sdk/conformance/adapters/rust/Cargo.toml"
                )
            raise suite.SuiteError(
                f"binding {binding.id!r} adapter not found at {path}{hint}"
            )


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
